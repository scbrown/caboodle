//! Durable rollback point for an updater interrupted between rename and verification.
use super::{atomic_copy, durable, hash};
use crate::model::State;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
pub(super) struct Pending {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_alias: Option<String>,
    pub tool: String,
    pub destination: PathBuf,
    pub backup: PathBuf,
    pub sha256: String,
}

fn is_false(value: &bool) -> bool {
    !value
}

fn journal(state: &Path) -> PathBuf {
    state.with_extension("release-pending.json")
}

pub(super) fn begin(state: &Path, pending: &Pending) -> Result<()> {
    if pending.restore_alias.is_some() {
        bail!("single artifact journal cannot restore an alias");
    }
    durable::write(&journal(state), &serde_json::to_vec(pending)?)
}

pub(super) fn finish(state: &Path) -> Result<()> {
    let target = journal(state);
    let bytes = fs::read(&target)?;
    fs::remove_file(&target).context("clear completed release transaction")?;
    let cleared = (|| {
        durable::checkpoint("journal-unlinked", &target)?;
        durable::sync_dir(durable::parent(&target))?;
        durable::checkpoint("journal-clear-durable", &target)
    })();
    if let Err(error) = cleared {
        // Do not certify an unlink whose persistence is unknown. Retain the
        // exact pending record when possible, so a later invocation recovers.
        let retained = durable::write(&target, &bytes);
        return Err(error).context(format!(
            "journal clear durability uncertain; pending retention: {retained:?}"
        ));
    }
    Ok(())
}

pub(super) fn recover(state: &Path) -> Result<()> {
    let journal = journal(state);
    match fs::symlink_metadata(&journal) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("cannot establish pending release journal state"),
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            bail!("pending release journal must be a regular non-symlink file; refusing recovery")
        }
    }
    let bytes = fs::read(&journal)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if value.get("artifacts").is_some() {
        let pair: PairPending = serde_json::from_slice(&bytes)?;
        return restore_pair(state, &pair);
    }
    let pending: Pending = serde_json::from_slice(&bytes)?;
    if pending.restore_alias.is_some() {
        bail!("alias recovery requires its versioned Quipu pair journal");
    }
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
    fs::File::open(state)?.sync_all()?;
    durable::sync_dir(durable::parent(state))?;
    finish(state)?;
    println!(
        "{}: recovered interrupted update from {}",
        pending.tool,
        pending.backup.display()
    );
    Ok(())
}

/// Paired releases share the SAME lock and pending journal as single binaries.
/// Keep the old format readable; a pair must restore all artifacts and exact state.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PairPending {
    #[serde(default, skip_serializing_if = "is_false")]
    pub runtime_aliases: bool,
    pub artifacts: Vec<Pending>,
    pub previous_state: Option<Vec<u8>>,
}

pub(super) fn begin_pair(state: &Path, pending: &PairPending) -> Result<()> {
    if pending.artifacts.len() != 2 {
        bail!("paired release must contain exactly two artifacts");
    }
    validate_alias_targets(pending)?;
    let target = journal(state);
    durable::write(&target, &serde_json::to_vec(pending)?)?;
    durable::checkpoint("pair-journal-durable", &target)
}

fn validate_alias_targets(p: &PairPending) -> Result<()> {
    if !p.runtime_aliases {
        if p.artifacts.iter().any(|a| a.restore_alias.is_some()) {
            bail!("alias metadata requires versioned pair journal");
        }
        return Ok(());
    }
    if p.artifacts.len() != 2
        || p.artifacts.iter().any(|a| a.tool != "quipu")
        || p.artifacts[0].destination.file_name() != Some(std::ffi::OsStr::new("quipu"))
        || p.artifacts[1].destination.file_name() != Some(std::ffi::OsStr::new("quipu-server"))
        || p.artifacts[0].destination.parent() != p.artifacts[1].destination.parent()
        || p.artifacts[0].restore_alias.is_some()
        || p.artifacts[1]
            .restore_alias
            .as_deref()
            .is_some_and(|s| s != "quipu")
    {
        bail!("alias journal requires the exact same-directory owned Quipu layout");
    }
    Ok(())
}

fn restore_pair(state: &Path, pending: &PairPending) -> Result<()> {
    if pending.artifacts.len() != 2
        || pending.artifacts[0].destination == pending.artifacts[1].destination
    {
        bail!("invalid paired release journal");
    }
    validate_alias_targets(pending)?;
    // Validate ALL backup bytes before restoring any of them.
    for (i, artifact) in pending.artifacts.iter().enumerate() {
        if !fs::symlink_metadata(&artifact.backup)?
            .file_type()
            .is_file()
        {
            bail!("paired recovery requires regular backup files");
        }
        if !fs::symlink_metadata(&artifact.destination)?
            .file_type()
            .is_file()
            && (!pending.runtime_aliases
                || i != 1
                || crate::quipu_layout::alias(&artifact.destination)?.as_deref() != Some("quipu"))
        {
            bail!(
                "paired recovery requires regular non-symlink files or its exact versioned alias"
            );
        }
        if hash(&artifact.backup)? != artifact.sha256 {
            bail!("paired rollback backup checksum mismatch; refusing recovery");
        }
    }
    // Restore the companion/alias before the CLI; both old and new MCP remain coherent.
    for artifact in pending.artifacts.iter().rev() {
        if artifact.restore_alias.is_some() {
            crate::quipu_layout::replace_alias(&artifact.destination)?;
        } else {
            atomic_copy(&artifact.backup, &artifact.destination)?;
        }
        durable::sync_dir(durable::parent(&artifact.destination))?;
    }
    // An alias points to the primary, which may still have candidate bytes until
    // every restore has completed. Verify the complete layout only afterwards.
    for artifact in &pending.artifacts {
        if hash(&artifact.destination)? != artifact.sha256 {
            bail!("paired rollback read-back mismatch");
        }
        if pending.runtime_aliases
            && crate::quipu_layout::alias(&artifact.destination)? != artifact.restore_alias
        {
            bail!("paired rollback alias layout mismatch");
        }
    }
    match &pending.previous_state {
        Some(bytes) => durable::write(state, bytes)?,
        None => match fs::remove_file(state) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        },
    }
    durable::sync_dir(durable::parent(state))?;
    finish(state)?;
    println!(
        "{}: restored interrupted/failed paired release and prior state",
        pending.artifacts[0].tool
    );
    Ok(())
}

#[cfg(test)]
mod journal_tests {
    use super::*;

    #[test]
    fn dangling_or_nonregular_pending_journals_refuse_and_remain() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state.json");
        let journal = state.with_extension("release-pending.json");
        std::os::unix::fs::symlink(root.path().join("missing"), &journal).unwrap();
        assert!(recover(&state)
            .unwrap_err()
            .to_string()
            .contains("regular non-symlink"));
        assert!(fs::symlink_metadata(&journal)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_file(&journal).unwrap();
        fs::create_dir(&journal).unwrap();
        assert!(recover(&state).is_err());
        assert!(journal.is_dir());
        fs::remove_dir(&journal).unwrap();
        recover(&state).unwrap();
    }
}

#[cfg(test)]
mod ordered_tests {
    use super::super::durable::probe;
    use super::*;

    fn fixture(root: &Path) -> (PathBuf, PairPending) {
        let state = root.join("state.json");
        fs::write(&state, b"prior exact state").unwrap();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        let mut artifacts = Vec::new();
        for (name, bytes) in [
            ("quipu", b"prior CLI".as_slice()),
            ("quipu-server", b"prior server".as_slice()),
        ] {
            let destination = bin.join(name);
            fs::write(&destination, bytes).unwrap();
            let backup = root
                .join("release-backups")
                .join(name)
                .join(hash(&destination).unwrap());
            artifacts.push(Pending {
                restore_alias: None,
                tool: "quipu".into(),
                sha256: hash(&destination).unwrap(),
                destination,
                backup,
            });
        }
        (
            state,
            PairPending {
                runtime_aliases: false,
                artifacts,
                previous_state: Some(b"prior exact state".to_vec()),
            },
        )
    }

    #[test]
    fn interrupted_alias_update_restores_primary_before_verifying_alias_bytes() {
        let root = tempfile::tempdir().unwrap();
        let (state, mut pair) = fixture(root.path());
        pair.runtime_aliases = true;
        let primary = pair.artifacts[0].destination.clone();
        let server = pair.artifacts[1].destination.clone();
        fs::remove_file(&server).unwrap();
        std::os::unix::fs::symlink("quipu", &server).unwrap();
        pair.artifacts[1].restore_alias = Some("quipu".into());
        pair.artifacts[1].sha256 = hash(&primary).unwrap();
        for a in &pair.artifacts {
            atomic_copy(&a.destination, &a.backup).unwrap();
        }
        begin_pair(&state, &pair).unwrap();
        fs::write(&primary, b"new unconfirmed binary").unwrap();
        fs::write(&state, b"new unconfirmed state").unwrap();
        recover(&state).unwrap();
        assert_eq!(fs::read(&primary).unwrap(), b"prior CLI");
        assert_eq!(fs::read_link(&server).unwrap(), Path::new("quipu"));
        assert_eq!(hash(&server).unwrap(), pair.artifacts[0].sha256);
        assert_eq!(fs::read(&state).unwrap(), b"prior exact state");
        assert!(!journal(&state).exists());
    }

    #[test]
    fn interrupted_conversion_restores_two_regular_executables() {
        let root = tempfile::tempdir().unwrap();
        let (state, mut pair) = fixture(root.path());
        pair.runtime_aliases = true;
        for a in &pair.artifacts {
            atomic_copy(&a.destination, &a.backup).unwrap();
        }
        begin_pair(&state, &pair).unwrap();
        fs::write(&pair.artifacts[0].destination, b"candidate").unwrap();
        crate::quipu_layout::replace_alias(&pair.artifacts[1].destination).unwrap();
        recover(&state).unwrap();
        for a in &pair.artifacts {
            assert!(fs::symlink_metadata(&a.destination).unwrap().is_file());
            assert_eq!(hash(&a.destination).unwrap(), a.sha256);
        }
        assert_eq!(fs::read(&state).unwrap(), b"prior exact state");
        assert!(!journal(&state).exists());
    }

    #[test]
    fn alias_recovery_refuses_corrupt_backup_or_foreign_link_before_any_write() {
        for corrupt in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (state, mut pair) = fixture(root.path());
            pair.runtime_aliases = true;
            for a in &pair.artifacts {
                atomic_copy(&a.destination, &a.backup).unwrap();
            }
            begin_pair(&state, &pair).unwrap();
            let primary = &pair.artifacts[0].destination;
            let server = &pair.artifacts[1].destination;
            fs::write(primary, b"candidate").unwrap();
            if corrupt {
                fs::write(&pair.artifacts[1].backup, b"corrupt").unwrap();
                crate::quipu_layout::replace_alias(server).unwrap();
            } else {
                fs::remove_file(server).unwrap();
                std::os::unix::fs::symlink("foreign", server).unwrap();
            }
            let before = fs::read(journal(&state)).unwrap();
            assert!(recover(&state).is_err());
            assert_eq!(fs::read(primary).unwrap(), b"candidate");
            assert_eq!(fs::read(&state).unwrap(), b"prior exact state");
            assert_eq!(fs::read(journal(&state)).unwrap(), before);
        }
    }

    #[test]
    fn alias_journal_requires_exact_quipu_pair_and_old_journals_omit_extension() {
        let root = tempfile::tempdir().unwrap();
        let (state, mut pair) = fixture(root.path());
        let old = serde_json::to_value(&pair).unwrap();
        assert!(old.get("runtime_aliases").is_none());
        assert!(old["artifacts"][0].get("restore_alias").is_none());
        pair.runtime_aliases = true;
        pair.artifacts[1].restore_alias = Some("foreign".into());
        assert!(begin_pair(&state, &pair).is_err());
        assert!(!journal(&state).exists());
        pair.artifacts[1].restore_alias = Some("quipu".into());
        pair.artifacts[0].tool = "other".into();
        assert!(begin_pair(&state, &pair).is_err());
        assert!(!journal(&state).exists());
    }

    #[test]
    fn new_ancestors_and_backup_renames_are_synced_before_journal_and_clear_before_success() {
        let root = tempfile::tempdir().unwrap();
        let (state, pair) = fixture(root.path());
        probe::start(None);
        for a in &pair.artifacts {
            atomic_copy(&a.destination, &a.backup).unwrap();
        }
        begin_pair(&state, &pair).unwrap();
        finish(&state).unwrap();
        let trace = probe::take().events;
        let journal_at = trace
            .iter()
            .position(|(s, _)| *s == "pair-journal-durable")
            .unwrap();
        for (i, (stage, path)) in trace.iter().enumerate() {
            if *stage == "directory-created" {
                assert!(trace[i + 1..journal_at]
                    .iter()
                    .any(|(s, p)| *s == "directory-synced" && p == path));
                assert!(trace[i + 1..journal_at]
                    .iter()
                    .any(|(s, p)| *s == "directory-synced" && p == durable::parent(path)));
            }
            if *stage == "copy-renamed" {
                assert!(i < journal_at);
                assert!(trace[i + 1..journal_at]
                    .iter()
                    .any(|(s, p)| *s == "directory-synced" && p == durable::parent(path)));
            }
        }
        assert_eq!(
            trace.iter().filter(|(s, _)| *s == "copy-renamed").count(),
            2
        );
        let unlink = trace
            .iter()
            .position(|(s, _)| *s == "journal-unlinked")
            .unwrap();
        let cleared = trace
            .iter()
            .position(|(s, _)| *s == "journal-clear-durable")
            .unwrap();
        assert!(journal_at < unlink && unlink < cleared);
        assert!(trace[unlink + 1..cleared]
            .iter()
            .any(|(s, p)| *s == "directory-synced" && p == durable::parent(&state)));
    }

    #[test]
    fn failed_ancestor_or_backup_barrier_never_arms_journal_and_retry_syncs_residue() {
        for fault in ["ancestor", "backup"] {
            let root = tempfile::tempdir().unwrap();
            let (state, pair) = fixture(root.path());
            let path = if fault == "ancestor" {
                root.path().join("release-backups")
            } else {
                durable::parent(&pair.artifacts[0].backup).to_path_buf()
            };
            probe::start(Some(("directory-sync", path.clone())));
            assert!(
                atomic_copy(&pair.artifacts[0].destination, &pair.artifacts[0].backup).is_err()
            );
            let trace = probe::take().events;
            assert!(!state.with_extension("release-pending.json").exists());
            assert!(!trace.iter().any(|(s, _)| *s == "pair-journal-durable"));
            // The failed mkdir's entry now EXISTS. It must still be synced on retry.
            probe::start(None);
            for a in &pair.artifacts {
                atomic_copy(&a.destination, &a.backup).unwrap();
            }
            begin_pair(&state, &pair).unwrap();
            let trace = probe::take().events;
            let ready = trace
                .iter()
                .position(|(s, _)| *s == "pair-journal-durable")
                .unwrap();
            assert!(trace[..ready]
                .iter()
                .any(|(s, p)| *s == "directory-synced" && p == &path));
        }
    }

    #[test]
    fn failed_clear_barrier_retains_exact_journal_for_later_recovery_not_success() {
        let root = tempfile::tempdir().unwrap();
        let (state, pair) = fixture(root.path());
        for a in &pair.artifacts {
            atomic_copy(&a.destination, &a.backup).unwrap();
        }
        begin_pair(&state, &pair).unwrap();
        let original = fs::read(journal(&state)).unwrap();
        for a in &pair.artifacts {
            fs::write(&a.destination, b"new unconfirmed").unwrap();
        }
        fs::write(&state, b"new unconfirmed state").unwrap();
        probe::start(Some(("directory-sync", root.path().to_path_buf())));
        assert!(finish(&state)
            .unwrap_err()
            .to_string()
            .contains("durability uncertain"));
        let trace = probe::take().events;
        assert!(!trace.iter().any(|(s, _)| *s == "journal-clear-durable"));
        assert_eq!(fs::read(journal(&state)).unwrap(), original);
        recover(&state).unwrap();
        for a in &pair.artifacts {
            assert_eq!(hash(&a.destination).unwrap(), a.sha256);
        }
        assert_eq!(fs::read(&state).unwrap(), b"prior exact state");
        assert!(!journal(&state).exists());
    }

    #[test]
    fn old_single_artifact_journal_recovers_without_paired_fields() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state.json");
        let mut value = State::read(&state).unwrap();
        value.tools.insert(
            "bobbin".into(),
            crate::model::ToolState {
                version: "keeper".into(),
                applied: true,
                verified: true,
            },
        );
        value.tools.insert(
            "seeds".into(),
            crate::model::ToolState {
                version: "unconfirmed".into(),
                applied: true,
                verified: true,
            },
        );
        value.write(&state).unwrap();
        let destination = root.path().join("sd");
        let backup = root.path().join("backup");
        fs::write(&destination, b"new").unwrap();
        fs::write(&backup, b"old").unwrap();
        let legacy = serde_json::json!({"tool":"seeds","destination":destination,"backup":backup,"sha256":hash(&backup).unwrap()});
        fs::write(journal(&state), serde_json::to_vec(&legacy).unwrap()).unwrap();
        recover(&state).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        let restored = State::read(&state).unwrap();
        assert_eq!(restored.tools["bobbin"].version, "keeper");
        assert!(!restored.tools.contains_key("seeds"));
        assert!(!journal(&state).exists());
    }
}
