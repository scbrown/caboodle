//! Exact transition aliases for the one-executable Quipu release layout.
use anyhow::{bail, Result};
use std::{fs, path::Path};

pub(crate) fn alias(path: &Path) -> Result<Option<String>> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_file() {
        return Ok(None);
    }
    if meta.file_type().is_symlink()
        && path.file_name().is_some_and(|n| n == "quipu-server")
        && fs::read_link(path)? == Path::new("quipu")
        && path.parent().is_some_and(|p| {
            fs::symlink_metadata(p.join("quipu")).is_ok_and(|m| m.file_type().is_file())
        })
    {
        return Ok(Some("quipu".into()));
    }
    bail!("Quipu server must be regular or the exact relative quipu-server -> quipu alias")
}

#[cfg(unix)]
pub(crate) fn replace_alias(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("alias has no parent"))?;
    let temporary = tempfile::tempdir_in(parent)?;
    let staged = temporary.path().join("quipu-server");
    std::os::unix::fs::symlink("quipu", &staged)?;
    fs::rename(staged, path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn replace_alias(_path: &Path) -> Result<()> {
    bail!("Quipu transition aliases require Unix")
}

/// Stage a regular executable without following a pre-existing destination link.
pub(crate) fn replace_regular(source: &Path, destination: &Path) -> Result<()> {
    if !fs::symlink_metadata(source)?.file_type().is_file() {
        bail!("Quipu primary must be a regular executable");
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("executable has no parent"))?;
    let temporary = tempfile::tempdir_in(parent)?;
    let staged = temporary.path().join("quipu");
    fs::copy(source, &staged)?;
    fs::File::open(&staged)?.sync_all()?;
    fs::rename(staged, destination)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
