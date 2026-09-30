//! Register Quipu's MCP endpoint with Claude Code without putting the secret in
//! any configuration file.
//!
//! A direct `{type: http, url}` entry sends no bearer, so every MCP write is
//! refused with 401 while reads keep working (aegis-nvw6ye). Claude Code's
//! `headersHelper` runs a command at connect time and uses the JSON it prints
//! as request headers. `apply` installs that command and registers the entry;
//! `verify` proves an authenticated MCP write reaches Quipu's parser, with an
//! unauthenticated positive control.
use std::{
    env,
    ffi::OsString,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub const SERVER_NAME: &str = "quipu";
const HELPER_NAME: &str = "quipu-mcp-headers";

/// Mirrors `quipu_auth::resolve`: QUIPU_AUTH_TOKEN, then QUIPU_AUTH_TOKEN_FILE,
/// then ~/.config/quipu/token. The token is read at connect time and is never
/// written into this script or into the Claude configuration.
pub const HELPER_SCRIPT: &str = r#"#!/bin/sh
# Installed by caboodle. Prints Quipu's MCP Authorization header at connect time.
# Token precedence matches caboodle: QUIPU_AUTH_TOKEN, then QUIPU_AUTH_TOKEN_FILE,
# then ~/.config/quipu/token. The secret is never stored in this file.
set -u
token=${QUIPU_AUTH_TOKEN:-}
if [ -z "$token" ]; then
    file=${QUIPU_AUTH_TOKEN_FILE:-}
    [ -n "$file" ] || file="${HOME:-}/.config/quipu/token"
    if [ ! -e "$file" ]; then
        echo "quipu-mcp-headers: no Quipu token: set QUIPU_AUTH_TOKEN_FILE or install ~/.config/quipu/token" >&2
        exit 1
    fi
    if ! token=$(cat "$file"); then
        echo "quipu-mcp-headers: cannot read the Quipu token file" >&2
        exit 1
    fi
    token=$(printf '%s' "$token" | tr -d ' \t\r\n')
fi
if [ -z "$token" ]; then
    echo "quipu-mcp-headers: the Quipu token is empty" >&2
    exit 1
fi
case $token in
    *[!A-Za-z0-9._~+/=-]*)
        echo "quipu-mcp-headers: the Quipu token has characters that cannot be sent as a header" >&2
        exit 1
        ;;
esac
printf '{"Authorization":"Bearer %s"}\n' "$token"
"#;

pub fn helper_path() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is not set; cannot place the Quipu MCP helper")?;
    Ok(Path::new(&home).join(".local/bin").join(HELPER_NAME))
}

/// The `/mcp` endpoint for a plan's Quipu URL, which may already name it.
pub fn endpoint(url: &str) -> String {
    let base = url.trim_end_matches('/');
    if base.ends_with("/mcp") {
        base.to_owned()
    } else {
        format!("{base}/mcp")
    }
}

pub fn desired_entry(url: &str, helper: &Path) -> Value {
    json!({"type": "http", "url": endpoint(url), "headersHelper": helper.to_string_lossy()})
}

fn claude_config_path() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Ok(Path::new(&dir).join(".claude.json"));
    }
    let home = env::var_os("HOME").context("HOME is not set; cannot read Claude configuration")?;
    Ok(Path::new(&home).join(".claude.json"))
}

fn read_claude_config() -> Result<Value> {
    let path = claude_config_path()?;
    match fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str(&body)
            .with_context(|| format!("parse Claude configuration {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

/// Every `quipu` entry Claude Code could use: user scope plus each project's
/// local scope. A local entry shadows the user entry in its directory.
pub fn quipu_entries(config: &Value) -> Vec<(String, Value)> {
    let mut found = Vec::new();
    if let Some(entry) = config["mcpServers"].get(SERVER_NAME) {
        found.push(("user scope".to_owned(), entry.clone()));
    }
    if let Some(projects) = config["projects"].as_object() {
        for (dir, project) in projects {
            if let Some(entry) = project["mcpServers"].get(SERVER_NAME) {
                found.push((format!("local scope for {dir}"), entry.clone()));
            }
        }
    }
    found
}

fn write_helper(path: &Path) -> Result<bool> {
    if fs::read_to_string(path).is_ok_and(|body| body == HELPER_SCRIPT)
        && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o777 == 0o755)
    {
        return Ok(false);
    }
    let dir = path
        .parent()
        .context("helper path has no parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).context("create helper temp file")?;
    temp.write_all(HELPER_SCRIPT.as_bytes())
        .context("write Quipu MCP helper")?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755))
        .context("make Quipu MCP helper executable")?;
    temp.persist(path)
        .with_context(|| format!("install {}", path.display()))?;
    Ok(true)
}

fn claude(args: &[OsString]) -> Result<()> {
    let out = Command::new("claude")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("run the `claude` CLI")?;
    if !out.status.success() {
        bail!(
            "`claude {}` failed ({}): {}",
            args.iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Install the helper and register the user-scope entry. Idempotent; replaces
/// a user-scope entry that differs (for example one without a headersHelper).
pub fn provision(url: &str) -> Result<String> {
    let helper = helper_path()?;
    let wrote = write_helper(&helper)?;
    let desired = desired_entry(url, &helper);
    let existing = read_claude_config()?["mcpServers"]
        .get(SERVER_NAME)
        .cloned();
    let mut notes = Vec::new();
    if wrote {
        notes.push(format!("helper installed at {}", helper.display()));
    }
    match existing {
        Some(entry) if entry == desired => {}
        Some(entry) => {
            notes.push(if entry.get("headersHelper").is_none() {
                "replaced a user-scope quipu entry that had no headersHelper (MCP writes were unauthenticated)".to_owned()
            } else {
                "replaced a user-scope quipu entry that differed from the plan".to_owned()
            });
            claude(&["mcp", "remove", SERVER_NAME, "-s", "user"].map(OsString::from))?;
            add(&desired)?;
        }
        None => {
            notes.push("registered the user-scope quipu entry".to_owned());
            add(&desired)?;
        }
    }
    let registered = read_claude_config()?["mcpServers"]
        .get(SERVER_NAME)
        .cloned();
    if registered.as_ref() != Some(&desired) {
        bail!("`claude mcp add-json` returned success but the user-scope quipu entry does not read back as planned");
    }
    for (scope, entry) in quipu_entries(&read_claude_config()?) {
        if scope != "user scope" && entry.get("headersHelper").is_none() {
            notes.push(format!(
                "WARNING: a quipu entry in {scope} has no headersHelper and shadows the user entry there; remove it with `claude mcp remove quipu -s local` in that directory"
            ));
        }
    }
    Ok(if notes.is_empty() {
        "already provisioned".to_owned()
    } else {
        notes.join("; ")
    })
}

fn add(entry: &Value) -> Result<()> {
    claude(&[
        OsString::from("mcp"),
        OsString::from("add-json"),
        OsString::from(SERVER_NAME),
        OsString::from(entry.to_string()),
        OsString::from("-s"),
        OsString::from("user"),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeAnswer {
    /// The call reached Quipu's Turtle parser: the bearer was accepted.
    Parsed,
    /// Quipu refused the call for a missing or invalid bearer.
    Unauthorized,
    /// Anything else, including no answer at all.
    Other,
}

/// Classify a `tools/call` answer. Quipu wraps REST errors as a tool result,
/// and the transport is SSE, so the HTTP status says nothing: read the body.
pub fn classify(body: &str) -> ProbeAnswer {
    let payloads: Vec<&str> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .collect();
    let candidates = if payloads.is_empty() {
        vec![body.trim()]
    } else {
        payloads
    };
    for payload in candidates {
        let Ok(message) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        let Some(text) = message["result"]["content"][0]["text"].as_str() else {
            continue;
        };
        let inner: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        if inner["reason"] == "missing_or_invalid_bearer_token" {
            return ProbeAnswer::Unauthorized;
        }
        if inner["error"]
            .as_str()
            .is_some_and(|e| e.starts_with("RDF parse error"))
        {
            return ProbeAnswer::Parsed;
        }
    }
    ProbeAnswer::Other
}

/// Invalid Turtle: Quipu checks the bearer first, then fails to parse, so an
/// accepted credential writes nothing.
const PROBE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"quipu_knot","arguments":{"turtle":"caboodle auth probe: not turtle <<<"}}}"#;

fn post(endpoint: &str, token: Option<&str>) -> Result<String> {
    let body_file = tempfile::NamedTempFile::new().context("create MCP probe output file")?;
    let auth = match token {
        Some(token) => {
            let file = tempfile::NamedTempFile::new().context("create MCP probe auth config")?;
            fs::write(file.path(), crate::adapter::curl_auth_header_line(token)?)
                .context("write MCP probe auth config")?;
            Some(file)
        }
        None => None,
    };
    let mut args: Vec<OsString> = [
        "--silent",
        "--max-time",
        "20",
        "--request",
        "POST",
        "--header",
        "Content-Type: application/json",
        "--header",
        "Accept: application/json, text/event-stream",
        "--header",
        "X-Quipu-Client: caboodle-verify",
        "--data",
        PROBE,
        "--output",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(body_file.path().as_os_str().to_owned());
    if let Some(file) = &auth {
        args.push(OsString::from("--config"));
        args.push(file.path().as_os_str().to_owned());
    }
    args.push(OsString::from(endpoint));
    let status = Command::new("curl")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("run curl for the Quipu MCP probe")?;
    if !status.success() {
        bail!("curl could not reach {endpoint} ({status})");
    }
    Ok(fs::read_to_string(body_file.path()).unwrap_or_default())
}

/// Run the registered helper exactly as Claude Code would and return the
/// bearer it supplies.
fn helper_token(helper: &str) -> Result<String> {
    let out = Command::new(helper)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run headersHelper {helper}"))?;
    if !out.status.success() {
        bail!(
            "headersHelper {helper} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let headers: Value = serde_json::from_slice(&out.stdout)
        .with_context(|| format!("headersHelper {helper} did not print a JSON object"))?;
    let value = headers["Authorization"]
        .as_str()
        .context("headersHelper printed no Authorization header")?;
    let token = value
        .strip_prefix("Bearer ")
        .context("headersHelper Authorization is not a Bearer credential")?;
    Ok(token.to_owned())
}

/// Prove the registered entry authenticates MCP writes. Every outcome other
/// than PASS is an error that names FAIL or UNKNOWN.
pub fn verify(url: &str) -> Result<()> {
    let helper = helper_path()?;
    let desired = desired_entry(url, &helper);
    let config = read_claude_config()?;
    let entries = quipu_entries(&config);
    match entries.iter().find(|(scope, _)| scope == "user scope") {
        None => bail!("FAIL: no user-scope quipu MCP entry is registered; run `caboodle apply`"),
        Some((_, entry)) if entry.get("headersHelper").is_none() => bail!(
            "FAIL: the user-scope quipu MCP entry has no headersHelper, so MCP writes carry no bearer; run `caboodle apply`"
        ),
        Some((_, entry)) if entry != &desired => bail!(
            "FAIL: the user-scope quipu MCP entry differs from the plan (expected {desired}, found {entry}); run `caboodle apply`"
        ),
        Some(_) => {}
    }
    for (scope, entry) in &entries {
        if entry.get("headersHelper").is_none() {
            bail!("FAIL: a quipu MCP entry in {scope} has no headersHelper and shadows the user entry there");
        }
    }
    let target = endpoint(url);
    // Positive control first: without a credential this server must refuse,
    // or an accepted authenticated call proves nothing about the helper.
    match classify(&post(&target, None)?) {
        ProbeAnswer::Unauthorized => {}
        ProbeAnswer::Parsed => bail!(
            "UNKNOWN: {target} accepted an MCP write with NO credential, so it cannot show whether the helper's bearer works"
        ),
        ProbeAnswer::Other => bail!(
            "UNKNOWN: {target} gave no recognisable answer to the unauthenticated control; the MCP write path is untested"
        ),
    }
    let token =
        helper_token(&helper.to_string_lossy()).map_err(|e| anyhow::anyhow!("FAIL: {e:#}"))?;
    match classify(&post(&target, Some(&token))?) {
        ProbeAnswer::Parsed => Ok(()),
        ProbeAnswer::Unauthorized => bail!(
            "FAIL: {target} refused the headersHelper's bearer (missing_or_invalid_bearer_token); check the Quipu token"
        ),
        ProbeAnswer::Other => bail!(
            "UNKNOWN: the authenticated MCP probe to {target} gave no recognisable answer"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_helper(env: &[(&str, &str)], home: &Path) -> (bool, String) {
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join(HELPER_NAME);
        write_helper(&helper).unwrap();
        let mut command = Command::new(&helper);
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", home);
        for (key, value) in env {
            command.env(key, value);
        }
        let out = command.output().unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    #[test]
    fn helper_precedence_matches_quipu_auth() {
        let home = tempfile::tempdir().unwrap();
        let default = home.path().join(".config/quipu/token");
        fs::create_dir_all(default.parent().unwrap()).unwrap();
        fs::write(&default, "default\n").unwrap();
        let explicit = home.path().join("explicit");
        let bearer = |t: &str| format!("{{\"Authorization\":\"Bearer {t}\"}}\n");
        // 1. the default file when nothing is set
        assert_eq!(run_helper(&[], home.path()), (true, bearer("default")));
        // 2. an explicit file wins over the default, and a missing explicit file
        //    does NOT fall back to the default (quipu_auth returns None there)
        let file = explicit.to_str().unwrap();
        assert!(!run_helper(&[("QUIPU_AUTH_TOKEN_FILE", file)], home.path()).0);
        fs::write(&explicit, "first\n").unwrap();
        assert_eq!(
            run_helper(&[("QUIPU_AUTH_TOKEN_FILE", file)], home.path()),
            (true, bearer("first"))
        );
        // an empty QUIPU_AUTH_TOKEN falls through to the file
        assert_eq!(
            run_helper(
                &[("QUIPU_AUTH_TOKEN", ""), ("QUIPU_AUTH_TOKEN_FILE", file)],
                home.path()
            ),
            (true, bearer("first"))
        );
        // 3. the environment value wins over both files
        assert_eq!(
            run_helper(
                &[
                    ("QUIPU_AUTH_TOKEN", "override"),
                    ("QUIPU_AUTH_TOKEN_FILE", file)
                ],
                home.path()
            ),
            (true, bearer("override"))
        );
        // empty and unsendable tokens are refused, never printed
        fs::write(&explicit, "\n").unwrap();
        assert!(!run_helper(&[("QUIPU_AUTH_TOKEN_FILE", file)], home.path()).0);
        fs::write(&explicit, "a\"b").unwrap();
        assert!(!run_helper(&[("QUIPU_AUTH_TOKEN_FILE", file)], home.path()).0);
    }

    #[test]
    fn helper_holds_no_secret() {
        assert!(!HELPER_SCRIPT.contains("Bearer ey"));
        assert!(HELPER_SCRIPT.contains("QUIPU_AUTH_TOKEN_FILE"));
    }

    #[test]
    fn classifies_the_measured_quipu_answers() {
        let refused = r#"data: {"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"endpoint\":\"/knot\",\"error\":\"unauthorized: /knot is a WRITE endpoint\",\"reason\":\"missing_or_invalid_bearer_token\"}"}],"isError":true}}"#;
        let parsed = r#"data: {"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"error\":\"RDF parse error: Parser error at line 1\"}"}],"isError":true}}"#;
        assert_eq!(classify(refused), ProbeAnswer::Unauthorized);
        assert_eq!(classify(parsed), ProbeAnswer::Parsed);
        // plain JSON transport, and noise
        assert_eq!(
            classify(parsed.strip_prefix("data: ").unwrap()),
            ProbeAnswer::Parsed
        );
        assert_eq!(classify(""), ProbeAnswer::Other);
        assert_eq!(classify("<html>502</html>"), ProbeAnswer::Other);
    }

    #[test]
    fn endpoint_and_entry_shape() {
        assert_eq!(endpoint("http://quipu.example"), "http://quipu.example/mcp");
        assert_eq!(
            endpoint("http://quipu.example/mcp/"),
            "http://quipu.example/mcp"
        );
        let entry = desired_entry(
            "http://quipu.example",
            Path::new("/h/.local/bin/quipu-mcp-headers"),
        );
        assert_eq!(entry["type"], "http");
        assert_eq!(entry["headersHelper"], "/h/.local/bin/quipu-mcp-headers");
    }

    #[test]
    fn finds_shadowing_local_entries() {
        let config = json!({
            "mcpServers": {"quipu": {"type": "http", "url": "u", "headersHelper": "h"}},
            "projects": {"/w": {"mcpServers": {"quipu": {"type": "http", "url": "u"}}}}
        });
        let entries = quipu_entries(&config);
        assert_eq!(entries.len(), 2);
        assert!(entries[1].0.contains("/w") && entries[1].1.get("headersHelper").is_none());
    }
}
