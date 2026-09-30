//! The one adapter for every data-declared stack member (aegis-1i5h1j C1).
//!
//! Install fetches the member's release asset for this host and accepts it only
//! if its SHA-256 equals the digest RECORDED in the reviewed manifest; it never
//! trusts a checksum file fetched at install time. It refuses to overwrite a
//! program of the same name that is not this member (A2). Verify runs the
//! manifest's steps in a hermetic temp HOME (A3), and the manifest schema
//! requires an absent-then-present marker pair (A4).

use std::{env, ffi::OsStr, fs, path::Path, process::Command};

use anyhow::{bail, Context, Result};

use super::{checked, download_https, managed_bin_dir, read_version, Adapter};
use crate::members::Manifest;
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

/// Refuse to replace a program on PATH that is not this member (A2).
fn refuse_foreign_program(manifest: &Manifest) -> Result<()> {
    for program in &manifest.programs {
        let Ok(out) = Command::new(&program.name)
            .args(&program.identity_argv)
            .output()
        else {
            continue; // not installed: nothing to protect
        };
        let text = String::from_utf8_lossy(&out.stdout);
        if !text.trim_start().starts_with(&program.identity_prefix) {
            bail!(
                "a different `{}` is already on PATH (its identity does not start with {:?}); \
                 refusing to replace it with {}",
                program.name,
                program.identity_prefix,
                manifest.name
            );
        }
    }
    Ok(())
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
        let digest = checked("sha256sum", [archive.as_os_str()], None)?;
        let got = String::from_utf8_lossy(&digest.stdout);
        let want = &m.sha256[target];
        if got.split_whitespace().next() != Some(want.as_str()) {
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
            let dest = bin.join(&program.name);
            fs::copy(&source, &dest).with_context(|| format!("install {}", dest.display()))?;
        }
        Ok(())
    }

    fn version(&self) -> Result<String> {
        let m = self.0;
        let program = &m.programs[0].name;
        if m.version_argv == ["--version"] {
            return read_version(program);
        }
        let out = checked(
            program.as_str(),
            m.version_argv.iter().map(String::as_str),
            None,
        )?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
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
        for (i, step) in m.verify.iter().enumerate() {
            let argv: Vec<String> = step
                .argv
                .iter()
                .map(|a| a.replace("{marker}", &marker))
                .collect();
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

/// Run one verify step in `root` with HOME and the XDG dirs inside it (A3), so
/// a verify never reads the user's config or writes into their repository.
fn hermetic(root: &Path, argv: &[String]) -> Result<std::process::Output> {
    let (program, args) = argv.split_first().context("empty verify step")?;
    let mut command = Command::new(program);
    command.args(args).current_dir(root);
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
