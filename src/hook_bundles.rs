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

/// Run st's check and assess the shipped bundles against it.
pub fn verify() -> Result<Vec<String>> {
    // Not `checked`: st exits 1 for drift (including live staleness) and 2 when
    // it cannot tell. Exit 2 is also what st returns when it cannot read what
    // ONE running agent carries, with every item configured ok (aegis-331f7p,
    // measured 2026-09-30). The JSON is the evidence either way; `assess`
    // decides, so an item st could not judge still fails on `configured`.
    let output = Command::new("st")
        .args(["ops", "hooks", "check", "--json"])
        .output()
        .context("run st ops hooks check --json")?;
    let report: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "st ops hooks check --json gave no readable report (exit {:?})",
            output.status.code()
        )
    })?;
    assess(BUNDLES, &report)
}

#[cfg(test)]
mod tests {
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

    /// Against the host's real st (`cargo test -- --ignored live_`): prints the
    /// verdict so an operator can see what `caboodle verify` would say.
    #[test]
    #[ignore = "needs a live st with a registry"]
    fn live_verify_against_host_st() {
        match verify() {
            Ok(lines) => lines.iter().for_each(|l| println!("OK {l}")),
            Err(e) => println!("FAIL {e}"),
        }
    }
}
