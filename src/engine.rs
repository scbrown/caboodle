use std::{ffi::OsString, path::Path};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::{
    adapter::adapter,
    crew::{self, CrewEvidence},
    emission,
    model::{Plan, ShareState, State, ToolState},
};

#[derive(Deserialize)]
struct ImportPromotion {
    eligible: bool,
    blockers: Vec<String>,
}

#[derive(Deserialize)]
struct ImportResult {
    outcome: String,
    share_id: String,
    staging_graph: String,
    promotion: ImportPromotion,
}

pub fn apply(plan: &Plan, state_path: &Path, skip_install: bool) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
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
        emission::queue_transition(
            state_path,
            name.as_str(),
            "applied",
            &state.tools[name.as_str()].version,
        )?;
        println!("{}: applied", name.as_str());
    }
    if let Some(selection) = &plan.crew {
        crew::apply(selection, &mut state, skip_install)?;
        state.write(state_path)?;
        for (name, runtime) in &state.crew {
            emission::queue_transition(state_path, name, "applied", &runtime.version)?;
        }
    }
    consume_shares(plan, &mut state, state_path)?;
    Ok(state)
}

fn consume_shares(plan: &Plan, state: &mut State, state_path: &Path) -> Result<()> {
    let Some(db) = plan.quipu_db.as_deref() else {
        return Ok(());
    };
    for share in &plan.shares {
        let result = crate::adapter::checked(
            "quipu",
            [
                OsString::from("import"),
                share.as_os_str().to_owned(),
                OsString::from("--db"),
                db.as_os_str().to_owned(),
            ],
            None,
        )
        .with_context(|| format!("import canonical Quipu share {}", share.display()))?;
        let imported: ImportResult = serde_json::from_slice(&result.stdout)
            .with_context(|| format!("parse Quipu import result for {}", share.display()))?;
        if !matches!(
            imported.outcome.as_str(),
            "staged" | "quarantined" | "unchanged"
        ) {
            bail!(
                "Quipu returned unknown share import outcome {:?} for {}",
                imported.outcome,
                share.display()
            );
        }
        state.shares.insert(
            imported.share_id.clone(),
            ShareState {
                path: share.clone(),
                staging_graph: imported.staging_graph,
                outcome: imported.outcome.clone(),
                promotion_eligible: imported.promotion.eligible,
                blockers: imported.promotion.blockers,
            },
        );
        state.write(state_path)?;
        println!(
            "share {}: {} (promotion eligible: {})",
            imported.share_id, imported.outcome, imported.promotion.eligible
        );
    }
    Ok(())
}

pub fn verify(plan: &Plan, state_path: &Path, evidence: &CrewEvidence) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
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
        emission::queue_transition(
            state_path,
            name.as_str(),
            "verified",
            &state.tools[name.as_str()].version,
        )?;
        println!("{}: verified", name.as_str());
    }
    if let Some(selection) = &plan.crew {
        crew::verify(selection, evidence, &mut state)?;
        state.write(state_path)?;
        for (name, runtime) in &state.crew {
            emission::queue_transition(state_path, name, "verified", &runtime.version)?;
        }
    }
    Ok(state)
}

/// Compare the running toolchain to the releases reviewed by this Caboodle
/// build. Returns true only when every selected tool is current.
pub fn check_updates(plan: &Plan) -> Result<bool> {
    plan.validate()?;
    let mut current = true;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
        let desired = adapter.desired_version();
        match adapter.version() {
            Ok(installed) if adapter.is_current(&installed) => {
                println!("{}: current ({installed})", name.as_str());
            }
            Ok(installed) => {
                current = false;
                println!(
                    "{}: update available (installed: {installed}; reviewed: {desired})",
                    name.as_str()
                );
            }
            Err(error) => {
                current = false;
                println!(
                    "{}: missing or unreadable ({error:#}); reviewed: {desired}",
                    name.as_str()
                );
            }
        }
    }
    if let Some(selection) = &plan.crew {
        current &= crew::check_updates(selection);
    }
    Ok(current)
}

/// Converge drifted tools to the reviewed releases and functionally verify
/// each changed tool before recording it as current.
pub fn update(plan: &Plan, state_path: &Path, evidence: &CrewEvidence) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
        let before = adapter.version().ok();
        if before
            .as_deref()
            .is_some_and(|installed| adapter.is_current(installed))
        {
            println!("{}: current", name.as_str());
            continue;
        }
        eprintln!(
            "{}: converging {:?} -> {}",
            name.as_str(),
            before,
            adapter.desired_version()
        );
        adapter
            .install()
            .with_context(|| format!("{} reviewed update install", name.as_str()))?;
        let version = adapter
            .version()
            .with_context(|| format!("{} version read-back after update", name.as_str()))?;
        if !adapter.is_current(&version) {
            bail!(
                "{} update did not reach reviewed release {} (got {})",
                name.as_str(),
                adapter.desired_version(),
                version
            );
        }
        adapter
            .verify()
            .with_context(|| format!("{} verification after update", name.as_str()))?;
        state.tools.insert(
            name.as_str().to_owned(),
            ToolState {
                version: version.clone(),
                applied: true,
                verified: true,
            },
        );
        state.write(state_path)?;
        emission::queue_transition(state_path, name.as_str(), "updated", &version)?;
        println!("{}: updated and verified", name.as_str());
    }
    if let Some(selection) = &plan.crew {
        if !crew::check_updates(selection) {
            crew::apply(selection, &mut state, false)?;
            crew::verify(selection, evidence, &mut state)?;
            state.write(state_path)?;
            for (name, runtime) in &state.crew {
                emission::queue_transition(state_path, name, "updated", &runtime.version)?;
            }
        }
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
