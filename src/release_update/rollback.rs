//! Seeds-only rollback of a recorded successful update, never a generic downgrade.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    destination: PathBuf,
    backup: PathBuf,
    state: PathBuf,
    plan_hash: String,
    state_hash: String,
    prior_hash: String,
    current_hash: String,
    prior_version: String,
    current_version: String,
}
fn receipt_path(state: &Path) -> PathBuf {
    state.with_extension("seeds-success.json")
}
fn plan_hash(plan: &Plan) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(plan)?)))
}
fn regular(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("rollback requires a regular non-symlink file");
    }
    Ok(())
}
pub(super) fn record(
    plan: &Plan,
    state: &Path,
    destination: &Path,
    backup: &Path,
    prior_hash: &str,
    prior_version: &str,
    current_version: &str,
) -> Result<()> {
    let receipt = Receipt {
        destination: destination.canonicalize()?,
        backup: backup.canonicalize()?,
        state: state.canonicalize()?,
        plan_hash: plan_hash(plan)?,
        state_hash: hash(state)?,
        prior_hash: prior_hash.into(),
        current_hash: hash(destination)?,
        prior_version: prior_version.into(),
        current_version: current_version.into(),
    };
    let file = tempfile::NamedTempFile::new_in(state.parent().context("state directory")?)?;
    fs::write(file.path(), serde_json::to_vec(&receipt)?)?;
    file.as_file().sync_all()?;
    file.persist(receipt_path(state)).map_err(|e| e.error)?;
    Ok(())
}
/// Require exact reviewed current/prior hashes and the same successful update's plan/state.
pub fn rollback_seeds(
    plan: &Plan,
    state_path: &Path,
    current_hash: &str,
    prior_hash: &str,
) -> Result<()> {
    rollback_at(
        plan,
        state_path,
        current_hash,
        prior_hash,
        &selected_path("sd")?,
    )
}

fn rollback_at(
    plan: &Plan,
    state_path: &Path,
    current_hash: &str,
    prior_hash: &str,
    destination: &Path,
) -> Result<()> {
    for value in [current_hash, prior_hash] {
        if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("rollback requires explicit SHA256 hashes");
        }
    }
    plan.validate()?;
    if !plan.tools.contains(&ToolName::Member("seeds")) {
        bail!("Seeds is not selected");
    }
    let _lock = lock_state_unrecovered(state_path)?;
    if state_path.with_extension("release-pending.json").exists() {
        bail!("pending release requires normal recovery before rollback");
    }
    if held() {
        bail!("rollback is held");
    }
    let path = receipt_path(state_path);
    regular(&path)?;
    let r: Receipt = serde_json::from_slice(&fs::read(&path)?)?;
    regular(destination)?;
    regular(&r.backup)?;
    if r.state != state_path.canonicalize()?
        || r.destination != destination.canonicalize()?
        || r.plan_hash != plan_hash(plan)?
        || r.state_hash != hash(state_path)?
    {
        bail!("rollback plan/state/destination binding changed");
    }
    let expected_backup = state_path
        .parent()
        .context("state directory")?
        .join("release-backups/sd")
        .join(prior_hash);
    if r.backup != expected_backup.canonicalize()?
        || r.prior_hash != prior_hash
        || r.current_hash != current_hash
        || hash(&r.backup)? != prior_hash
        || hash(destination)? != current_hash
    {
        bail!("rollback hash/backup binding mismatch");
    }
    let m = crate::members::get("seeds").context("Seeds manifest")?;
    crate::adapter::identify_member(m, destination)?;
    crate::adapter::identify_member(m, &r.backup)?;
    if text(
        destination.to_str().context("path encoding")?,
        &["--version"],
    )? != r.current_version
        || text(r.backup.to_str().context("path encoding")?, &["--version"])? != r.prior_version
    {
        bail!("rollback version binding mismatch");
    }
    let recovery = state_path
        .parent()
        .context("state directory")?
        .join("release-backups/sd")
        .join(current_hash);
    atomic_copy(destination, &recovery)?;
    transaction::begin(
        state_path,
        &transaction::Pending {
            restore_alias: None,
            tool: "seeds".into(),
            destination: destination.to_path_buf(),
            backup: recovery.clone(),
            sha256: current_hash.into(),
        },
    )?;
    atomic_copy(&r.backup, destination)?;
    let proof = (|| -> Result<()> {
        if hash(destination)? != prior_hash {
            bail!("rollback installed checksum mismatch");
        }
        if text(
            destination.to_str().context("path encoding")?,
            &["--version"],
        )? != r.prior_version
        {
            bail!("rollback installed version mismatch");
        }
        crate::adapter::verify_at(m, destination)
    })();
    if let Err(error) = proof {
        atomic_copy(&recovery, destination)?;
        if hash(destination)? != current_hash {
            bail!("failed rollback recovery checksum mismatch");
        }
        transaction::finish(state_path)?;
        return Err(error).context("rollback verification failed; current artifact restored");
    }
    let mut state = State::read(state_path)?;
    state.tools.insert(
        "seeds".into(),
        ToolState {
            version: r.prior_version,
            applied: true,
            verified: true,
        },
    );
    state.write(state_path)?;
    transaction::finish(state_path)?;
    fs::remove_file(path)?;
    println!(
        "seeds: previous successful-update artifact restored and verified sha256={prior_hash}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn fixture() -> (tempfile::TempDir, Plan, PathBuf, PathBuf, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");
        let plan = Plan::for_profile(crate::model::Profile::Everything);
        let mut state = State::read(&state_path).unwrap();
        state.tools.insert(
            "bobbin".into(),
            ToolState {
                version: "keeper".into(),
                applied: true,
                verified: true,
            },
        );
        state.tools.insert(
            "seeds".into(),
            ToolState {
                version: "sd 0.2.2 (seeds)".into(),
                applied: true,
                verified: true,
            },
        );
        state.write(&state_path).unwrap();
        let script = |v: &str| {
            format!("#!/bin/sh\ncase \"$1\" in --help) echo 'seeds is the beads-compatible tracker';; --version) echo 'sd {v} (seeds)';; list) test ! -f marker || cat marker;; create) printf '%s' \"$2\" > marker;; esac\n")
        };
        let destination = dir.path().join("sd");
        let prior = dir.path().join("prior");
        fs::write(&destination, script("0.2.2")).unwrap();
        fs::write(&prior, script("0.2.0")).unwrap();
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&prior, fs::Permissions::from_mode(0o755)).unwrap();
        let current_hash = hash(&destination).unwrap();
        let prior_hash = hash(&prior).unwrap();
        let backup = dir.path().join("release-backups/sd").join(&prior_hash);
        atomic_copy(&prior, &backup).unwrap();
        record(
            &plan,
            &state_path,
            &destination,
            &backup,
            &prior_hash,
            "sd 0.2.0 (seeds)",
            "sd 0.2.2 (seeds)",
        )
        .unwrap();
        (dir, plan, state_path, destination, current_hash, prior_hash)
    }
    #[test]
    fn rollback_verifies_prior_and_preserves_other_state() {
        let (_dir, plan, state, destination, current, prior) = fixture();
        rollback_at(&plan, &state, &current, &prior, &destination).unwrap();
        assert_eq!(hash(&destination).unwrap(), prior);
        let after = State::read(&state).unwrap();
        assert_eq!(after.tools["bobbin"].version, "keeper");
        assert_eq!(after.tools["seeds"].version, "sd 0.2.0 (seeds)");
        assert!(after.tools["seeds"].verified);
        assert!(!receipt_path(&state).exists());
    }
    #[test]
    fn changed_state_and_wrong_hash_refuse_without_swap() {
        let (_dir, plan, state, destination, current, prior) = fixture();
        assert!(rollback_at(&plan, &state, &"0".repeat(64), &prior, &destination).is_err());
        assert_eq!(hash(&destination).unwrap(), current);
        let mut changed = State::read(&state).unwrap();
        changed.tools.get_mut("bobbin").unwrap().version = "new keeper".into();
        changed.write(&state).unwrap();
        assert!(rollback_at(&plan, &state, &current, &prior, &destination).is_err());
        assert_eq!(hash(&destination).unwrap(), current);
        assert!(!state.with_extension("release-pending.json").exists());
    }
    #[test]
    fn failed_functional_proof_restores_current_bytes_and_state() {
        let (dir, plan, state, destination, current, _prior) = fixture();
        let prior = dir.path().join("broken");
        fs::write(&prior, "#!/bin/sh\ncase \"$1\" in --help) echo 'seeds is the beads-compatible tracker';; --version) echo 'sd 0.2.0 (seeds)';; *) exit 7;; esac\n").unwrap();
        fs::set_permissions(&prior, fs::Permissions::from_mode(0o755)).unwrap();
        let prior_hash = hash(&prior).unwrap();
        let backup = dir.path().join("release-backups/sd").join(&prior_hash);
        atomic_copy(&prior, &backup).unwrap();
        record(
            &plan,
            &state,
            &destination,
            &backup,
            &prior_hash,
            "sd 0.2.0 (seeds)",
            "sd 0.2.2 (seeds)",
        )
        .unwrap();
        let before = fs::read(&state).unwrap();
        let error = rollback_at(&plan, &state, &current, &prior_hash, &destination).unwrap_err();
        assert!(error.to_string().contains("current artifact restored"));
        assert_eq!(hash(&destination).unwrap(), current);
        assert_eq!(fs::read(&state).unwrap(), before);
        assert!(receipt_path(&state).exists());
        assert!(!state.with_extension("release-pending.json").exists());
    }
    #[test]
    fn interrupted_rollback_recovers_current_and_invalidates_only_seeds_proof() {
        let (dir, plan, state, destination, current, prior) = fixture();
        let recovery = dir.path().join("release-backups/sd").join(&current);
        atomic_copy(&destination, &recovery).unwrap();
        transaction::begin(
            &state,
            &transaction::Pending {
                restore_alias: None,
                tool: "seeds".into(),
                destination: destination.clone(),
                backup: recovery,
                sha256: current.clone(),
            },
        )
        .unwrap();
        atomic_copy(
            &dir.path().join("release-backups/sd").join(&prior),
            &destination,
        )
        .unwrap();
        assert!(rollback_at(&plan, &state, &current, &prior, &destination)
            .unwrap_err()
            .to_string()
            .contains("pending release"));
        transaction::recover(&state).unwrap();
        assert_eq!(hash(&destination).unwrap(), current);
        let recovered = State::read(&state).unwrap();
        assert_eq!(recovered.tools["bobbin"].version, "keeper");
        assert!(!recovered.tools.contains_key("seeds"));
        assert!(rollback_at(&plan, &state, &current, &prior, &destination).is_err());
        assert!(receipt_path(&state).exists());
    }
    #[test]
    fn lock_contention_and_tampered_backup_refuse_before_swap() {
        let (dir, plan, state, destination, current, prior) = fixture();
        let lock = lock_state_unrecovered(&state).unwrap();
        assert!(rollback_at(&plan, &state, &current, &prior, &destination).is_err());
        assert_eq!(hash(&destination).unwrap(), current);
        drop(lock);
        fs::write(
            dir.path().join("release-backups/sd").join(&prior),
            "corrupt backup",
        )
        .unwrap();
        assert!(rollback_at(&plan, &state, &current, &prior, &destination).is_err());
        assert_eq!(hash(&destination).unwrap(), current);
        assert!(!state.with_extension("release-pending.json").exists());
    }
    #[test]
    fn missing_receipt_and_symlink_destination_are_refused() {
        let (dir, plan, state, destination, current, prior) = fixture();
        let link = dir.path().join("linked-sd");
        std::os::unix::fs::symlink(&destination, &link).unwrap();
        assert!(rollback_at(&plan, &state, &current, &prior, &link).is_err());
        fs::remove_file(receipt_path(&state)).unwrap();
        assert!(rollback_at(&plan, &state, &current, &prior, &destination).is_err());
        assert_eq!(hash(&destination).unwrap(), current);
    }
}
