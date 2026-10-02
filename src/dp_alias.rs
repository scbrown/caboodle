//! The `bd` -> `sd` command alias in Desire Path, for plans that install both
//! seeds and Desire Path (aegis-fbaso4).
//!
//! seeds is the beads-compatible tracker (`sd`). An agent that types `bd` from
//! habit is redirected to `sd` by a Desire Path command-substitution rule.
//!
//! NEVER OVERWRITE. `dp alias --cmd bd --replace sd` silently REPLACES an
//! existing `bd` rule (measured on a scratch db: `bd -> br` became `bd -> sd`
//! with exit 0 and one row left). A host that already routes `bd` somewhere
//! else made that choice deliberately — the aegis fleet routes `bd -> br`, its
//! live tracker, and rewriting it to `sd` would send every bead command to a
//! store nobody reads. So the existing rules are READ first, and a different
//! `bd` target is reported and left alone.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub const FROM: &str = "bd";
pub const TO: &str = "sd";

/// The alias belongs to plans that select BOTH seeds and Desire Path.
pub fn wanted<'a>(tools: impl IntoIterator<Item = &'a str>) -> bool {
    let tools: Vec<&str> = tools.into_iter().collect();
    tools.contains(&"seeds") && tools.contains(&"desire-path")
}

/// What `dp aliases --json` already says about the `bd` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Existing {
    Absent,
    Ours,
    /// A `bd` command rule with a different target: a deliberate local choice.
    Other(String),
}

/// Read the `bd` command rule out of `dp aliases --json` output. Only a
/// `match_kind: "command"` rule for command `bd` counts: regex and literal
/// rules that merely mention bd are different kinds and are not ours to judge.
pub fn existing(aliases_json: &[u8]) -> Result<Existing> {
    let text = String::from_utf8_lossy(aliases_json);
    // dp prints `null`, not `[]`, when no rule exists; both mean none.
    let rules: Value = serde_json::from_str(text.trim())
        .with_context(|| format!("parse `dp aliases --json` output: {}", text.trim()))?;
    let rules = match rules {
        Value::Null => return Ok(Existing::Absent),
        Value::Array(rules) => rules,
        other => bail!("`dp aliases --json` returned {other}, expected a list of rules"),
    };
    for rule in &rules {
        let is_bd_command = rule.get("match_kind").and_then(Value::as_str) == Some("command")
            && rule.get("command").and_then(Value::as_str) == Some(FROM);
        if !is_bd_command {
            continue;
        }
        return Ok(match rule.get("to").and_then(Value::as_str) {
            Some(TO) => Existing::Ours,
            Some(other) => Existing::Other(other.to_owned()),
            None => Existing::Other("<no target>".to_owned()),
        });
    }
    Ok(Existing::Absent)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Set,
    AlreadySet,
    KeptLocal(String),
    NoDp,
}

impl Outcome {
    pub fn line(&self) -> String {
        match self {
            Self::Set => format!("{FROM} -> {TO}: set"),
            Self::AlreadySet => format!("{FROM} -> {TO}: present"),
            Self::KeptLocal(target) => format!(
                "{FROM} -> {TO}: NOT set; this host already routes {FROM} -> {target}, \
                 a deliberate local choice that caboodle never overwrites \
                 (remove it with `dp alias --delete --cmd {FROM} --replace {target}` to adopt {TO})"
            ),
            Self::NoDp => format!(
                "{FROM} -> {TO}: note: `dp` is not on PATH, so the alias was not set; \
                 rerun apply once Desire Path is on PATH"
            ),
        }
    }
}

/// The `dp` that PATH runs, if any.
pub fn locate() -> Option<PathBuf> {
    let path = std::env::var_os("PATH");
    crate::adapter::which_all("dp", path.as_deref())
        .into_iter()
        .next()
}

fn run(dp: &Path, db: Option<&Path>, args: &[&str]) -> Result<std::process::Output> {
    let mut argv: Vec<OsString> = Vec::new();
    if let Some(db) = db {
        argv.push("--db".into());
        argv.push(db.as_os_str().to_owned());
    }
    argv.extend(args.iter().map(OsString::from));
    crate::adapter::checked(dp, argv, None)
}

fn read(dp: &Path, db: Option<&Path>) -> Result<Existing> {
    let out = run(dp, db, &["aliases", "--json"]).context("read Desire Path aliases")?;
    existing(&out.stdout)
}

/// Set `bd -> sd` unless a `bd` rule already exists. `db` overrides dp's
/// default database (tests); production passes `None`.
pub fn apply(dp: Option<&Path>, db: Option<&Path>) -> Result<Outcome> {
    let Some(dp) = dp else {
        return Ok(Outcome::NoDp);
    };
    match read(dp, db)? {
        Existing::Ours => return Ok(Outcome::AlreadySet),
        Existing::Other(target) => return Ok(Outcome::KeptLocal(target)),
        Existing::Absent => {}
    }
    run(
        dp,
        db,
        &[
            "alias",
            "--cmd",
            FROM,
            "--replace",
            TO,
            "--message",
            "seeds (sd) is this stack's beads-compatible tracker",
        ],
    )
    .context("set the Desire Path bd -> sd alias")?;
    // Verify by effect: the rule must now read back as ours.
    match read(dp, db)? {
        Existing::Ours => Ok(Outcome::Set),
        other => bail!("set `dp alias --cmd {FROM} --replace {TO}` but dp now reports {other:?}"),
    }
}

/// Assert the alias: ours passes, a different local target passes with a note,
/// and absence fails (apply would have set it).
pub fn verify(dp: Option<&Path>, db: Option<&Path>) -> Result<Outcome> {
    let Some(dp) = dp else {
        bail!("`dp` is not on PATH; cannot read the {FROM} -> {TO} alias");
    };
    match read(dp, db)? {
        Existing::Ours => Ok(Outcome::AlreadySet),
        Existing::Other(target) => Ok(Outcome::KeptLocal(target)),
        Existing::Absent => bail!(
            "Desire Path has no {FROM} command alias; `caboodle apply` sets {FROM} -> {TO} for plans with seeds and desire-path"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The shape measured on a live host (aegis-fbaso4 baseline), including a
    // regex rule that mentions bd and must NOT be mistaken for the command rule.
    const LIVE: &str = r#"[
      {"from":"(^|[[:space:]|;&])b[dr] comment ","to":"${1}br comments add ","tool":"Bash","param":"command","match_kind":"regex"},
      {"from":"bd","to":"br","tool":"Bash","param":"command","command":"bd","match_kind":"command"},
      {"from":"python","to":"python3","tool":"Bash","param":"command","command":"python","match_kind":"command"}
    ]"#;

    #[test]
    fn wanted_needs_both_members() {
        assert!(wanted(["quipu", "seeds", "desire-path"]));
        assert!(!wanted(["seeds"]));
        assert!(!wanted(["desire-path", "bobbin"]));
    }

    #[test]
    fn existing_reads_the_command_rule_only() {
        assert_eq!(
            existing(LIVE.as_bytes()).unwrap(),
            Existing::Other("br".into())
        );
        let ours = r#"[{"from":"bd","to":"sd","command":"bd","match_kind":"command"}]"#;
        assert_eq!(existing(ours.as_bytes()).unwrap(), Existing::Ours);
        // A regex rule mentioning bd alone is not a bd command alias.
        let regex_only = r#"[{"from":"bd x","to":"sd x","match_kind":"regex"}]"#;
        assert_eq!(existing(regex_only.as_bytes()).unwrap(), Existing::Absent);
        assert_eq!(existing(b"null\n").unwrap(), Existing::Absent);
        assert_eq!(existing(b"[]").unwrap(), Existing::Absent);
        assert!(existing(b"not json").is_err());
    }

    #[test]
    fn absent_dp_is_a_note_on_apply_and_a_failure_on_verify() {
        assert_eq!(apply(None, None).unwrap(), Outcome::NoDp);
        assert!(verify(None, None).is_err());
    }

    /// A stand-in `dp` that keeps one `bd` rule in a file and, like the real
    /// dp (measured), OVERWRITES it on every `alias --cmd bd --replace X`.
    /// The never-overwrite arm is therefore only green because apply reads
    /// first: delete that read and `keeps_a_different_local_target` fails.
    #[cfg(unix)]
    fn fake_dp(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("dp");
        std::fs::write(
            &script,
            r#"#!/bin/sh
store="$(dirname "$0")/bd.rule"
if [ "$1" = "--db" ]; then shift 2; fi
case "$1" in
  aliases)
    if [ -s "$store" ]; then
      printf '[{"from":"bd","to":"%s","command":"bd","match_kind":"command"}]\n' "$(cat "$store")"
    else
      echo null
    fi ;;
  alias)
    shift
    while [ $# -gt 0 ]; do
      case "$1" in --replace) printf '%s' "$2" > "$store"; shift 2 ;; *) shift ;; esac
    done ;;
  *) exit 2 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[cfg(unix)]
    #[test]
    fn sets_on_a_clean_db_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let dp = fake_dp(dir.path());
        assert!(
            verify(Some(&dp), None).is_err(),
            "control: clean db has no alias"
        );
        assert_eq!(apply(Some(&dp), None).unwrap(), Outcome::Set);
        assert_eq!(apply(Some(&dp), None).unwrap(), Outcome::AlreadySet);
        assert_eq!(verify(Some(&dp), None).unwrap(), Outcome::AlreadySet);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("bd.rule")).unwrap(),
            "sd"
        );
    }

    #[cfg(unix)]
    #[test]
    fn keeps_a_different_local_target() {
        let dir = tempfile::tempdir().unwrap();
        let dp = fake_dp(dir.path());
        std::fs::write(dir.path().join("bd.rule"), "br").unwrap();
        assert_eq!(
            apply(Some(&dp), None).unwrap(),
            Outcome::KeptLocal("br".into())
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("bd.rule")).unwrap(),
            "br"
        );
        assert_eq!(
            verify(Some(&dp), None).unwrap(),
            Outcome::KeptLocal("br".into())
        );
        assert!(Outcome::KeptLocal("br".into())
            .line()
            .contains("never overwrites"));
    }

    /// Both arms against the REAL dp on a scratch database. Ignored by default
    /// because CI has no dp; run with `cargo test -- --ignored real_dp`.
    #[test]
    #[ignore]
    fn real_dp_both_arms() {
        let dp = locate().expect("real_dp_both_arms needs dp on PATH");
        let clean = tempfile::tempdir().unwrap();
        let db = clean.path().join("d.db");
        assert_eq!(apply(Some(&dp), Some(&db)).unwrap(), Outcome::Set);
        assert_eq!(verify(Some(&dp), Some(&db)).unwrap(), Outcome::AlreadySet);

        let local = tempfile::tempdir().unwrap();
        let db = local.path().join("d.db");
        run(&dp, Some(&db), &["alias", "--cmd", "bd", "--replace", "br"]).unwrap();
        assert_eq!(
            apply(Some(&dp), Some(&db)).unwrap(),
            Outcome::KeptLocal("br".into())
        );
        assert_eq!(read(&dp, Some(&db)).unwrap(), Existing::Other("br".into()));
    }
}
