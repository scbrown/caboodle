use std::{ffi::OsString, path::Path};

use anyhow::{Context, Result};

use crate::{
    adapter::adapter,
    crew::{self, CrewEvidence},
    model::{Plan, State, ToolState},
};

pub fn apply(plan: &Plan, state_path: &Path, skip_install: bool) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name);
        let version = match adapter.version() {
            Ok(version) => version,
            Err(error) if !skip_install => {
                eprintln!("{}: not installed ({error:#}); installing", name.as_str());
                adapter
                    .install()
                    .with_context(|| format!("{} install step", name.as_str()))?;
                adapter
                    .version()
                    .with_context(|| format!("{} version read-back after install", name.as_str()))?
            }
            Err(error) => {
                return Err(error).context(format!("{} version read-back", name.as_str()))
            }
        };
        let remains_verified = state
            .tools
            .get(name.as_str())
            .is_some_and(|previous| previous.version == version && previous.verified);
        state.tools.insert(
            name.as_str().to_owned(),
            ToolState {
                version,
                applied: true,
                verified: remains_verified,
            },
        );
        state.write(state_path)?;
        println!("{}: applied", name.as_str());
    }
    if let Some(selection) = &plan.crew {
        crew::apply(selection, &mut state, skip_install)?;
        state.write(state_path)?;
    }
    Ok(state)
}

pub fn verify(plan: &Plan, state_path: &Path, evidence: &CrewEvidence) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name);
        let version = adapter
            .version()
            .with_context(|| format!("{} version read-back", name.as_str()))?;
        adapter
            .verify()
            .with_context(|| format!("{} functional verification", name.as_str()))?;
        state.tools.insert(
            name.as_str().to_owned(),
            ToolState {
                version,
                applied: true,
                verified: true,
            },
        );
        state.write(state_path)?;
        println!("{}: verified", name.as_str());
    }
    if let Some(selection) = &plan.crew {
        crew::verify(selection, evidence, &mut state)?;
        state.write(state_path)?;
    }
    Ok(state)
}

pub fn verify_questions(plan: &Plan, db: Option<&Path>) -> Result<()> {
    plan.validate()?;
    let intent = plan.intent.as_ref().context(
        "plan has no intended-use/question contract; regenerate it through the Phase 2 interview",
    )?;
    for (index, contract) in intent.anticipated_questions.iter().enumerate() {
        let mut args = vec![OsString::from("read"), OsString::from(&contract.sparql)];
        if let Some(db) = db {
            args.push(OsString::from("--db"));
            args.push(db.as_os_str().to_owned());
        }
        let result = crate::adapter::checked("quipu", args, None)
            .with_context(|| format!("anticipated question {} query", index + 1))?;
        let answer = String::from_utf8_lossy(&result.stdout);
        if !answer.contains(&contract.expected) {
            anyhow::bail!(
                "anticipated question {} was executable but its answer did not contain {:?}: {}",
                index + 1,
                contract.expected,
                contract.question
            );
        }
        println!("question {}: verified — {}", index + 1, contract.question);
    }
    Ok(())
}
