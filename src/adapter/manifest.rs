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
use crate::members::{Kind, Manifest, Program, ANY_TARGET};
use crate::model::ToolName;

pub(super) struct ManifestAdapter(pub(super) &'static Manifest);

/// The release target triple for this host, if the manifest records one.
pub(crate) fn release_target(manifest: &Manifest) -> Result<&'static str> {
    if manifest.kind == Kind::PythonWheel {
        // One pure-Python wheel serves every host; validate() guarantees the key.
        return Ok(ANY_TARGET);
    }
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
/// Probes run in a throwaway HOME, so reading an identity never touches the
/// user's own config or store (wu, #46).
fn identify(path: &Path, program: &Program) -> Result<()> {
    identify_via(path, program, None)
}

/// `identify`, with the probe run through `wrapper` when one is given.
fn identify_via(path: &Path, program: &Program, wrapper: Option<&Path>) -> Result<()> {
    let root = tempfile::tempdir().context("create identity probe root")?;
    let out = hermetic_command(root.path(), wrapper, path)?
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
        let unpack = match m.kind {
            Kind::RustRelease => root.path().join("unpack"),
            // The venv outlives the install: its console scripts point into it.
            Kind::PythonWheel => venv_root(m)?,
        };
        let unpack = materialize(m, &archive, &unpack)?;
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
        reports_version(installed, &self.0.version)
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
            let stdin = step.stdin.as_ref().map(|s| s.replace("{marker}", &marker));
            let out =
                hermetic_via(root.path(), None, &argv, stdin.as_deref()).with_context(|| {
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

/// Does a version line report `version`, as a whole word (`v` optional)?
fn reports_version(line: &str, version: &str) -> bool {
    line.split_whitespace()
        .any(|word| word.trim_start_matches('v') == version)
}

/// Prove a release asset IS `manifest` before its digest is recorded (malcolm
/// S1): unpack it, and require every program to answer its identity and the
/// first to report `manifest.version`. A release that dropped its identity text
/// would otherwise bump green and then fail every fleet version/verify with a
/// foreign-program refusal that points at the wrong cause.
///
/// This EXECUTES an unreviewed release. `wrapper`, when given, is run as
/// `<wrapper> <program> <args...>` for every such execution, so the caller can
/// confine it (the unattended bump cron uses a no-network, no-home sandbox,
/// aegis-uy26l7). caboodle itself stays platform-neutral.
pub(crate) fn prove_release(
    manifest: &Manifest,
    archive: &Path,
    wrapper: Option<&Path>,
) -> Result<()> {
    let scratch = tempfile::tempdir().context("create release proof directory")?;
    let unpack = materialize(manifest, archive, &scratch.path().join("unpack"))?;
    for program in &manifest.programs {
        let path = find_program(&unpack, &program.name).with_context(|| {
            format!(
                "{} release does not contain `{}`",
                manifest.name, program.name
            )
        })?;
        identify_via(&path, program, wrapper).with_context(|| {
            format!(
                "{} {} does not answer its manifest identity; fix identity_contains in review \
                 (it must still match the oldest installed release) before bumping",
                manifest.name, manifest.version
            )
        })?;
    }
    let first =
        find_program(&unpack, &manifest.programs[0].name).context("release program vanished")?;
    let mut argv = vec![first.to_string_lossy().into_owned()];
    argv.extend(manifest.version_argv.iter().cloned());
    let out =
        hermetic_via(scratch.path(), wrapper, &argv, None).context("read the release's version")?;
    let reported = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !reports_version(&reported, &manifest.version) {
        bail!(
            "{} release tagged {} reports version {reported:?}; refusing to pin it",
            manifest.name,
            manifest.version
        );
    }
    Ok(())
}

/// Where a python-wheel member's venv lives: caboodle-owned, one per version.
fn venv_root(manifest: &Manifest) -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is required for a python-wheel member")?;
    Ok(PathBuf::from(home)
        .join(".local/share/caboodle/members")
        .join(&manifest.name)
        .join(&manifest.version))
}

/// Turn a verified release asset into a directory holding its programs.
///
/// rust-release: unpack the tarball into `dest`. python-wheel: build a fresh
/// venv at `dest` (replacing one a previous install left) and pip-install the
/// wheel into it; the programs are its console scripts in `dest/bin`. Installing
/// a wheel runs none of its code, so the caller may still confine the first
/// EXECUTION of an unreviewed release with a probe wrapper.
fn materialize(manifest: &Manifest, archive: &Path, dest: &Path) -> Result<PathBuf> {
    match manifest.kind {
        Kind::RustRelease => {
            fs::create_dir_all(dest)?;
            checked(
                "tar",
                [
                    OsStr::new("-xzf"),
                    archive.as_os_str(),
                    OsStr::new("-C"),
                    dest.as_os_str(),
                ],
                None,
            )?;
            Ok(dest.to_path_buf())
        }
        Kind::PythonWheel => {
            if dest.exists() {
                fs::remove_dir_all(dest).with_context(|| format!("replace {}", dest.display()))?;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            checked(
                "python3",
                [OsStr::new("-m"), OsStr::new("venv"), dest.as_os_str()],
                None,
            )?;
            checked(
                dest.join("bin/python").as_os_str(),
                [
                    OsStr::new("-m"),
                    OsStr::new("pip"),
                    OsStr::new("install"),
                    OsStr::new("--quiet"),
                    OsStr::new("--disable-pip-version-check"),
                    archive.as_os_str(),
                ],
                None,
            )
            .with_context(|| format!("pip install {}", manifest.name))?;
            Ok(dest.join("bin"))
        }
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
fn hermetic_via(
    root: &Path,
    wrapper: Option<&Path>,
    argv: &[String],
    stdin: Option<&str>,
) -> Result<std::process::Output> {
    use std::io::Write;
    use std::process::Stdio;
    let (program, args) = argv.split_first().context("empty verify step")?;
    let mut command = hermetic_command(root, wrapper, program)?;
    command
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| format!("run {program}"))?;
    if let Some(text) = stdin {
        // Dropped at the end of this statement, which closes the pipe (EOF).
        child
            .stdin
            .take()
            .context("stdin pipe")?
            .write_all(text.as_bytes())
            .with_context(|| format!("write stdin to {program}"))?;
    }
    let out = child
        .wait_with_output()
        .with_context(|| format!("run {program}"))?;
    if !out.status.success() {
        bail!(
            "exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out)
}

/// `program` with a CLEARED environment rooted at `root`: HOME and the XDG dirs
/// inside it, PATH and the locale passed through, nothing else.
fn hermetic_command(
    root: &Path,
    wrapper: Option<&Path>,
    program: impl AsRef<OsStr>,
) -> Result<Command> {
    let mut command = match wrapper {
        Some(wrapper) => {
            let mut c = Command::new(wrapper);
            c.arg(program);
            c
        }
        None => Command::new(program),
    };
    command.current_dir(root).env_clear();
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
    Ok(command)
}
