//! The one adapter for every data-declared stack member (aegis-1i5h1j C1).
//!
//! Install fetches the member's release asset for this host and accepts it only
//! if its SHA-256 equals the digest RECORDED in the reviewed manifest; it never
//! trusts a checksum file fetched at install time. It refuses to overwrite a
//! program of the same name that is not this member (A2). Verify runs the
//! manifest's steps in a hermetic temp HOME (A3), and the manifest schema
//! requires an absent-then-present marker pair (A4).

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};

use super::{checked, download_https, managed_bin_dir, Adapter};
use crate::members::{Manifest, Program};
use crate::model::ToolName;

pub(super) struct ManifestAdapter(pub(super) &'static Manifest);

/// The release target triple for this host, if the manifest records one.
pub(crate) fn release_target(manifest: &Manifest) -> Result<&'static str> {
    let host = match (env::consts::ARCH, env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        (arch, os) => bail!("{}: no release target for {arch}-{os}", manifest.name),
    };
    if !manifest.sha256.contains_key(host) {
        bail!(
            "{} {} has no reviewed digest for {host}; this host cannot install it",
            manifest.name,
            manifest.version
        );
    }
    Ok(host)
}

/// Where install writes `program`. HOME or CARGO_HOME is required.
fn destination(program: &str) -> Result<PathBuf> {
    Ok(managed_bin_dir()
        .context("HOME is required for a stack member")?
        .join(program))
}

/// The first executable `program` on PATH, if any.
fn on_path(program: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|p| {
            p.is_file()
                && p.metadata()
                    .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        })
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Does the executable at `path` identify as this member's `program`?
fn identify(path: &Path, program: &Program) -> Result<()> {
    let out = Command::new(path)
        .args(&program.identity_argv)
        .output()
        .with_context(|| format!("cannot run {} to read its identity", path.display()))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !text.contains(&program.identity_contains) {
        bail!(
            "a different `{}` is at {} (its identity does not contain {:?})",
            program.name,
            path.display(),
            program.identity_contains
        );
    }
    Ok(())
}

/// Refuse to replace, or to be shadowed by, a program that is not this member
/// (A2). The DESTINATION is what install overwrites, so it is checked whether
/// or not its directory is on PATH (malcolm B1, wu F2); a foreign program
/// earlier on PATH would shadow the install, so it is refused too.
fn refuse_foreign_program(manifest: &Manifest) -> Result<()> {
    for program in &manifest.programs {
        let dest = destination(&program.name)?;
        if dest.exists() {
            identify(&dest, program)
                .with_context(|| format!("refusing to overwrite it with {}", manifest.name))?;
        }
        if let Some(found) = on_path(&program.name).filter(|p| !same_file(p, &dest)) {
            identify(&found, program).with_context(|| {
                format!(
                    "it would shadow {} installed at {}",
                    manifest.name,
                    dest.display()
                )
            })?;
        }
    }
    Ok(())
}

/// The executable version and verify must run: the managed install when it
/// exists, otherwise the first on PATH. Either way its identity is checked,
/// so a same-named foreign program is never read back or verified as this member.
fn resolve(program: &Program) -> Result<PathBuf> {
    let dest = destination(&program.name)?;
    let path = if dest.exists() {
        dest
    } else {
        on_path(&program.name).with_context(|| format!("`{}` is not installed", program.name))?
    };
    identify(&path, program)?;
    Ok(path)
}

/// Copy `source` over `dest` via a temp file in the same directory and a
/// rename, so an interrupted install never leaves a truncated binary and a
/// running program is never written in place (malcolm N1).
fn install_atomically(source: &Path, dest: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = dest.parent().context("destination has no directory")?;
    let staged = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("stage in {}", dir.display()))?;
    fs::copy(source, staged.path()).with_context(|| format!("stage {}", dest.display()))?;
    fs::set_permissions(staged.path(), fs::Permissions::from_mode(0o755))?;
    staged.as_file().sync_all()?;
    staged
        .persist(dest)
        .map_err(|e| e.error)
        .with_context(|| format!("install {}", dest.display()))?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

impl Adapter for ManifestAdapter {
    fn name(&self) -> ToolName {
        ToolName::Member(self.0.name.as_str())
    }

    fn desired_version(&self) -> String {
        format!("{} {}", self.0.name, self.0.version)
    }

    fn install(&self) -> Result<()> {
        let m = self.0;
        let target = release_target(m)?;
        refuse_foreign_program(m)?;
        let asset = m.asset_for(target);
        let root =
            tempfile::tempdir().with_context(|| format!("create {} download directory", m.name))?;
        let archive = root.path().join(&asset);
        download_https(
            &format!(
                "https://github.com/{}/releases/download/{}/{asset}",
                m.repo,
                m.tag()
            ),
            &archive,
        )?;
        if sha256_file(&archive)? != m.sha256[target] {
            bail!(
                "{} {} release checksum mismatch for {target}",
                m.name,
                m.version
            );
        }
        let unpack = root.path().join("unpack");
        fs::create_dir_all(&unpack)?;
        checked(
            "tar",
            [
                OsStr::new("-xzf"),
                archive.as_os_str(),
                OsStr::new("-C"),
                unpack.as_os_str(),
            ],
            None,
        )?;
        let bin = managed_bin_dir().context("HOME is required to install a stack member")?;
        fs::create_dir_all(&bin).with_context(|| format!("create {}", bin.display()))?;
        for program in &m.programs {
            let source = find_program(&unpack, &program.name).with_context(|| {
                format!("{} release does not contain `{}`", m.name, program.name)
            })?;
            install_atomically(&source, &bin.join(&program.name))?;
        }
        Ok(())
    }

    fn version(&self) -> Result<String> {
        let m = self.0;
        let path = resolve(&m.programs[0])?;
        let out = checked(
            path.as_os_str(),
            m.version_argv.iter().map(String::as_str),
            None,
        )?;
        let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if version.is_empty() {
            bail!(
                "{} {} returned no version",
                m.name,
                m.version_argv.join(" ")
            );
        }
        Ok(version)
    }

    fn is_current(&self, installed: &str) -> bool {
        installed
            .split_whitespace()
            .any(|word| word.trim_start_matches('v') == self.0.version)
    }

    fn verify(&self) -> Result<()> {
        let m = self.0;
        let root =
            tempfile::tempdir().with_context(|| format!("create {} verification root", m.name))?;
        let marker = format!(
            "caboodle-{}-{}",
            m.name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        // Verify the member, not whatever answers to its name (B1).
        let resolved = m
            .programs
            .iter()
            .map(|p| Ok((p.name.as_str(), resolve(p)?)))
            .collect::<Result<Vec<_>>>()?;
        for (i, step) in m.verify.iter().enumerate() {
            let mut argv: Vec<String> = step
                .argv
                .iter()
                .map(|a| a.replace("{marker}", &marker))
                .collect();
            if let Some((_, path)) = resolved.iter().find(|(name, _)| *name == argv[0]) {
                argv[0] = path.to_string_lossy().into_owned();
            }
            let out = hermetic(root.path(), &argv).with_context(|| {
                format!("{} verify step {} ({})", m.name, i + 1, argv.join(" "))
            })?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            if let Some(absent) = &step.absent {
                let needle = absent.replace("{marker}", &marker);
                if stdout.contains(&needle) {
                    bail!("{} verify step {}: control failed, {needle:?} present before it was created", m.name, i + 1);
                }
            }
            if let Some(present) = &step.present {
                let needle = present.replace("{marker}", &marker);
                if !stdout.contains(&needle) {
                    bail!(
                        "{} verify step {}: {needle:?} not present after it was created",
                        m.name,
                        i + 1
                    );
                }
            }
        }
        Ok(())
    }
}

fn find_program(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|e| {
            let p = e.path();
            if p.is_dir() {
                find_program(&p, name)
            } else {
                None
            }
        })
}

/// Run one verify step in `root` with a CLEARED environment (A3): HOME and the
/// XDG dirs inside `root`, PATH and the locale passed through, nothing else. A
/// member that reads its store location from the environment would otherwise
/// write the verify marker into the user's live store (malcolm B2).
fn hermetic(root: &Path, argv: &[String]) -> Result<std::process::Output> {
    let (program, args) = argv.split_first().context("empty verify step")?;
    let mut command = Command::new(program);
    command.args(args).current_dir(root).env_clear();
    for var in ["PATH", "LANG", "LC_ALL", "TERM"] {
        if let Some(value) = env::var_os(var) {
            command.env(var, value);
        }
    }
    for (var, dir) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        let path = root.join(dir);
        fs::create_dir_all(&path)?;
        command.env(var, path);
    }
    let out = command.output().with_context(|| format!("run {program}"))?;
    if !out.status.success() {
        bail!(
            "exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out)
}
