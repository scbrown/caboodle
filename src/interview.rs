use std::{
    fs,
    io::{BufRead, Write},
    path::Path,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{CrewMode, Plan, Profile, SCHEMA_VERSION};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    schema_version: u32,
    profile: Option<Profile>,
    crew: Option<CrewMode>,
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

    let plan = if profile == Profile::Crew {
        Plan::for_crew(draft.crew.context("crew profile requires a crew answer")?)
    } else {
        Plan::for_profile(profile)
    };
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
    Ok(answer.trim().to_ascii_lowercase())
}

fn parse_profile(answer: &str) -> Result<Profile> {
    match answer {
        "kg" => Ok(Profile::Kg),
        "retrieval" => Ok(Profile::Retrieval),
        "crew" => Ok(Profile::Crew),
        _ => bail!("invalid profile {answer:?}; expected kg, retrieval, or crew"),
    }
}

fn parse_crew(answer: &str) -> Result<CrewMode> {
    match answer {
        "shantytown" => Ok(CrewMode::Shantytown),
        "creel" => Ok(CrewMode::Creel),
        "both" => Ok(CrewMode::Both),
        "standalone" => Ok(CrewMode::Standalone),
        _ => bail!("invalid crew mode {answer:?}; expected shantytown, creel, both, or standalone"),
    }
}
