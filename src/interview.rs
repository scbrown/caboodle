use std::{
    fs,
    io::{BufRead, Write},
    path::Path,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{
    CrewMemberTheme, CrewMode, InstallIntent, Plan, Profile, QuestionContract, SCHEMA_VERSION,
};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    schema_version: u32,
    profile: Option<Profile>,
    crew: Option<CrewMode>,
    intended_use: Option<String>,
    crew_count: Option<usize>,
    #[serde(default)]
    crew_members: Vec<CrewMemberTheme>,
    member_name: Option<String>,
    member_theme: Option<String>,
    member_domain: Option<String>,
    question_count: Option<usize>,
    #[serde(default)]
    anticipated_questions: Vec<QuestionContract>,
    question: Option<String>,
    answer_shape: Option<String>,
    seed_intent: Option<String>,
    sparql: Option<String>,
}

impl Draft {
    fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                schema_version: SCHEMA_VERSION,
                ..Self::default()
            });
        }
        let body = fs::read_to_string(path)
            .with_context(|| format!("read interview session {}", path.display()))?;
        let draft: Self = toml::from_str(&body)
            .with_context(|| format!("parse interview session {}", path.display()))?;
        if draft.schema_version != SCHEMA_VERSION {
            bail!(
                "unsupported interview schema {}; expected {}",
                draft.schema_version,
                SCHEMA_VERSION
            );
        }
        Ok(draft)
    }

    fn write(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("create interview directory {}", parent.display()))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("create temporary interview in {}", parent.display()))?;
        tmp.write_all(toml::to_string_pretty(self)?.as_bytes())
            .context("serialize interview session")?;
        tmp.persist(path)
            .map_err(|error| error.error)
            .with_context(|| format!("replace interview session {}", path.display()))?;
        Ok(())
    }
}

pub fn guided<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    session_path: &Path,
    plan_path: &Path,
) -> Result<Plan> {
    let mut draft = Draft::read(session_path)?;
    draft.write(session_path)?;

    if draft.profile.is_none() {
        let answer = ask(input, output, "profile [kg/retrieval/crew]: ", session_path)?;
        draft.profile = Some(parse_profile(&answer)?);
        draft.write(session_path)?;
    } else {
        writeln!(output, "resuming: profile already answered")?;
    }

    let profile = draft
        .profile
        .context("interview profile was not recorded")?;
    if profile == Profile::Crew && draft.crew.is_none() {
        let answer = ask(
            input,
            output,
            "crew [shantytown/creel/both/standalone]: ",
            session_path,
        )?;
        draft.crew = Some(parse_crew(&answer)?);
        draft.write(session_path)?;
    }

    if draft.intended_use.is_none() {
        draft.intended_use = Some(ask(
            input,
            output,
            "what should this installation help you do? ",
            session_path,
        )?);
        draft.write(session_path)?;
    }
    if draft.crew_count.is_none() {
        draft.crew_count = Some(parse_count(
            &ask(
                input,
                output,
                "how many themed crew members should be configured? ",
                session_path,
            )?,
            "crew member",
        )?);
        draft.write(session_path)?;
    }
    while draft.crew_members.len() < draft.crew_count.unwrap_or(0) {
        let ordinal = draft.crew_members.len() + 1;
        if draft.member_name.is_none() {
            draft.member_name = Some(ask(
                input,
                output,
                &format!("crew member {ordinal} name: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        if draft.member_theme.is_none() {
            draft.member_theme = Some(ask(
                input,
                output,
                &format!("crew member {ordinal} theme or identity: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        if draft.member_domain.is_none() {
            draft.member_domain = Some(ask(
                input,
                output,
                &format!("crew member {ordinal} knowledge domain: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        let role = ask(
            input,
            output,
            &format!("crew member {ordinal} role and responsibilities: "),
            session_path,
        )?;
        draft.crew_members.push(CrewMemberTheme {
            name: draft.member_name.take().unwrap(),
            theme: draft.member_theme.take().unwrap(),
            domain: draft.member_domain.take().unwrap(),
            role,
        });
        draft.write(session_path)?;
    }
    if draft.question_count.is_none() {
        draft.question_count = Some(parse_count(
            &ask(
                input,
                output,
                "how many ontology questions must the finished graph answer? ",
                session_path,
            )?,
            "anticipated question",
        )?);
        if draft.question_count == Some(0) {
            bail!("at least one anticipated ontology question is required");
        }
        draft.write(session_path)?;
    }
    while draft.anticipated_questions.len() < draft.question_count.unwrap_or(0) {
        let ordinal = draft.anticipated_questions.len() + 1;
        if draft.question.is_none() {
            draft.question = Some(ask(
                input,
                output,
                &format!("anticipated question {ordinal}: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        if draft.answer_shape.is_none() {
            draft.answer_shape = Some(ask(
                input,
                output,
                &format!("question {ordinal} expected answer shape: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        if draft.seed_intent.is_none() {
            draft.seed_intent = Some(ask(
                input,
                output,
                &format!("question {ordinal} fixture or seed facts: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        if draft.sparql.is_none() {
            draft.sparql = Some(ask(
                input,
                output,
                &format!("question {ordinal} executable SELECT/ASK SPARQL: "),
                session_path,
            )?);
            draft.write(session_path)?;
        }
        let expected = ask(
            input,
            output,
            &format!("question {ordinal} expected result marker: "),
            session_path,
        )?;
        draft.anticipated_questions.push(QuestionContract {
            question: draft.question.take().unwrap(),
            answer_shape: draft.answer_shape.take().unwrap(),
            seed_intent: draft.seed_intent.take().unwrap(),
            sparql: draft.sparql.take().unwrap(),
            expected,
        });
        draft.write(session_path)?;
    }

    let mut plan = if profile == Profile::Crew {
        Plan::for_crew(draft.crew.context("crew profile requires a crew answer")?)
    } else {
        Plan::for_profile(profile)
    };
    plan.intent = Some(InstallIntent {
        intended_use: draft.intended_use.clone().unwrap(),
        crew_members: draft.crew_members.clone(),
        anticipated_questions: draft.anticipated_questions.clone(),
    });
    plan.write(plan_path)?;
    fs::remove_file(session_path)
        .with_context(|| format!("remove completed interview {}", session_path.display()))?;
    writeln!(output, "plan: {}", plan_path.display())?;
    Ok(plan)
}

fn ask<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
    session_path: &Path,
) -> Result<String> {
    write!(output, "{prompt}")?;
    output.flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 {
        bail!(
            "interview paused at end of input; resume with --session {}",
            session_path.display()
        );
    }
    Ok(answer.trim().to_owned())
}

fn parse_profile(answer: &str) -> Result<Profile> {
    match answer.to_ascii_lowercase().as_str() {
        "kg" => Ok(Profile::Kg),
        "retrieval" => Ok(Profile::Retrieval),
        "crew" => Ok(Profile::Crew),
        _ => bail!("invalid profile {answer:?}; expected kg, retrieval, or crew"),
    }
}

fn parse_count(answer: &str, noun: &str) -> Result<usize> {
    answer
        .parse()
        .with_context(|| format!("{noun} count must be a non-negative integer"))
}

fn parse_crew(answer: &str) -> Result<CrewMode> {
    match answer.to_ascii_lowercase().as_str() {
        "shantytown" => Ok(CrewMode::Shantytown),
        "creel" => Ok(CrewMode::Creel),
        "both" => Ok(CrewMode::Both),
        "standalone" => Ok(CrewMode::Standalone),
        _ => bail!("invalid crew mode {answer:?}; expected shantytown, creel, both, or standalone"),
    }
}
