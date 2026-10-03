//! The Quipu stack's hook bundles, and the verification that st renders them.
//!
//! Each tool's hooks are declared as a bundle (schema `st.hook-bundle/1`) under
//! `hook-bundles/`. CABOODLE ships them; st owns rendering them into every
//! emitted role file and checking them (aegis-u1ybxo, aegis-68j0ys). Here
//! verification asks st's own check whether each bundle is configured.

use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// The bundles CABOODLE ships, as `(name, bundle JSON)`.
pub const BUNDLES: &[(&str, &str)] = &[
    ("bobbin", include_str!("../hook-bundles/bobbin.json")),
    (
        "desire-path",
        include_str!("../hook-bundles/desire-path.json"),
    ),
    ("yupana", include_str!("../hook-bundles/yupana.json")),
    ("quipu", include_str!("../hook-bundles/quipu.json")),
];

/// The shipped bundles whose tool this plan installs. A bundle for a tool the
/// host does not run would render hooks calling a missing binary, failing on
/// every tool call, so neither registration nor verification includes it.
pub fn selected<'a>(tools: impl IntoIterator<Item = &'a str>) -> Vec<(&'static str, &'static str)> {
    let tools: Vec<&str> = tools.into_iter().collect();
    BUNDLES
        .iter()
        .filter(|(name, _)| tools.contains(name))
        .copied()
        .collect()
}

/// Hook commands in `bundle` that START with an explicit path (`/`, `~/` or
/// `$HOME/`) naming no executable on this host. Such a hook fails on every
/// event it fires on, which is worse than not registering it: the quipu
/// bundle's `$HOME/.gt/hooks/quipu-session-capture.sh` exists only on a Gas
/// Town host (measured on a fresh Mac, aegis-u1ybxo). Bare program names are
/// not checked: they are the plan's own tools, installed by apply before this.
pub fn missing_executables(bundle: &str, home: Option<&std::path::Path>) -> Vec<String> {
    let Ok(obj) = serde_json::from_str::<Value>(bundle) else {
        return Vec::new();
    };
    let mut missing = Vec::new();
    for hook in obj
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(cmd) = hook.get("command").and_then(Value::as_str) else {
            continue;
        };
        let first = cmd
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(['"', '\'']);
        let path = if let Some(rest) = first.strip_prefix("$HOME/").or(first.strip_prefix("~/")) {
            match home {
                Some(h) => h.join(rest),
                None => {
                    missing.push(first.to_string());
                    continue;
                }
            }
        } else if first.starts_with('/') {
            std::path::PathBuf::from(first)
        } else {
            continue;
        };
        let executable = std::fs::metadata(&path).is_ok_and(|m| {
            use std::os::unix::fs::PermissionsExt;
            m.is_file() && m.permissions().mode() & 0o111 != 0
        });
        if !executable && !missing.contains(&first.to_string()) {
            missing.push(first.to_string());
        }
    }
    missing
}

/// Register `bundles` with st through its generic interface (`st ops hooks
/// register <file>`, aegis-u1ybxo scope a). Idempotent: st compares the
/// registered copy and answers `unchanged` without re-rendering anything.
///
/// When `st` is not installed this is a NOTE, not a failure: st is optional on
/// a host, and `caboodle verify` reports the bundles as unregistered there. A
/// bundle st REFUSES, or an answer outside st's contract, fails: that is a
/// defect in the shipped bundle or a changed contract, never something to
/// skip. Returns one report line per bundle.
pub fn register(bundles: &[(&str, &str)], st: &str) -> Result<Vec<String>> {
    if bundles.is_empty() {
        return Ok(vec!["none selected by this plan".into()]);
    }
    let mut lines = Vec::new();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    for (name, json) in bundles {
        let missing = missing_executables(json, home.as_deref());
        if !missing.is_empty() {
            lines.push(format!(
                "{name}: NOT registered on this host: hook command {} is not an executable \
                 here (it would fail on every event)",
                missing.join(", ")
            ));
            continue;
        }
        let mut file = tempfile::Builder::new()
            .prefix(&format!("caboodle-{name}-"))
            .suffix(".json")
            .tempfile()
            .context("create a temporary bundle file")?;
        std::io::Write::write_all(&mut file, json.as_bytes())
            .with_context(|| format!("write bundle {name}"))?;
        let output = match Command::new(st)
            .args(["ops", "hooks", "register"])
            .arg(file.path())
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(vec![format!(
                    "NOT registered: `{st}` (shantytown) is not installed on this host. \
                     Install it and rerun `caboodle apply`; verify reports the bundles until then"
                )]);
            }
            Err(error) => {
                return Err(error).with_context(|| format!("run {st} ops hooks register"))
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            bail!(
                "st refused hook bundle {name} (exit {:?}): {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let first = stdout.lines().next().unwrap_or("").trim();
        let outcome = first
            .strip_prefix(&format!("{name}: "))
            .filter(|o| matches!(*o, "installed" | "updated" | "unchanged"))
            .with_context(|| {
                format!("st answered {first:?} registering {name}; expected `{name}: installed|updated|unchanged`")
            })?;
        lines.push(format!("{name}: {outcome}"));
    }
    Ok(lines)
}

/// Judge one `st ops hooks check --json` report (schema `st.hook-check/1`)
/// against the shipped bundles.
///
/// Each bundle fails verification when it is absent from the report, when it
/// is registered at a different version, or when any of its items is not
/// `configured: ok` (missing, duplicate, unsupported). A running agent that
/// launched before the emit (`live: stale`/`unknown`) is REPORTED and does not
/// fail. Only a relaunch delivers it, and verify must not demand a fleet
/// restart. Returns the report lines on success.
pub fn assess(bundles: &[(&str, &str)], report: &Value) -> Result<Vec<String>> {
    if report.get("schema").and_then(Value::as_str) != Some("st.hook-check/1") {
        bail!("st hook check returned an unrecognised report schema");
    }
    if let Some(errors) = report
        .get("registry_errors")
        .and_then(Value::as_array)
        .filter(|e| !e.is_empty())
    {
        bail!("st cannot read its hook-bundle registry: {errors:?}");
    }
    let items = report
        .get("items")
        .and_then(Value::as_array)
        .context("st hook check report has no items array")?;
    let mut lines = Vec::new();
    let mut failures = Vec::new();
    for (name, json) in bundles {
        let bundle: Value =
            serde_json::from_str(json).with_context(|| format!("parse shipped bundle {name}"))?;
        let want = bundle.get("version").and_then(Value::as_str).unwrap_or("");
        let mine: Vec<&Value> = items
            .iter()
            .filter(|item| item.get("bundle").and_then(Value::as_str) == Some(name))
            .collect();
        if mine.is_empty() {
            failures.push(format!("{name}: not registered with st"));
            continue;
        }
        let versions: Vec<&str> = mine
            .iter()
            .filter_map(|item| item.get("version").and_then(Value::as_str))
            .collect();
        if versions.iter().any(|v| *v != want) {
            failures.push(format!(
                "{name}: registered version {} differs from shipped {want}",
                versions.first().copied().unwrap_or("?")
            ));
            continue;
        }
        let bad: Vec<String> = mine
            .iter()
            .filter(|item| item.get("configured").and_then(Value::as_str) != Some("ok"))
            .map(|item| {
                format!(
                    "{} {} {}",
                    item.get("event").and_then(Value::as_str).unwrap_or("?"),
                    item.get("role").and_then(Value::as_str).unwrap_or("?"),
                    item.get("configured")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                )
            })
            .collect();
        if !bad.is_empty() {
            failures.push(format!("{name}: {}", bad.join("; ")));
            continue;
        }
        let stale = mine
            .iter()
            .filter(|item| {
                matches!(
                    item.get("live").and_then(Value::as_str),
                    Some("stale" | "unknown")
                )
            })
            .count();
        let mut line = format!("{name}: configured in {} emitted item(s)", mine.len());
        if stale > 0 {
            line.push_str(&format!(
                "; {stale} item(s) not yet live in running agents (delivered at relaunch)"
            ));
        }
        lines.push(line);
    }
    if !failures.is_empty() {
        bail!("hook bundles not verified: {}", failures.join(" | "));
    }
    Ok(lines)
}

/// Run st's check and assess `bundles` (the plan's selection) against it.
///
/// Symmetric with `register`: no selected bundle is a note, and so is a host
/// without st (st is optional; apply printed the same note). Only a host WITH
/// st can be asked, and there every applicable bundle must be configured.
pub fn verify(bundles: &[(&str, &str)]) -> Result<Vec<String>> {
    verify_with(bundles, "st")
}

fn verify_with(bundles: &[(&str, &str)], st: &str) -> Result<Vec<String>> {
    if bundles.is_empty() {
        return Ok(vec!["none selected by this plan".into()]);
    }
    // Not `checked`: st exits 1 for drift (including live staleness) and 2 when
    // it cannot tell. Exit 2 is also what st returns when it cannot read what
    // ONE running agent carries, with every item configured ok (aegis-331f7p,
    // measured 2026-09-30). The JSON is the evidence either way; `assess`
    // decides, so an item st could not judge still fails on `configured`.
    let output = match Command::new(st)
        .args(["ops", "hooks", "check", "--json"])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(vec![format!(
                "NOT verified: `{st}` (shantytown) is not installed on this host, so no \
                 hook bundle is registered here"
            )]);
        }
        Err(error) => {
            return Err(error).with_context(|| format!("run {st} ops hooks check --json"))
        }
    };
    let report: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "st ops hooks check --json gave no readable report (exit {:?})",
            output.status.code()
        )
    })?;
    // A bundle whose hook command cannot exist on this host was not registered
    // (see `register`); demanding it here would be the same impossible ask.
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let (applicable, absent): (Vec<_>, Vec<_>) = bundles
        .iter()
        .copied()
        .partition(|(_, json)| missing_executables(json, home.as_deref()).is_empty());
    let mut lines = assess(&applicable, &report)?;
    for (name, json) in absent {
        lines.push(format!(
            "{name}: not applicable on this host (hook command {} is not an executable here)",
            missing_executables(json, home.as_deref()).join(", ")
        ));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {

    /// A fake `st` that copies the bundle file it was given to `got.json`,
    /// prints `out` and exits `code`.
    fn fake_st(dir: &std::path::Path, out: &str, code: i32) -> String {
        use std::os::unix::fs::PermissionsExt;
        // A distinct file per fake: rewriting one path that a parallel test's
        // fork still holds open is the classic ETXTBSY race.
        let path = dir.join(format!("st-{}-{code}", out.replace([' ', ':'], "_")));
        let got = dir.join("got.json");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\ncp \"$4\" '{}'\necho '{out}'\necho 'refused: bad bundle' >&2\nexit {code}\n",
                got.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    /// `register` against a just-written fake, retrying the exec race
    /// ("Text file busy") that parallel tests can cause. Test-only.
    fn reg(bundles: &[(&str, &str)], st: &str) -> Result<Vec<String>> {
        for _ in 0..5 {
            match register(bundles, st) {
                Err(e) if format!("{e:#}").contains("Text file busy") => {
                    std::thread::sleep(std::time::Duration::from_millis(50))
                }
                other => return other,
            }
        }
        register(bundles, st)
    }

    #[test]
    fn verify_without_st_is_a_note_and_with_nothing_selected_asks_nobody() {
        // Symmetric with `register`: st is optional on a host (aegis-u1ybxo).
        let bobbin = BUNDLES
            .iter()
            .find(|(n, _)| *n == "bobbin")
            .copied()
            .unwrap();
        let lines = verify_with(&[bobbin], "caboodle-test-no-such-st").unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("NOT verified:"), "{lines:?}");
        assert_eq!(
            verify_with(&[], "caboodle-test-no-such-st").unwrap(),
            ["none selected by this plan"]
        );
    }

    #[test]
    fn a_hook_starting_with_a_missing_path_is_found_and_bare_names_are_not_checked() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let quipu = BUNDLES.iter().find(|(n, _)| *n == "quipu").unwrap().1;
        // Fresh host (the measured Mac): no ~/.gt/hooks at all.
        assert_eq!(
            missing_executables(quipu, Some(home.path())),
            ["$HOME/.gt/hooks/quipu-session-capture.sh"]
        );
        // Present but not executable still counts as missing.
        let dir = home.path().join(".gt/hooks");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("quipu-session-capture.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(missing_executables(quipu, Some(home.path())).len(), 1);
        // Executable: applicable (the Gas Town host, e.g. vati).
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(missing_executables(quipu, Some(home.path())).is_empty());
        // Bare program names (bobbin) and shell forms (yupana's `out=$(yupana`) are
        // the plan's own tools: never reported, even with no HOME at all.
        for name in ["bobbin", "yupana", "desire-path"] {
            let b = BUNDLES.iter().find(|(n, _)| *n == name).unwrap().1;
            assert!(missing_executables(b, None).is_empty(), "{name}");
        }
    }

    #[test]
    fn register_skips_a_bundle_whose_hook_path_does_not_exist_without_calling_st() {
        let dir = tempfile::tempdir().unwrap();
        let st = fake_st(dir.path(), "ghost: installed", 0);
        let ghost = r#"{"schema":"st.hook-bundle/1","name":"ghost","version":"1",
            "owner":"caboodle","roles":["*"],
            "hooks":[{"event":"Stop","command":"/nonexistent/ghost-hook.sh"}]}"#;
        let lines = reg(&[("ghost", ghost)], &st).unwrap();
        assert!(
            lines[0].starts_with("ghost: NOT registered on this host"),
            "{lines:?}"
        );
        assert!(lines[0].contains("/nonexistent/ghost-hook.sh"));
        assert!(
            !dir.path().join("got.json").exists(),
            "st was never asked to register it"
        );
    }

    #[test]
    fn selection_follows_the_plan_tools() {
        fn names(b: Vec<(&'static str, &'static str)>) -> Vec<&'static str> {
            b.into_iter().map(|(n, _)| n).collect()
        }
        assert_eq!(
            names(selected(["bobbin", "quipu", "camayoc"])),
            ["bobbin", "quipu"]
        );
        assert!(selected(Vec::<&str>::new()).is_empty());
        assert_eq!(
            names(selected(["quipu", "bobbin", "yupana", "desire-path"])).len(),
            BUNDLES.len()
        );
        assert_eq!(register(&[], "st").unwrap(), ["none selected by this plan"]);
    }

    #[test]
    fn register_passes_the_shipped_bundle_and_reports_st_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let bobbin = selected(["bobbin"]);
        for outcome in ["installed", "updated", "unchanged"] {
            let st = fake_st(dir.path(), &format!("bobbin: {outcome}"), 0);
            assert_eq!(reg(&bobbin, &st).unwrap(), [format!("bobbin: {outcome}")]);
        }
        // st received exactly the shipped bundle.
        let got = std::fs::read_to_string(dir.path().join("got.json")).unwrap();
        assert_eq!(got, bobbin[0].1);
    }

    #[test]
    fn st_absent_is_a_note_but_a_refusal_or_off_contract_answer_fails() {
        let dir = tempfile::tempdir().unwrap();
        let bobbin = selected(["bobbin"]);
        let missing = dir.path().join("no-such-st").display().to_string();
        let note = reg(&bobbin, &missing).unwrap();
        assert!(note[0].starts_with("NOT registered"), "{note:?}");
        let refused = fake_st(dir.path(), "", 1);
        let e = reg(&bobbin, &refused).unwrap_err().to_string();
        assert!(e.contains("refused") && e.contains("bad bundle"), "{e}");
        let odd = fake_st(dir.path(), "bobbin: maybe", 0);
        let e = reg(&bobbin, &odd).unwrap_err().to_string();
        assert!(e.contains("expected"), "{e}");
    }

    use super::*;
    use serde_json::json;

    const ONE: &[(&str, &str)] = &[(
        "demo",
        r#"{"schema":"st.hook-bundle/1","name":"demo","version":"1.0","hooks":[]}"#,
    )];

    fn report(items: Value) -> Value {
        json!({"schema": "st.hook-check/1", "items": items})
    }

    #[test]
    fn configured_bundle_verifies_and_reports_staleness() {
        let r = report(json!([
            {"bundle": "demo", "version": "1.0", "event": "Stop", "role": "worker", "configured": "ok", "live": "ok"},
            {"bundle": "demo", "version": "1.0", "event": "Stop", "role": "lead", "configured": "ok", "live": "stale"}
        ]));
        let lines = assess(ONE, &r).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("2 emitted item(s)"), "{lines:?}");
        assert!(lines[0].contains("1 item(s) not yet live"), "{lines:?}");
    }

    #[test]
    fn unregistered_duplicate_missing_and_version_drift_fail() {
        let absent = report(json!([]));
        assert!(assess(ONE, &absent)
            .unwrap_err()
            .to_string()
            .contains("not registered"));
        for configured in ["missing", "duplicate", "unsupported"] {
            let r = report(json!([
                {"bundle": "demo", "version": "1.0", "event": "Stop", "role": "worker", "configured": configured}
            ]));
            let err = assess(ONE, &r).unwrap_err().to_string();
            assert!(err.contains(configured), "{configured}: {err}");
        }
        let drift = report(json!([
            {"bundle": "demo", "version": "0.9", "event": "Stop", "role": "worker", "configured": "ok"}
        ]));
        assert!(assess(ONE, &drift)
            .unwrap_err()
            .to_string()
            .contains("differs"));
    }

    /// st exits 2 when it cannot read one running agent, even with every item
    /// configured ok. That is a liveness unknown and is reported, not failed;
    /// a broken registry is still refused.
    #[test]
    fn live_unknown_is_reported_and_registry_errors_fail() {
        let r = report(json!([
            {"bundle": "demo", "version": "1.0", "event": "Stop", "role": "worker", "configured": "ok", "live": "unknown"}
        ]));
        let lines = assess(ONE, &r).unwrap();
        assert!(lines[0].contains("not yet live"), "{lines:?}");
        let mut broken = r.clone();
        broken["registry_errors"] = json!(["demo.json: bad schema"]);
        assert!(assess(ONE, &broken)
            .unwrap_err()
            .to_string()
            .contains("registry"));
    }

    #[test]
    fn unrecognised_report_schema_is_refused() {
        let r = json!({"schema": "other", "items": []});
        assert!(assess(ONE, &r).is_err());
    }

    #[test]
    fn shipped_bundles_parse_and_name_themselves() {
        for (name, json) in BUNDLES {
            let v: Value = serde_json::from_str(json).unwrap();
            assert_eq!(v["schema"], "st.hook-bundle/1", "{name}");
            assert_eq!(v["name"], *name);
            assert!(
                v["hooks"].as_array().is_some_and(|h| !h.is_empty()),
                "{name}"
            );
        }
    }

    /// desire-path's codex source is codex's per-turn `notify`, not per-tool
    /// hooks (aegis-331f7p). The argv is the one `dp init --source codex`
    /// writes for dp 0.3.2 (measured in a sandbox CODEX_HOME); a hand edit
    /// that drifts from it would leave codex invisible to desire-path.
    #[test]
    fn desire_path_bundle_claims_codex_notify_with_dp_installer_argv() {
        let (_, json) = BUNDLES
            .iter()
            .find(|(name, _)| *name == "desire-path")
            .unwrap();
        let v: Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            v["codex_notify"],
            json!([
                "bash",
                "-c",
                "printf '%s' \"$1\" | dp ingest --source codex",
                "--"
            ])
        );
        // codex notify is a single slot: exactly one shipped bundle claims it.
        let claimants = BUNDLES
            .iter()
            .filter(|(_, j)| serde_json::from_str::<Value>(j).unwrap()["codex_notify"].is_array())
            .count();
        assert_eq!(claimants, 1);
    }

    #[test]
    fn codex_signposts_cover_both_sides_of_a_shell_search() {
        let bundle: Value = serde_json::from_str(
            BUNDLES
                .iter()
                .find(|(name, _)| *name == "desire-path")
                .unwrap()
                .1,
        )
        .unwrap();
        let hooks: Vec<_> = bundle["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["harnesses"].as_array().unwrap().contains(&json!("codex")))
            .collect();
        for (event, command) in [
            ("PreToolUse", "dp signpost-prefetch"),
            ("PostToolUse", "dp signpost"),
        ] {
            assert_eq!(
                hooks
                    .iter()
                    .filter(|h| h["event"] == event
                        && h["matcher"] == "Bash"
                        && h["command"] == command)
                    .count(),
                1
            );
        }
        // Notify retains Codex attribution. Do not feed Codex hook events to
        // the Claude ingest source or claim unsupported failure-hook coverage.
        assert!(hooks.iter().all(|h| h["event"] != "PostToolUseFailure"
            && h["command"] != "dp ingest --source claude-code"));
    }

    /// Against the host's real st (`cargo test -- --ignored live_register`):
    /// registers every shipped bundle, exactly the step `caboodle apply` runs.
    /// It WRITES the host's st registry; a second run must print `unchanged`
    /// for every bundle (idempotence on the real st).
    #[test]
    #[ignore = "writes the host's st hook-bundle registry"]
    fn live_register_against_host_st() {
        for line in register(BUNDLES, "st").expect("register against host st") {
            println!("REGISTER {line}");
        }
    }

    /// Against the host's real st (`cargo test -- --ignored live_`): prints the
    /// verdict so an operator can see what `caboodle verify` would say.
    #[test]
    fn evidence_is_well_formed_and_fails_closed_without_its_source() {
        let mut seen = 0;
        for (name, json) in BUNDLES {
            let v: Value = serde_json::from_str(json).unwrap();
            for hook in v["hooks"].as_array().unwrap() {
                let Some(ev) = hook.get("evidence") else {
                    continue;
                };
                seen += 1;
                let cmd = ev["command"].as_str().expect("evidence.command");
                assert!(!cmd.trim().is_empty(), "{name}");
                assert!(
                    ev["max_age_seconds"].as_u64().is_some_and(|n| n > 0),
                    "{name}"
                );
                // Fail closed: with HOME pointing at an empty dir the source is
                // absent, so the command must exit non-zero, never print a ts.
                let empty = tempfile::tempdir().unwrap();
                let out = std::process::Command::new("sh")
                    .args(["-c", cmd])
                    .env("HOME", empty.path())
                    .output()
                    .unwrap();
                assert!(
                    !out.status.success(),
                    "{name}: evidence succeeded without its source"
                );
                assert!(out.stdout.is_empty(), "{name}: printed without its source");
            }
        }
        assert!(
            seen >= 6,
            "expected evidence on at least 6 hooks, saw {seen}"
        );
    }

    #[test]
    #[ignore = "needs a live st with a registry"]
    fn live_verify_against_host_st() {
        match verify(BUNDLES) {
            Ok(lines) => lines.iter().for_each(|l| println!("OK {l}")),
            Err(e) => println!("FAIL {e}"),
        }
    }
}
