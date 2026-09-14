//! Durable rollback point for an updater interrupted between rename and verification.
use super::{atomic_copy, hash};
use crate::model::State;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
pub(super) struct Pending {
    pub tool: String,
    pub destination: PathBuf,
    pub backup: PathBuf,
    pub sha256: String,
}

fn journal(state: &Path) -> PathBuf {
    state.with_extension("release-pending.json")
}

pub(super) fn begin(state: &Path, pending: &Pending) -> Result<()> {
    let target = journal(state);
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file = tempfile::NamedTempFile::new_in(parent)?;
    fs::write(file.path(), serde_json::to_vec(pending)?)?;
    file.as_file().sync_all()?;
    file.persist(target).map_err(|e| e.error)?;
    Ok(())
}

pub(super) fn finish(state: &Path) -> Result<()> {
    fs::remove_file(journal(state)).context("clear completed release transaction")
}

pub(super) fn recover(state: &Path) -> Result<()> {
    let journal = journal(state);
    if !journal.exists() {
        return Ok(());
    }
    let pending: Pending = serde_json::from_slice(&fs::read(&journal)?)?;
    if hash(&pending.backup)? != pending.sha256 {
        bail!("pending rollback backup checksum mismatch; refusing recovery");
    }
    atomic_copy(&pending.backup, &pending.destination)
        .context("restore interrupted release update")?;
    if hash(&pending.destination)? != pending.sha256 {
        bail!("interrupted release rollback read-back mismatch");
    }
    // A previous state write might have completed before the process died. It
    // cannot certify the restored binary; make it earn functional proof again.
    let mut previous = State::read(state)?;
    previous.tools.remove(&pending.tool);
    previous.write(state)?;
    fs::remove_file(journal)?;
    println!(
        "{}: recovered interrupted update from {}",
        pending.tool,
        pending.backup.display()
    );
    Ok(())
}
