//! Read-only preflight: what will stop `caboodle install` on this host, before
//! anything is downloaded, built, or started.
//!
//! Every check here inspects the platform, PATH, the MCP registrations the agent
//! CLI reports, and the Quipu server named by `QUIPU_SERVER`: health, one read,
//! and an authenticated no-op write probe that Quipu refuses before writing
//! anything. Nothing is installed, written, or registered, so running it first is
//! always safe. A `fail` line is a blocker `install` would hit; a `warn` line is
//! something that works but will surprise you.

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
    if scope.tools.contains(&ToolName::Camayoc) || scope.tools.contains(&ToolName::Quipu) {
        findings.extend(check_graph());
    }
    findings.extend(check_mcp(scope));
    findings
}

/// MCP servers an agent needs for the selected tools, by the names the install
/// docs register them under (`claude mcp add bobbin|yupana ...`).
fn expected_mcp_servers(scope: &Scope) -> Vec<&'static str> {
    let mut names = Vec::new();
    if scope.tools.contains(&ToolName::Bobbin) {
        names.push("bobbin");
    }
    if scope.tools.contains(&ToolName::Yupana) {
        names.push("yupana");
    }
    names
}

/// Parse `claude mcp list`: `name: target - ✔ Connected` per server.
pub(crate) fn parse_mcp_list(text: &str) -> Vec<(String, bool)> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(": ")?;
            let (_, status) = rest.rsplit_once(" - ")?;
            let name = name.trim();
            if name.is_empty() || name.contains(' ') && !name.starts_with("claude.ai") {
                return None;
            }
            Some((
                name.to_owned(),
                status.contains("Connected") && !status.contains("Failed"),
            ))
        })
        .collect()
}

pub(crate) fn mcp_findings(expected: &[&str], listed: &[(String, bool)]) -> Vec<Finding> {
    expected
        .iter()
        .map(|want| match listed.iter().find(|(name, _)| name == want) {
            Some((_, true)) => Finding::new(Level::Ok, format!("mcp {want}"), "registered and connected"),
            Some((_, false)) => Finding::new(
                Level::Warn,
                format!("mcp {want}"),
                "registered but NOT connecting; agents in this directory get no tools from it. Run `claude mcp list` for the error",
            ),
            None => Finding::new(
                Level::Warn,
                format!("mcp {want}"),
                format!("not registered for this directory; agents here get no {want} tools. After install: `claude mcp add {want} -- {want} serve`"),
            ),
        })
        .collect()
}

fn check_mcp(scope: &Scope) -> Vec<Finding> {
    let expected = expected_mcp_servers(scope);
    if expected.is_empty() {
        return Vec::new();
    }
    if which_all("claude", env::var_os("PATH").as_deref()).is_empty() {
        return vec![Finding::new(
            Level::Warn,
            "mcp",
            "cannot check MCP registration: the `claude` CLI is not on PATH",
        )];
    }
    match Command::new("claude")
        .args(["mcp", "list"])
        .stdin(Stdio::null())
        .output()
    {
        Ok(out) if out.status.success() => mcp_findings(
            &expected,
            &parse_mcp_list(&String::from_utf8_lossy(&out.stdout)),
        ),
        Ok(out) => vec![Finding::new(
            Level::Warn,
            "mcp",
            format!(
                "`claude mcp list` failed ({}); MCP registration unknown",
                out.status
            ),
        )],
        Err(error) => vec![Finding::new(
            Level::Warn,
            "mcp",
            format!("could not run `claude mcp list`: {error}; MCP registration unknown"),
        )],
    }
}

/// What the no-op write probe's answer means. Quipu authorizes before parsing.
pub(crate) fn classify_write_probe(code: u16, body: &str, token_set: bool) -> Finding {
    let token = if token_set {
        "token from environment or configured file"
    } else {
        "no token (environment/file empty or absent)"
    };
    let data: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let error = data["error"].as_str().unwrap_or("");
    match code {
        403 if data["reason"] == "server_is_read_only" => Finding::new(
            Level::Warn, "quipu write",
            "READ-ONLY: no credential can authorize writes; use the intended writable server",
        ),
        400 if error.starts_with("invalid episode JSON:") && error.contains("missing field") => Finding::new(
            Level::Ok,
            "quipu write",
            format!("authorized ({token}); the probe was refused as an empty episode, nothing written"),
        ),
        401 => Finding::new(
            Level::Warn,
            "quipu write",
            format!("{}: REFUSED with HTTP {code} ({token}). Obtain an accepted credential from the server administrator; install at ~/.config/quipu/token (mode 0400), or set QUIPU_AUTH_TOKEN_FILE. QUIPU_AUTH_TOKEN overrides the file", if token_set { "BAD-TOKEN" } else { "NO-TOKEN" }),
        ),
        0 => Finding::new(Level::Warn, "quipu write", "write probe got no HTTP answer; write path unknown"),
        other => Finding::new(
            Level::Warn,
            "quipu write",
            format!("write probe returned an unexpected HTTP {other} ({token}); write path unknown"),
        ),
    }
}

fn check_graph() -> Vec<Finding> {
    let explicit = env::var("QUIPU_SERVER").ok();
    let server = explicit
        .clone()
        .unwrap_or_else(|| "http://localhost:3030".to_owned());
    let mut findings = vec![check_quipu_server()];
    let reachable = findings[0].detail.contains("is live");
    if !reachable {
        return findings;
    }
    let base = server.trim_end_matches('/');
    // The namespace is the setting people most often get wrong, so echo it and
    // check the server actually holds facts under it.
    match adapter::camayoc_root()
        .ok()
        .map(|root| root.join("ontology/core.ttl"))
        .filter(|path| path.exists())
        .map(|path| adapter::camayoc_aegis_namespace(&path))
    {
        Some(Ok(namespace)) => {
            // Subject-bound on a class camayoc's ontology declares: an index hit.
            // A prefix FILTER over `?s ?p ?o` scans the store and 408s on a real
            // graph (measured), which would report a healthy server as broken.
            let query = format!("SELECT ?p WHERE {{ <{namespace}Verification> ?p ?o }} LIMIT 1");
            findings.push(
                match adapter::curl_json(&format!("{base}/query"), &serde_json::json!({ "query": query })) {
                    Ok(answer) if answer["count"].as_u64().unwrap_or(0) > 0 => Finding::new(
                        Level::Ok,
                        "quipu namespace",
                        format!("{namespace} on {server}: read OK, camayoc's ontology is loaded here"),
                    ),
                    Ok(_) => Finding::new(
                        Level::Warn,
                        "quipu namespace",
                        format!("{namespace} on {server}: read OK but camayoc's ontology is NOT loaded here. Either it was never bootstrapped on this server, or this is the wrong server or namespace"),
                    ),
                    Err(error) => Finding::new(
                        Level::Warn,
                        "quipu namespace",
                        format!("{namespace} on {server}: read FAILED: {}", one_line(&format!("{error:#}"))),
                    ),
                },
            );
        }
        Some(Err(error)) => findings.push(Finding::new(
            Level::Warn,
            "quipu namespace",
            format!(
                "cannot resolve camayoc's namespace: {}",
                one_line(&format!("{error:#}"))
            ),
        )),
        None => findings.push(Finding::new(
            Level::Ok,
            "quipu namespace",
            "camayoc not installed yet; namespace resolves after install",
        )),
    }
    findings.push(match adapter::quipu_write_probe(base) {
        Ok((code, body, token_set)) => classify_write_probe(code, &body, token_set),
        Err(error) => Finding::new(
            Level::Warn,
            "quipu write",
            format!(
                "write probe could not run: {}",
                one_line(&format!("{error:#}"))
            ),
        ),
    });
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

    // Verbatim shape of `claude mcp list` (servers renamed, targets shortened).
    const MCP_LIST: &str = "Checking MCP server health…\n\n\
claude.ai Gmail: https://gmailmcp.example/mcp/v1 - ✔ Connected\n\
bobbin: http://bobbin-mcp.example/mcp (HTTP) - ✔ Connected\n\
yupana: yupana serve - ✗ Failed to connect\n";

    #[test]
    fn mcp_list_parses_connected_and_failed_servers() {
        let listed = parse_mcp_list(MCP_LIST);
        assert!(listed.contains(&("bobbin".to_owned(), true)));
        assert!(listed.contains(&("yupana".to_owned(), false)));
        assert!(!listed.iter().any(|(name, _)| name.starts_with("Checking")));
    }

    #[test]
    fn mcp_findings_distinguish_connected_failing_and_missing() {
        let listed = parse_mcp_list(MCP_LIST);
        let got = mcp_findings(&["bobbin", "yupana", "quipu"], &listed);
        assert_eq!(got[0].level, Level::Ok);
        assert_eq!(got[1].level, Level::Warn);
        assert!(got[1].detail.contains("NOT connecting"));
        assert_eq!(got[2].level, Level::Warn);
        assert!(got[2].detail.contains("not registered"));
        // A clean install with zero servers: every expected one is reported, none silently.
        assert!(mcp_findings(&["bobbin"], &[])
            .iter()
            .all(|f| f.level == Level::Warn));
    }

    #[test]
    fn write_probe_is_authorized_only_on_a_validation_refusal() {
        let ok = classify_write_probe(
            400,
            r#"{"error":"invalid episode JSON: missing field `name`"}"#,
            true,
        );
        assert_eq!(ok.level, Level::Ok);
        assert!(ok.detail.contains("nothing written"));

        let refused = classify_write_probe(401, r#"{"error":"unauthorized"}"#, false);
        assert_eq!(refused.level, Level::Warn);
        assert!(refused.detail.contains("REFUSED") && refused.detail.contains("NO-TOKEN"));

        assert!(classify_write_probe(401, "{}", true)
            .detail
            .contains("BAD-TOKEN"));
        assert!(
            classify_write_probe(403, r#"{"reason":"server_is_read_only"}"#, true)
                .detail
                .contains("READ-ONLY")
        );
        assert_eq!(
            classify_write_probe(400, r#"{"error":"episode failed"}"#, true).level,
            Level::Warn
        );
        // A 400 for some other reason is not proof of authorization.
        assert_eq!(
            classify_write_probe(400, "bad gateway html", true).level,
            Level::Warn
        );
        assert_eq!(classify_write_probe(0, "", true).level, Level::Warn);
        assert_eq!(classify_write_probe(200, "{}", true).level, Level::Warn);
    }

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
