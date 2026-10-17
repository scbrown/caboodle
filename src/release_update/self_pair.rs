//! Keep both installer commands coherent when updating a dual-command install.
use super::{atomic_copy, checked, hash, installed_version, text, transaction};
use anyhow::{bail, Context, Result};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub(super) struct Companion {
    pub destination: PathBuf,
    pub candidate: PathBuf,
    pub old_hash: String,
    pub new_hash: String,
    pub after: String,
    pub backup: PathBuf,
}

fn optional_path(primary: &Path) -> Result<Option<PathBuf>> {
    // Only the same installation directory participates. A Tambo executable in
    // another PATH root does not authorize this legacy updater to replace it.
    let path = primary
        .parent()
        .context("installer has no directory")?
        .join("tambo");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("Tambo path eligibility is UNKNOWN"),
        Ok(meta) if meta.file_type().is_file() && meta.permissions().mode() & 0o111 != 0 => Ok(Some(path)),
        Ok(_) => bail!("Tambo companion must be a regular executable; use the package manager or verified installer"),
    }
}

pub(super) fn prepare(
    archive: &Path,
    directory: &Path,
    primary: &Path,
    before: &str,
    after: &str,
    state: &Path,
) -> Result<Option<Companion>> {
    let Some(destination) = optional_path(primary)? else {
        return Ok(None);
    };
    if !fs::symlink_metadata(primary)?.file_type().is_file() || primary == destination {
        bail!("installer pair requires distinct regular executable paths");
    }
    crate::apt_owned::refuse_replacement("tambo", &destination)?;
    let old_version = text(
        destination.to_str().context("Tambo path is not UTF-8")?,
        &["--version"],
    )?;
    if old_version
        .strip_prefix("tambo ")
        .context("installed Tambo omitted its command identity")?
        != before
            .strip_prefix("caboodle ")
            .context("installed Caboodle omitted its command identity")?
    {
        bail!("installed Tambo/Caboodle identities differ; refusing an ambiguous pair update");
    }
    let archive = archive.to_str().context("archive path is not UTF-8")?;
    let members = text("tar", &["-tzf", archive])?;
    let matches: Vec<_> = members
        .lines()
        .filter(|m| Path::new(m).file_name().is_some_and(|n| n == "tambo"))
        .collect();
    if matches.len() != 1 {
        bail!("installer archive must contain exactly one Tambo companion")
    }
    let member = matches[0];
    if Path::new(member).is_absolute()
        || Path::new(member)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("unsafe Tambo archive member");
    }
    let detail = text("tar", &["-tvzf", archive, "--", member])?;
    if !detail.starts_with('-') || detail.lines().count() != 1 {
        bail!("Tambo companion must be a regular archive member");
    }
    let candidate = directory.join("tambo");
    fs::write(
        &candidate,
        checked("tar", ["-xOzf", archive, "--", member], None)?.stdout,
    )?;
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755))?;
    let new_version = text(candidate.to_str().unwrap(), &["--version"])?;
    if new_version
        .strip_prefix("tambo ")
        .context("candidate Tambo omitted its command identity")?
        != after
            .strip_prefix("caboodle ")
            .context("candidate Caboodle omitted its command identity")?
    {
        bail!("candidate Tambo/Caboodle identities differ");
    }
    let old_hash = hash(&destination)?;
    let new_hash = hash(&candidate)?;
    if installed_version(before)? == installed_version(after)? && old_hash != new_hash {
        bail!("installed Tambo version equals release but bytes differ; refusing ambiguous replacement");
    }
    let backup = state
        .parent()
        .context("state has no directory")?
        .join("release-backups/tambo")
        .join(&old_hash);
    Ok(Some(Companion {
        destination,
        candidate,
        old_hash,
        new_hash,
        after: new_version,
        backup,
    }))
}

impl Companion {
    pub fn pending(&self) -> transaction::Pending {
        transaction::Pending {
            restore_alias: None,
            tool: "tambo".into(),
            destination: self.destination.clone(),
            backup: self.backup.clone(),
            sha256: self.old_hash.clone(),
        }
    }
    pub fn backup(&self) -> Result<()> {
        atomic_copy(&self.destination, &self.backup)
    }
    pub fn unchanged(&self) -> Result<()> {
        if hash(&self.destination)? != self.old_hash {
            bail!("Tambo companion changed concurrently")
        }
        Ok(())
    }
    pub fn install(&self) -> Result<()> {
        atomic_copy(&self.candidate, &self.destination)
    }
    pub fn verify(&self) -> Result<()> {
        if hash(&self.destination)? != self.new_hash
            || text(self.destination.to_str().unwrap(), &["--version"])? != self.after
        {
            bail!("Tambo companion installed read-back mismatch");
        }
        if !text(
            self.destination.to_str().unwrap(),
            &["update-release", "--help"],
        )?
        .contains("--tool")
        {
            bail!("Tambo companion omitted the release-update command contract");
        }
        Ok(())
    }
}
