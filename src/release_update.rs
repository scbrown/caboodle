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

pub(crate) fn stable_version(raw: &str) -> Result<(u64, u64, u64)> {
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

/// Is `installed` strictly OLDER than `reviewed`? `None` when either string
/// carries no comparable semantic version, so a caller can tell "this is a
/// stale install I may converge" from "I cannot tell, do not act".
///
/// Direction matters and the two answers have opposite remedies: converging a
/// STALE tool is the aegis-5ctwu3 fix, converging an AHEAD one is the
/// aegis-48dvl3 downgrade this repo already paid for once.
pub fn behind_reviewed(installed: &str, reviewed: &str) -> Option<bool> {
    let installed = installed_version(installed).ok()?;
    let reviewed = installed_version(reviewed).ok()?;
    Some(installed < reviewed)
}

fn names(
    tool: Option<ToolName>,
    tag: &str,
) -> Result<(&'static str, &'static str, String, String)> {
    names_for_platform(tool, tag, env::consts::OS, env::consts::ARCH)
}

fn names_for_platform(
    tool: Option<ToolName>,
    tag: &str,
    os: &str,
    arch: &str,
) -> Result<(&'static str, &'static str, String, String)> {
    stable_version(tag)?;
    if tool.is_none() {
        let target = match (os, arch) {
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            _ => bail!("Caboodle self-update has no published target for {os}/{arch}"),
        };
        let archive = format!("caboodle-{tag}-{target}.tar.gz");
        return Ok((
            "caboodle",
            "caboodle",
            archive.clone(),
            format!("{archive}.sha256"),
        ));
    }
    if os != "linux" || arch != "x86_64" {
        bail!("release-update currently supports Linux x86_64 only");
    }
    stable_version(tag)?;
    Ok(match tool {
        Some(ToolName::Bobbin) => (
            "bobbin",
            "bobbin",
            format!("bobbin-{tag}-x86_64-unknown-linux-gnu.tar.gz"),
            "SHA256SUMS.txt".into(),
        ),
        Some(ToolName::Yupana) => {
            let archive = format!("yupana-{tag}-x86_64-linux-gnu.tar.gz");
            (
                "yupana",
                "yupana",
                archive.clone(),
                format!("{archive}.sha256"),
            )
        }
        Some(ToolName::DesirePath) => (
            "desire-path",
            "dp",
            format!(
                "desire-path_{}_linux_amd64.tar.gz",
                tag.trim_start_matches('v')
            ),
            "checksums.txt".into(),
        ),
        None => {
            let archive = format!("caboodle-{tag}-x86_64-unknown-linux-gnu.tar.gz");
            (
                "caboodle",
                "caboodle",
                archive.clone(),
                format!("{archive}.sha256"),
            )
        }
        Some(tool) => bail!(
            "{} has a separate source/service update contract; release-update refuses it",
            tool.as_str()
        ),
    })
}

pub(crate) fn text(program: &str, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(checked(program, args, None)?.stdout)?
        .trim()
        .to_owned())
}

pub(crate) fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn checksum(body: &str, archive: &str) -> Result<String> {
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
    // Every variant is NAMED: a wildcard here once let any new tool pass this
    // guard silently (malcolm, aegis-1i5h1j).
    let (binary, args): (&str, Vec<&str>) = match tool {
        ToolName::Bobbin => ("bobbin", vec!["--version"]),
        ToolName::Yupana => ("yupana", vec!["--version"]),
        ToolName::DesirePath => ("dp", vec!["version"]),
        ToolName::Member(name) => {
            let m = crate::members::get(name).context("unknown stack member")?;
            (
                m.programs[0].name.as_str(),
                m.version_argv.iter().map(String::as_str).collect(),
            )
        }
        ToolName::Quipu | ToolName::Camayoc => return Ok(()),
    };
    let path = selected_path(binary)?;
    let installed = text(path.to_str().context("binary path is not UTF-8")?, &args)?;
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
    if let ToolName::Member(name) = tool {
        let m = crate::members::get(name).context("unknown stack member")?;
        return update_member(m, state_path, check_only);
    }
    update_binary(Some(plan), Some(tool), state_path, check_only, None, None)
}

/// Take the release-update state lock and finish any interrupted swap.
fn lock_state(state_path: &Path) -> Result<fs::File> {
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
    Ok(lock)
}

/// Release tracking for a data-declared stack member (aegis-2ezq5j).
///
/// A member tracks its REVIEWED PIN, never the newest published release: the
/// pin's digest is the member's trust anchor (members/README), and pins move by
/// `bump-member` PR -> caboodle release -> `update-self`. So this installs the
/// pinned asset, accepted on the digest embedded in this build, over the copy
/// PATH runs (the same copy the built-in tools update), with the same backup,
/// transaction and rollback-on-failed-verify. Every run ends in a functional
/// verify of that copy, so "verified" in the output is always a fresh proof.
fn update_member(m: &crate::members::Manifest, state_path: &Path, check_only: bool) -> Result<()> {
    let _lock = lock_state(state_path)?;
    let tool_id = m.name.as_str();
    if held() {
        println!("{tool_id}: held (no release lookup or install)");
        return Ok(());
    }
    let program = crate::adapter::single_program(m)?;
    let version_args: Vec<&str> = m.version_argv.iter().map(String::as_str).collect();
    let destination = selected_path(&program.name)?;
    crate::adapter::identify_member(m, &destination)?;
    let before = text(
        destination.to_str().context("binary path is not UTF-8")?,
        &version_args,
    )?;
    let pin = &m.version;
    let is_pin = |line: &str| {
        line.split_whitespace()
            .any(|w| w.trim_start_matches('v') == pin.as_str())
    };
    if is_pin(&before) {
        if check_only {
            println!("{tool_id}: at reviewed pin {pin} ({before}); functional proof not run");
            return Ok(());
        }
        crate::adapter::verify_at(m, &destination)?;
        println!("{tool_id}: current and verified (reviewed pin {pin}; {before})");
        return Ok(());
    }
    // Both sides in `<name> <version>` form: the version is the second word.
    match behind_reviewed(&before, &format!("{} {pin}", program.name)) {
        Some(true) => {}
        Some(false) => {
            println!(
                "{tool_id}: ahead of reviewed pin {pin} — refusing downgrade (installed {before})"
            );
            return Ok(());
        }
        None => bail!(
            "{tool_id}: cannot compare installed {before:?} with reviewed pin {pin}; refusing"
        ),
    }
    if check_only {
        println!(
            "{tool_id}: reviewed pin {pin}, installed {before}; install/functional proof not run"
        );
        return Ok(());
    }
    let (_scratch, programs) = crate::adapter::stage(m)?;
    let candidate = crate::adapter::find_program(&programs, &program.name)
        .with_context(|| format!("{tool_id} release does not contain `{}`", program.name))?;
    let after = text(
        candidate.to_str().context("candidate path is not UTF-8")?,
        &version_args,
    )?;
    if !is_pin(&after) {
        bail!("{tool_id}: candidate reports {after:?}, not the reviewed pin {pin}");
    }
    let old_hash = hash(&destination)?;
    let backup = state_path
        .parent()
        .context("state has no directory")?
        .join("release-backups")
        .join(&program.name)
        .join(&old_hash);
    atomic_copy(&destination, &backup)?;
    if held() {
        println!("{tool_id}: held before swap");
        return Ok(());
    }
    transaction::begin(
        state_path,
        &transaction::Pending {
            tool: tool_id.into(),
            destination: destination.clone(),
            backup: backup.clone(),
            sha256: old_hash.clone(),
        },
    )?;
    atomic_copy(&candidate, &destination)?;
    let verify = (|| -> Result<()> {
        let read_back = text(destination.to_str().unwrap(), &version_args)?;
        if !is_pin(&read_back) {
            bail!("installed {tool_id} reads back {read_back:?}, not {pin}");
        }
        crate::adapter::verify_at(m, &destination)
    })();
    if let Err(error) = verify {
        atomic_copy(&backup, &destination).context("verification failed AND rollback failed")?;
        if hash(&destination)? != old_hash {
            bail!("rollback checksum mismatch after {error:#}");
        }
        transaction::finish(state_path)?;
        return Err(error).context("member verification failed; previous artifact restored");
    }
    let mut state = State::read(state_path)?;
    state.tools.insert(
        tool_id.to_owned(),
        ToolState {
            version: after.clone(),
            applied: true,
            verified: true,
        },
    );
    state.write(state_path)?;
    emission::queue_transition(state_path, tool_id, "release-updated", &after)?;
    transaction::finish(state_path)?;
    println!(
        "{tool_id}: installed and verified reviewed pin {pin} (was {before}) backup={}",
        backup.display()
    );
    Ok(())
}

/// Update Caboodle itself from a published checksummed release.
pub fn update_self(state_path: &Path, check_only: bool) -> Result<()> {
    update_binary(None, None, state_path, check_only, None, None)
}

/// Event-driven callers select the exact published installer release.
pub fn update_self_at(state_path: &Path, check_only: bool, tag: &str) -> Result<()> {
    update_binary(None, None, state_path, check_only, Some(tag), None)
}

/// Bind an exact installer release to independently obtained artifact evidence.
pub fn update_self_pinned(
    state_path: &Path,
    check_only: bool,
    tag: &str,
    archive_sha256: &str,
    binary_sha256: &str,
) -> Result<()> {
    update_binary(
        None,
        None,
        state_path,
        check_only,
        Some(tag),
        Some((archive_sha256, binary_sha256)),
    )
}

fn release_path(tag: Option<&str>) -> Result<String> {
    match tag {
        None => Ok("releases/latest".into()),
        Some(tag) => {
            if !tag.starts_with('v') {
                bail!("installer tag must be an exact stable v-prefixed version");
            }
            stable_version(tag)?;
            Ok(format!("releases/tags/{tag}"))
        }
    }
}

fn update_binary(
    plan: Option<&Plan>,
    tool: Option<ToolName>,
    state_path: &Path,
    check_only: bool,
    requested_tag: Option<&str>,
    expected_artifacts: Option<(&str, &str)>,
) -> Result<()> {
    let endpoint = release_path(requested_tag)?;
    if let Some((archive, binary)) = expected_artifacts {
        if requested_tag.is_none() || tool.is_some() {
            bail!("artifact binding requires an exact installer release");
        }
        for hash in [archive, binary] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                bail!("installer artifact binding requires lowercase SHA256 values");
            }
        }
    }
    let tool_id = tool.map_or("caboodle", |t| t.as_str());
    let (repo, binary, _, _) = names(tool, "v0.0.0")?;
    let _lock = lock_state(state_path)?;
    if held() {
        println!("{}: held (no release lookup or install)", tool_id);
        return Ok(());
    }
    let destination = selected_path(binary)?;
    let version_arg = if tool == Some(ToolName::DesirePath) {
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
            &format!("https://api.github.com/repos/scbrown/{repo}/{endpoint}"),
        ],
    )?;
    let release: Release = serde_json::from_str(&raw).context("parse published release")?;
    if requested_tag.is_some_and(|tag| tag != release.tag_name) {
        bail!("published metadata tag differs from requested installer release");
    }
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
            tool_id, release.tag_name
        );
        return Ok(());
    }
    if check_only {
        println!("{}: published {}, installed {before}; assets available, install/functional proof not run", tool_id, release.tag_name);
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
    let archive_hash = hash(&archive)?;
    if archive_hash != checksum(&fs::read_to_string(sums)?, &archive_name)? {
        bail!("release archive SHA256 mismatch");
    }
    if expected_artifacts.is_some_and(|(expected, _)| expected != archive_hash) {
        bail!("release archive differs from pre-install artifact evidence");
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
    let new_hash = hash(&candidate)?;
    if expected_artifacts.is_some_and(|(_, expected)| expected != new_hash) {
        bail!("installer candidate differs from pre-install artifact evidence");
    }
    let after = text(candidate.to_str().unwrap(), &[version_arg])?;
    if installed_version(&after)? != wanted {
        bail!("candidate version differs from published release");
    }
    let old_hash = hash(&destination)?;
    let mut state = State::read(state_path)?;
    if old_hash == new_hash
        && state
            .tools
            .get(tool_id)
            .is_some_and(|s| s.verified && s.version == after)
    {
        println!(
            "{}: current and verified ({}; sha256 {new_hash})",
            tool_id, release.tag_name
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
        println!("{}: held before swap", tool_id);
        return Ok(());
    }
    if hash(&destination)? != old_hash {
        bail!("installed binary changed concurrently");
    }
    transaction::begin(
        state_path,
        &transaction::Pending {
            tool: tool_id.into(),
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
        if let Some(tool) = tool {
            adapter(
                tool,
                plan.context("tool update requires a reviewed plan")?
                    .quipu_flavor,
            )
            .verify()?;
        } else {
            let help = text(destination.to_str().unwrap(), &["update-release", "--help"])?;
            if !help.contains("--tool") {
                bail!("updated Caboodle omitted release-update command contract");
            }
        }
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
        tool_id.to_owned(),
        ToolState {
            version: after.clone(),
            applied: true,
            verified: true,
        },
    );
    state.write(state_path)?;
    emission::queue_transition(state_path, tool_id, "release-updated", &after)?;
    transaction::finish(state_path)?;
    println!(
        "{}: installed and verified {} sha256={new_hash} backup={}",
        tool_id,
        release.tag_name,
        backup.display()
    );
    // Self-update replaces the copy PATH runs. If this process is a different
    // copy (run by full path), it stays at its old build: say so (aegis-nvw6ye.1).
    if tool.is_none() {
        if let Ok(current) = env::current_exe() {
            let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            if canonical(&current) != canonical(&destination) {
                println!(
                    "caboodle: note: updated {} (the copy PATH runs); this binary {} was NOT changed",
                    destination.display(),
                    current.display()
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_installer_tag_selects_only_its_safe_release_path() {
        assert_eq!(release_path(None).unwrap(), "releases/latest");
        assert_eq!(
            release_path(Some("v0.2.1")).unwrap(),
            "releases/tags/v0.2.1"
        );
        for bad in [
            "",
            "0.2.1",
            "v01.2.1",
            "v0.2.1-rc1",
            "../latest",
            "v0.2.1?x=1",
        ] {
            assert!(release_path(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn behind_reviewed_reports_direction_and_refuses_to_guess() {
        assert_eq!(behind_reviewed("bobbin 0.1.0", "bobbin 0.16.2"), Some(true));
        assert_eq!(
            behind_reviewed("bobbin 0.16.2", "bobbin 0.16.2"),
            Some(false)
        );
        assert_eq!(
            behind_reviewed("bobbin 0.17.1", "bobbin 0.16.2"),
            Some(false)
        );
        // Not comparable is None, never "stale": replacing an unrecognised build
        // on a guess is the aegis-48dvl3 downgrade.
        assert_eq!(behind_reviewed("bobbin dev-build", "bobbin 0.16.2"), None);
        assert_eq!(behind_reviewed("bobbin", "bobbin 0.16.2"), None);
    }
    #[test]
    fn self_update_uses_only_published_platform_archives() {
        for (os, arch, triple) in [
            ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
            ("macos", "x86_64", "x86_64-apple-darwin"),
            ("macos", "aarch64", "aarch64-apple-darwin"),
        ] {
            let (repo, binary, archive, sums) =
                names_for_platform(None, "v0.2.13", os, arch).unwrap();
            assert_eq!((repo, binary), ("caboodle", "caboodle"));
            assert_eq!(archive, format!("caboodle-v0.2.13-{triple}.tar.gz"));
            assert_eq!(sums, format!("{archive}.sha256"));
        }
        for (os, arch) in [
            ("linux", "aarch64"),
            ("windows", "x86_64"),
            ("macos", "arm"),
        ] {
            assert!(names_for_platform(None, "v0.2.13", os, arch).is_err());
        }
        // Do not infer the other tools' asset layouts from Caboodle's layout.
        assert!(names_for_platform(Some(ToolName::Bobbin), "v0.2.13", "macos", "aarch64").is_err());
    }
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
