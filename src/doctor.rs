//! Read-only preflight: what will stop `caboodle install` on this host, before
//! anything is downloaded, built, or started.
//!
//! Every check here inspects the platform, PATH, and one optional HTTP probe of
//! the Quipu server. Nothing is installed, written, or registered, so running it
//! first is always safe. A `fail` line is a blocker `install` would hit; a `warn`
//! line is something that works but will surprise you.

use std::{
    env,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::Result;

use crate::adapter::which_all;

use crate::{
    adapter,
    model::{CrewMode, Plan, Profile, QuipuFlavor, ToolName},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub level: Level,
    pub subject: String,
    pub detail: String,
}

impl Finding {
    fn new(level: Level, subject: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level,
            subject: subject.into(),
            detail: detail.into(),
        }
    }
}

/// What the doctor inspects. Built from a plan when one exists, else from the
/// `everything` profile so a first run reports every possible blocker.
pub struct Scope {
    pub tools: Vec<ToolName>,
    pub quipu_flavor: QuipuFlavor,
    pub crew: Option<CrewMode>,
}

impl Scope {
    pub fn from_plan(plan: &Plan) -> Self {
        Self {
            tools: plan.tools.clone(),
            quipu_flavor: plan.quipu_flavor,
            crew: plan.crew.as_ref().map(|crew| crew.mode),
        }
    }

    pub fn everything() -> Self {
        Self {
            tools: Profile::Everything.tools(),
            quipu_flavor: QuipuFlavor::Release,
            crew: None,
        }
    }
}

/// Versions such as quipu's span several lines; a report line keeps one.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn crew_prerequisites(mode: CrewMode) -> Vec<&'static str> {
    let mut commands = vec!["curl", "sha256sum"];
    if matches!(mode, CrewMode::Shantytown | CrewMode::Both) {
        commands.push("python3");
    }
    if matches!(mode, CrewMode::Creel | CrewMode::Both) {
        commands.push("tar");
    }
    commands
}

fn prerequisite_hint(command: &str) -> &'static str {
    match command {
        "sha256sum" => "older macOS lacks it: `brew install coreutils`; Debian/Ubuntu: coreutils",
        "go" => "Desire Path is built from source: install Go 1.22+ from https://go.dev/dl/",
        "cargo" => "install Rust from https://rustup.rs",
        "python3" => "install Python 3.9+ from your package manager",
        "git" | "curl" | "tar" | "bash" => "install it from your package manager",
        _ => "install it and rerun `caboodle doctor`",
    }
}

/// Run every check against the live environment.
pub fn diagnose(scope: &Scope) -> Vec<Finding> {
    let path = env::var_os("PATH");
    let mut findings = vec![Finding::new(
        Level::Ok,
        "platform",
        format!("{}-{}", env::consts::ARCH, env::consts::OS),
    )];
    findings.extend(check_bin_dir_on_path(
        adapter::managed_bin_dir().as_deref(),
        path.as_deref(),
    ));
    findings.extend(check_prerequisites(scope, path.as_deref()));
    for &tool in &scope.tools {
        findings.extend(check_tool(tool, scope.quipu_flavor, path.as_deref()));
    }
    if scope.tools.contains(&ToolName::Camayoc) {
        findings.push(check_quipu_server());
    }
    findings
}

fn check_bin_dir_on_path(bin: Option<&Path>, path: Option<&std::ffi::OsStr>) -> Vec<Finding> {
    let Some(bin) = bin else {
        return vec![Finding::new(
            Level::Fail,
            "install directory",
            "neither CARGO_HOME nor HOME is set; caboodle has nowhere to install binaries",
        )];
    };
    let on_path = path.is_some_and(|path| env::split_paths(path).any(|entry| entry == bin));
    if on_path {
        vec![Finding::new(
            Level::Ok,
            "install directory",
            format!("{} is on PATH", bin.display()),
        )]
    } else {
        vec![Finding::new(
            Level::Fail,
            "install directory",
            format!(
                "{} is not on PATH, so installed tools cannot be read back; add `export PATH=\"{}:$PATH\"` to your shell profile",
                bin.display(),
                bin.display()
            ),
        )]
    }
}

fn check_prerequisites(scope: &Scope, path: Option<&std::ffi::OsStr>) -> Vec<Finding> {
    let mut needed: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    let mut need = |command: &'static str, by: &'static str| match needed
        .iter_mut()
        .find(|(name, _)| *name == command)
    {
        Some((_, users)) if !users.contains(&by) => users.push(by),
        Some(_) => {}
        None => needed.push((command, vec![by])),
    };
    for &tool in &scope.tools {
        for &command in adapter::prerequisites(tool, scope.quipu_flavor) {
            need(command, tool.as_str());
        }
    }
    if let Some(mode) = scope.crew {
        for command in crew_prerequisites(mode) {
            need(command, "crew");
        }
    }
    needed
        .into_iter()
        .map(|(command, users)| match which_all(command, path).first() {
            Some(found) => Finding::new(
                Level::Ok,
                format!("prerequisite {command}"),
                found.display().to_string(),
            ),
            None => Finding::new(
                Level::Fail,
                format!("prerequisite {command}"),
                format!(
                    "not on PATH, needed by {}; {}",
                    users.join(", "),
                    prerequisite_hint(command)
                ),
            ),
        })
        .collect()
}

fn check_tool(tool: ToolName, flavor: QuipuFlavor, path: Option<&std::ffi::OsStr>) -> Vec<Finding> {
    let mut findings = Vec::new();
    let adapter = adapter::adapter(tool, flavor);
    let desired = adapter.desired_version();
    match adapter.version() {
        Ok(installed) if adapter.is_current(&installed) => findings.push(Finding::new(
            Level::Ok,
            tool.as_str(),
            format!("installed and current ({})", one_line(&installed)),
        )),
        Ok(installed) => findings.push(Finding::new(
            Level::Warn,
            tool.as_str(),
            format!(
                "installed ({}) differs from the reviewed {desired}; `caboodle check-updates` explains the drift",
                one_line(&installed)
            ),
        )),
        Err(_) => match adapter::release_blocker(tool, flavor) {
            Some(blocker) => findings.push(Finding::new(
                Level::Fail,
                tool.as_str(),
                format!("not installed and cannot be installed here: {blocker}. {}", source_hint(tool)),
            )),
            None => findings.push(Finding::new(
                Level::Ok,
                tool.as_str(),
                format!("not installed yet; install will fetch {desired}"),
            )),
        },
    }
    for &program in adapter::programs(tool) {
        let copies = which_all(program, path);
        if copies.len() > 1 {
            let listed = copies
                .iter()
                .map(|copy| copy.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(Finding::new(
                Level::Warn,
                format!("{program} on PATH"),
                format!(
                    "{} copies, the first one runs: {listed}. Remove the stale ones so a new install is the one you run",
                    copies.len()
                ),
            ));
        }
    }
    findings
}

fn source_hint(tool: ToolName) -> String {
    match adapter::source_install_command(tool) {
        Some(command) => format!("Build it from source first, then rerun: `{command}`"),
        None => "Install it by hand first, then rerun; caboodle adopts an installed tool that passes verification".to_owned(),
    }
}

fn check_quipu_server() -> Finding {
    let explicit = env::var("QUIPU_SERVER").ok();
    let server = explicit
        .clone()
        .unwrap_or_else(|| "http://localhost:3030".to_owned());
    let reachable = Command::new("curl")
        .args([
            "-sf",
            "-m",
            "2",
            &format!("{}/health", server.trim_end_matches('/')),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    match (reachable, explicit.is_some()) {
        (false, _) => Finding::new(
            Level::Ok,
            "quipu server",
            format!(
                "nothing answers at {server}; camayoc verification uses its own scratch server and never touches this address"
            ),
        ),
        (true, source) => Finding::new(
            Level::Ok,
            "quipu server",
            format!(
                "{server} ({}) is live. camayoc verification does not write to it (it uses its own scratch server); run camayoc's scripts/bootstrap.sh yourself to set this server up",
                if source { "from QUIPU_SERVER" } else { "the default" }
            ),
        ),
    }
}

/// Print findings and return true when nothing blocks an install.
pub fn report<W: Write>(findings: &[Finding], output: &mut W) -> Result<bool> {
    for finding in findings {
        let tag = match finding.level {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        };
        writeln!(output, "{tag} {}: {}", finding.subject, finding.detail)?;
    }
    let failures = findings
        .iter()
        .filter(|finding| finding.level == Level::Fail)
        .count();
    let warnings = findings
        .iter()
        .filter(|finding| finding.level == Level::Warn)
        .count();
    if failures == 0 {
        writeln!(
            output,
            "doctor: ready to install ({warnings} warning{})",
            if warnings == 1 { "" } else { "s" }
        )?;
    } else {
        writeln!(
            output,
            "doctor: {failures} blocker{} — fix the FAIL lines, then rerun `caboodle doctor`",
            if failures == 1 { "" } else { "s" }
        )?;
    }
    Ok(failures == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, fs, path::PathBuf};

    fn executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn which_all_lists_every_copy_in_path_order() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let a = executable(first.path(), "yupana");
        let b = executable(second.path(), "yupana");
        fs::write(second.path().join("not-executable"), "").unwrap();
        let path = env::join_paths([first.path(), second.path(), first.path()]).unwrap();
        assert_eq!(which_all("yupana", Some(&path)), vec![a, b]);
        assert!(which_all("not-executable", Some(&path)).is_empty());
        assert!(which_all("yupana", None).is_empty());
    }

    #[test]
    fn missing_install_directory_on_path_is_a_blocker() {
        let bin = PathBuf::from("/opt/caboodle-test/bin");
        let without = OsString::from("/usr/bin:/bin");
        let with = env::join_paths([Path::new("/usr/bin"), &bin]).unwrap();
        let missing = check_bin_dir_on_path(Some(&bin), Some(&without));
        assert_eq!(missing[0].level, Level::Fail);
        assert!(missing[0].detail.contains("export PATH="));
        assert_eq!(
            check_bin_dir_on_path(Some(&bin), Some(&with))[0].level,
            Level::Ok
        );
    }

    #[test]
    fn missing_prerequisite_names_the_tools_that_need_it() {
        let empty = tempfile::tempdir().unwrap();
        let path = env::join_paths([empty.path()]).unwrap();
        let findings = check_prerequisites(&Scope::everything(), Some(&path));
        let go = findings
            .iter()
            .find(|finding| finding.subject == "prerequisite go")
            .expect("desire-path needs go");
        assert_eq!(go.level, Level::Fail);
        assert!(go.detail.contains("desire-path"));
        let curl = findings
            .iter()
            .find(|finding| finding.subject == "prerequisite curl")
            .unwrap();
        assert!(curl.detail.contains("quipu") && curl.detail.contains("bobbin"));
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.subject == "prerequisite curl")
                .count(),
            1,
            "each command is reported once"
        );
    }

    #[test]
    fn report_fails_only_on_blockers() {
        let mut out = Vec::new();
        let warn_only = [Finding::new(Level::Warn, "x", "y")];
        assert!(report(&warn_only, &mut out).unwrap());
        let blocked = [Finding::new(Level::Fail, "x", "y")];
        assert!(!report(&blocked, &mut out).unwrap());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("ready to install (1 warning)"));
        assert!(text.contains("FAIL x: y"));
    }
}
