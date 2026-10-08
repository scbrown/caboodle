//! Each tool's OWN hook bundle, read from the installed tool (aegis-5s32or.5).
//!
//! The stack tools now ship their hook definitions themselves:
//! `<tool> hooks bundle` prints the tool's `st.hook-bundle/1`, versioned with
//! the tool. CABOODLE registers that bundle, so the hooks a host runs are the
//! ones its installed tool version declares. An older tool without the command
//! (or one that prints something that is not its own bundle) falls back to the
//! copy CABOODLE ships in `hook-bundles/`. The source is reported either way.

use serde_json::Value;

/// Where a resolved bundle came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `<tool> hooks bundle` on this host.
    Tool,
    /// The copy shipped in CABOODLE's `hook-bundles/`.
    Shipped,
}

impl Source {
    /// Report label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Shipped => "shipped copy",
        }
    }
}

/// The executable that prints a bundle's hooks.
#[must_use]
pub fn binary(bundle: &str) -> &str {
    match bundle {
        "desire-path" => "dp",
        other => other,
    }
}

/// `<bin> hooks bundle` stdout, or `None` when the command is absent or fails.
#[must_use]
pub fn run_hooks_bundle(bin: &str) -> Option<String> {
    let out = std::process::Command::new(bin)
        .args(["hooks", "bundle"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Accept `text` only if it is a `st.hook-bundle/1` named `name`.
fn own_bundle(name: &str, text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    (v.get("schema").and_then(Value::as_str) == Some("st.hook-bundle/1")
        && v.get("name").and_then(Value::as_str) == Some(name)
        && v.get("hooks").is_some_and(Value::is_array))
    .then(|| text.to_string())
}

/// Resolve the selected bundles: the tool's own when it answers with its own
/// bundle, else the shipped copy. `run` is injectable for tests.
pub fn resolve_with(
    shipped: &[(&'static str, &'static str)],
    run: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String, Source)> {
    shipped
        .iter()
        .map(
            |(name, copy)| match run(binary(name)).and_then(|text| own_bundle(name, &text)) {
                Some(json) => ((*name).to_string(), json, Source::Tool),
                None => ((*name).to_string(), (*copy).to_string(), Source::Shipped),
            },
        )
        .collect()
}

/// [`resolve_with`] against the installed tools.
#[must_use]
pub fn resolve(shipped: &[(&'static str, &'static str)]) -> Vec<(String, String, Source)> {
    resolve_with(shipped, run_hooks_bundle)
}

/// Borrowed `(name, json)` pairs for the register/verify functions.
#[must_use]
pub fn as_pairs(resolved: &[(String, String, Source)]) -> Vec<(&str, &str)> {
    resolved
        .iter()
        .map(|(n, j, _)| (n.as_str(), j.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &[(&str, &str)] = &[
        (
            "bobbin",
            r#"{"schema":"st.hook-bundle/1","name":"bobbin","version":"old","hooks":[]}"#,
        ),
        (
            "desire-path",
            r#"{"schema":"st.hook-bundle/1","name":"desire-path","version":"old","hooks":[]}"#,
        ),
    ];

    #[test]
    fn the_tools_own_bundle_wins_and_dp_is_asked_as_dp() {
        let asked = std::cell::RefCell::new(Vec::new());
        let got = resolve_with(SHIPPED, |bin| {
            asked.borrow_mut().push(bin.to_string());
            let name = if bin == "dp" { "desire-path" } else { bin };
            Some(format!(
                r#"{{"schema":"st.hook-bundle/1","name":"{name}","version":"new","hooks":[]}}"#
            ))
        });
        assert_eq!(*asked.borrow(), ["bobbin", "dp"]);
        assert!(got
            .iter()
            .all(|(_, j, s)| *s == Source::Tool && j.contains("\"new\"")));
    }

    #[test]
    fn an_older_tool_or_a_wrong_answer_falls_back_to_the_shipped_copy() {
        // bobbin: no `hooks bundle` command; dp: answers with someone else's bundle.
        let got = resolve_with(SHIPPED, |bin| {
            (bin == "dp")
                .then(|| r#"{"schema":"st.hook-bundle/1","name":"bobbin","hooks":[]}"#.to_string())
        });
        assert!(got
            .iter()
            .all(|(_, j, s)| *s == Source::Shipped && j.contains("\"old\"")));
        assert_eq!(as_pairs(&got)[1].0, "desire-path");
    }

    #[test]
    fn non_json_is_not_a_bundle() {
        let got = resolve_with(&SHIPPED[..1], |_| {
            Some("bobbin hooks: unknown command".into())
        });
        assert_eq!(got[0].2, Source::Shipped);
    }
}
