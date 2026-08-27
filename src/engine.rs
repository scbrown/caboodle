use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    adapter::adapter,
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
    Ok(state)
}

pub fn verify(plan: &Plan, state_path: &Path) -> Result<State> {
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
    Ok(state)
}
