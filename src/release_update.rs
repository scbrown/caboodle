//! Opt-in published-release delivery for binary tools selected by a reviewed plan.
//! Source-managed services and source bundles retain their own update contracts.
mod transaction;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    adapter::{adapter, checked, download_https},
    emission,
    model::{Plan, State, ToolName, ToolState},
};

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
}

fn stable_version(raw: &str) -> Result<(u64, u64, u64)> {
    let raw = raw.strip_prefix('v').unwrap_or(raw);
    let values: Vec<_> = raw.split('.').collect();
    if values.len() != 3
        || values.iter().any(|v| {
            v.is_empty()
                || !v.bytes().all(|c| c.is_ascii_digit())
                || (v.len() > 1 && v.starts_with('0'))
        })
    {
        bail!("not an unambiguous stable semantic version: {raw}");
    }
    Ok((values[0].parse()?, values[1].parse()?, values[2].parse()?))
}

fn installed_version(output: &str) -> Result<(u64, u64, u64)> {
    stable_version(
        output
            .split_whitespace()
            .nth(1)
            .context("binary omitted version")?,
    )
}

fn names(tool: ToolName, tag: &str) -> Result<(&'static str, &'static str, String, String)> {
    if env::consts::OS != "linux" || env::consts::ARCH != "x86_64" {
        bail!("release-update currently supports Linux x86_64 only");
    }
    stable_version(tag)?;
    Ok(match tool {
        ToolName::Bobbin => (
            "bobbin",
            "bobbin",
            format!("bobbin-{tag}-x86_64-unknown-linux-gnu.tar.gz"),
            "SHA256SUMS.txt".into(),
        ),
        ToolName::Yupana => {
            let archive = format!("yupana-{tag}-x86_64-linux-gnu.tar.gz");
            (
                "yupana",
                "yupana",
                archive.clone(),
                format!("{archive}.sha256"),
            )
        }
        ToolName::DesirePath => (
            "desire-path",
            "dp",
            format!(
                "desire-path_{}_linux_amd64.tar.gz",
                tag.trim_start_matches('v')
            ),
            "checksums.txt".into(),
        ),
        _ => bail!(
            "{} has a separate source/service update contract; release-update refuses it",
            tool.as_str()
        ),
    })
}

fn text(program: &str, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(checked(program, args, None)?.stdout)?
        .trim()
        .to_owned())
}

fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn checksum(body: &str, archive: &str) -> Result<String> {
    let candidates: Vec<_> = body
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() == 2 && fields[1].trim_start_matches('*') == archive {
                Some(fields[0])
            } else {
                None
            }
        })
        .collect();
    if candidates.len() != 1
        || candidates[0].len() != 64
        || !candidates[0].bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("checksum file must contain exactly one SHA256 for {archive}");
    }
    Ok(candidates[0].to_ascii_lowercase())
}

fn atomic_copy(source: &Path, target: &Path) -> Result<()> {
    let parent = target.parent().context("destination has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    fs::copy(source, temporary.path())?;
    temporary.as_file().sync_all()?;
    if hash(source)? != hash(temporary.path())? {
        bail!("staged copy checksum differs");
    }
    temporary.persist(target).map_err(|e| e.error)?;
    Ok(())
}

fn selected_path(binary: &str) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    for directory in env::split_paths(&env::var_os("PATH").context("PATH missing")?) {
        let candidate = directory.join(binary);
        if candidate.is_file() && candidate.metadata()?.permissions().mode() & 0o111 != 0 {
            return Ok(candidate);
        }
    }
    bail!("{binary} is not installed on PATH; use the reviewed initial install first")
}

fn held() -> bool {
    env::var_os("CABOODLE_HOLD_FILE")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".caboodle/hold")))
        .is_some_and(|p| p.exists())
}

/// Guard the older reviewed-pin updater after a release-tracking install.
/// An unreadable installed identity is never evidence that replacement is safe.
pub fn guard_reviewed_update(tool: ToolName, desired: &str) -> Result<()> {
    let binary = match tool {
        ToolName::Bobbin => "bobbin",
        ToolName::Yupana => "yupana",
        ToolName::DesirePath => "dp",
        _ => return Ok(()),
    };
    let path = selected_path(binary)?;
    let arg = if tool == ToolName::DesirePath {
        "version"
    } else {
        "--version"
    };
    let installed = text(path.to_str().context("binary path is not UTF-8")?, &[arg])?;
    if installed_version(&installed)? >= installed_version(desired)? {
        bail!("refusing reviewed-pin downgrade or ambiguous replacement: installed {installed}, reviewed {desired}; use update-release for published upgrades");
    }
    Ok(())
}

/// Update one selected binary, preserving every other adapter's state and gate.
pub fn update(plan: &Plan, tool: ToolName, state_path: &Path, check_only: bool) -> Result<()> {
    plan.validate()?;
    if !plan.tools.contains(&tool) {
        bail!("tool is not selected by the reviewed plan");
    }
    let (repo, binary, _, _) = names(tool, "v0.0.0")?;
    let state_dir = state_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(state_dir)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state_dir.join("release-update.lock"))?;
    FileExt::try_lock_exclusive(&lock).context("another release update holds the state lock")?;
    transaction::recover(state_path)?;
    if held() {
        println!("{}: held (no release lookup or install)", tool.as_str());
        return Ok(());
    }
    let destination = selected_path(binary)?;
    let version_arg = if tool == ToolName::DesirePath {
        "version"
    } else {
        "--version"
    };
    let before = text(
        destination.to_str().context("binary path is not UTF-8")?,
        &[version_arg],
    )?;
    let installed = installed_version(&before)?;
    let raw = text(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            "--proto",
            "=https",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/scbrown/{repo}/releases/latest"),
        ],
    )?;
    let release: Release = serde_json::from_str(&raw).context("parse published release")?;
    if release.draft || release.prerelease {
        bail!("refusing draft/prerelease");
    }
    let wanted = stable_version(&release.tag_name)?;
    let (_, _, archive_name, sums_name) = names(tool, &release.tag_name)?;
    for asset in [&archive_name, &sums_name] {
        if release.assets.iter().filter(|a| &a.name == asset).count() != 1 {
            bail!("published release lacks unique asset {asset}");
        }
    }
    if installed > wanted {
        println!(
            "{}: ahead of published {} — refusing downgrade (installed {before})",
            tool.as_str(),
            release.tag_name
        );
        return Ok(());
    }
    if check_only {
        println!("{}: published {}, installed {before}; assets available, install/functional proof not run", tool.as_str(), release.tag_name);
        return Ok(());
    }
    let temporary = tempfile::tempdir()?;
    let archive = temporary.path().join(&archive_name);
    let sums = temporary.path().join(&sums_name);
    let base = format!(
        "https://github.com/scbrown/{repo}/releases/download/{}",
        release.tag_name
    );
    download_https(&format!("{base}/{archive_name}"), &archive)?;
    download_https(&format!("{base}/{sums_name}"), &sums)?;
    if hash(&archive)? != checksum(&fs::read_to_string(sums)?, &archive_name)? {
        bail!("release archive SHA256 mismatch");
    }
    // Extract only the one named executable; never unpack archive paths onto disk.
    let members = text(
        "tar",
        &[
            "-tzf",
            archive.to_str().context("archive path is not UTF-8")?,
        ],
    )?;
    let matches: Vec<_> = members
        .lines()
        .filter(|m| Path::new(m).file_name().is_some_and(|n| n == binary))
        .collect();
    if matches.len() != 1 {
        bail!("archive must contain exactly one {binary} executable");
    }
    let member = matches[0];
    if Path::new(member).is_absolute()
        || Path::new(member)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("unsafe archive member");
    }
    let output = checked(
        "tar",
        ["-xOzf", archive.to_str().unwrap(), "--", member],
        None,
    )?;
    let candidate = temporary.path().join(binary);
    fs::write(&candidate, output.stdout)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o755))?;
    let after = text(candidate.to_str().unwrap(), &[version_arg])?;
    if installed_version(&after)? != wanted {
        bail!("candidate version differs from published release");
    }
    let old_hash = hash(&destination)?;
    let new_hash = hash(&candidate)?;
    let mut state = State::read(state_path)?;
    if old_hash == new_hash
        && state
            .tools
            .get(tool.as_str())
            .is_some_and(|s| s.verified && s.version == after)
    {
        println!(
            "{}: current and verified ({}; sha256 {new_hash})",
            tool.as_str(),
            release.tag_name
        );
        return Ok(());
    }
    // Same semver with different bytes can be an installed source build ahead of
    // its release. Never silently replace it based on a version-only equality.
    if installed == wanted && old_hash != new_hash {
        bail!("installed version equals release but bytes differ; refusing ambiguous replacement");
    }
    let backup = state_path
        .parent()
        .context("state has no directory")?
        .join("release-backups")
        .join(binary)
        .join(&old_hash);
    atomic_copy(&destination, &backup)?;
    if held() {
        println!("{}: held before swap", tool.as_str());
        return Ok(());
    }
    if hash(&destination)? != old_hash {
        bail!("installed binary changed concurrently");
    }
    transaction::begin(
        state_path,
        &transaction::Pending {
            tool: tool.as_str().into(),
            destination: destination.clone(),
            backup: backup.clone(),
            sha256: old_hash.clone(),
        },
    )?;
    atomic_copy(&candidate, &destination)?;
    let verify = (|| -> Result<()> {
        if hash(&destination)? != new_hash
            || text(destination.to_str().unwrap(), &[version_arg])? != after
        {
            bail!("installed artifact read-back mismatch");
        }
        adapter(tool, plan.quipu_flavor).verify()?;
        Ok(())
    })();
    if let Err(error) = verify {
        atomic_copy(&backup, &destination).context("verification failed AND rollback failed")?;
        if hash(&destination)? != old_hash {
            bail!("rollback checksum mismatch after {error:#}");
        }
        transaction::finish(state_path)?;
        return Err(error).context("release verification failed; previous artifact restored");
    }
    state.tools.insert(
        tool.as_str().to_owned(),
        ToolState {
            version: after.clone(),
            applied: true,
            verified: true,
        },
    );
    state.write(state_path)?;
    emission::queue_transition(state_path, tool.as_str(), "release-updated", &after)?;
    transaction::finish(state_path)?;
    println!(
        "{}: installed and verified {} sha256={new_hash} backup={}",
        tool.as_str(),
        release.tag_name,
        backup.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_versions_are_numeric_and_ambiguous_inputs_refused() {
        assert!(stable_version("v0.16.3").unwrap() > stable_version("0.9.9").unwrap());
        for bad in [
            "1.2",
            "01.2.3",
            "1.2.3-rc1",
            "1.2.3+build",
            "1.2.3.4",
            "1.2.x",
        ] {
            assert!(stable_version(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn checksum_requires_unique_exact_filename() {
        let sha = "a".repeat(64);
        assert_eq!(
            checksum(&format!("{sha} *target.tar.gz\n"), "target.tar.gz").unwrap(),
            sha
        );
        assert!(checksum(&format!("{sha} other.tar.gz"), "target.tar.gz").is_err());
        assert!(checksum(
            &format!("{sha} target.tar.gz\n{sha} target.tar.gz"),
            "target.tar.gz"
        )
        .is_err());
        assert!(checksum("abc target.tar.gz", "target.tar.gz").is_err());
    }
    #[test]
    fn atomic_copy_preserves_symlink_target_and_previous_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        let target = dir.path().join("live");
        let source = dir.path().join("new");
        fs::write(&original, "old").unwrap();
        fs::write(&source, "new").unwrap();
        std::os::unix::fs::symlink(&original, &target).unwrap();
        atomic_copy(&source, &target).unwrap();
        assert_eq!(fs::read_to_string(original).unwrap(), "old");
        assert_eq!(fs::read_to_string(target).unwrap(), "new");
    }
}
