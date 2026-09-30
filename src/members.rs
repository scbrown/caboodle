//! Stack members declared as DATA (aegis-1i5h1j C1).
//!
//! A member is one reviewed `members/<name>.toml`, embedded at build time by
//! `build.rs`, so the version and per-target digests a user installs are the
//! ones this caboodle build was reviewed with. Adding a member is a data file,
//! never a Rust match arm: every site that dispatches on `ToolName` has one
//! explicit `Member` arm that reads from here.
//!
//! Every manifest is parsed and validated by a unit test, so a malformed or
//! unsafe manifest fails caboodle's CI and never reaches a user.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::model::{Profile, ToolName};

/// How a member is delivered. A new kind is a code change; a new member is data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A `.tar.gz` GitHub release asset per target, holding the programs.
    RustRelease,
}

/// One program the member puts on PATH, and how to tell it from a program of
/// the same name that is NOT this member (seeds' `sd` vs chmln/sd, A2).
///
/// A version line cannot do that: both print `sd <semver>`. So the identity is
/// text only this member prints (seeds: a line of its `--help`), and a bare
/// name or name-plus-version identity is refused (wu F1).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub name: String,
    /// Arguments that print the identity text, e.g. `["--help"]`.
    pub identity_argv: Vec<String>,
    /// The identity output (stdout, then stderr) must CONTAIN this literal text.
    pub identity_contains: String,
}

/// One verify step. It must exit 0; `absent` / `present` then test its stdout
/// for a substring. `{marker}` in any field is a fresh per-run marker.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub argv: Vec<String>,
    #[serde(default)]
    pub absent: Option<String>,
    #[serde(default)]
    pub present: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub kind: Kind,
    /// GitHub `owner/repo` the release is published under.
    pub repo: String,
    pub version: String,
    /// Release tag, with `{version}` substituted (A1: not derivable in general).
    pub tag: String,
    /// Asset file name, with `{tag}`, `{version}` and `{target}` substituted.
    pub asset: String,
    /// Recorded lowercase-hex SHA-256 of the asset, per target triple. Install
    /// verifies against THIS, never against a checksum file fetched at install.
    pub sha256: BTreeMap<String, String>,
    /// The release asset listing every asset's SHA-256, e.g. `SHA256SUMS.txt`.
    /// Read only by `bump-member`, which records its digests here for review;
    /// install never trusts it (wu M2).
    pub sums_asset: String,
    pub programs: Vec<Program>,
    /// Arguments printing the installed version.
    pub version_argv: Vec<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// Profiles this member joins (see `Profile::tools`).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Verify steps, run in a hermetic temp HOME (A3).
    pub verify: Vec<Step>,
}

impl Manifest {
    /// The concrete asset name for `target`.
    pub fn asset_for(&self, target: &str) -> String {
        let tag = self.tag.replace("{version}", &self.version);
        self.asset
            .replace("{tag}", &tag)
            .replace("{version}", &self.version)
            .replace("{target}", target)
    }

    pub fn tag(&self) -> String {
        self.tag.replace("{version}", &self.version)
    }

    /// Parse and validate one manifest's text.
    pub fn parse(text: &str) -> Result<Self> {
        let m: Manifest = toml::from_str(text).context("invalid member manifest")?;
        m.validate()?;
        Ok(m)
    }

    fn validate(&self) -> Result<()> {
        let safe = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !safe(&self.name) || ToolName::BUILTINS.iter().any(|t| t.as_str() == self.name) {
            bail!(
                "member name '{}' is unsafe or shadows a built-in tool",
                self.name
            );
        }
        // Both are substituted into the download URL (malcolm N4).
        if !safe(&self.version) || !safe(&self.tag.replace("{version}", &self.version)) {
            bail!("{}: version and tag must be safe names", self.name);
        }
        // bump-member recovers the version from a release tag through this.
        if self.tag.matches("{version}").count() != 1 {
            bail!("{}: tag must contain {{version}} exactly once", self.name);
        }
        if !safe(&self.sums_asset) {
            bail!("{}: sums_asset must be a safe name", self.name);
        }
        if self.repo.split('/').count() != 2 || !self.repo.split('/').all(safe) {
            bail!("{}: repo must be owner/name", self.name);
        }
        if self.sha256.is_empty() {
            bail!("{}: at least one target digest is required", self.name);
        }
        for (target, digest) in &self.sha256 {
            if !safe(target)
                || digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                bail!(
                    "{}: sha256 for {target} must be 64 lowercase hex",
                    self.name
                );
            }
        }
        if self.programs.is_empty() || !self.programs.iter().all(|p| safe(&p.name)) {
            bail!("{}: programs must be non-empty safe names", self.name);
        }
        for p in &self.programs {
            // After the program's own name, the identity must still say
            // something a same-named program would not: a word, not a version.
            let rest = p.identity_contains.replace(&p.name, "");
            let is_version = |w: &str| {
                w.trim_start_matches('v')
                    .starts_with(|c: char| c.is_ascii_digit())
            };
            if !rest
                .split_whitespace()
                .filter(|w| !is_version(w))
                .any(|w| w.chars().any(char::is_alphabetic))
            {
                bail!(
                    "{}: identity_contains {:?} for `{}` is only its name and a version, which a \
                     different `{}` also prints (F1); use text only this member prints",
                    self.name,
                    p.identity_contains,
                    p.name,
                    p.name
                );
            }
        }
        // A4: a verify that only ever sees the marker PRESENT would pass for a
        // program that prints everything. Require an absent-then-present pair.
        let absent_at = self.verify.iter().position(|s| s.absent.is_some());
        let present_at = self.verify.iter().rposition(|s| s.present.is_some());
        match (absent_at, present_at) {
            (Some(a), Some(p)) if a < p => {}
            _ => bail!(
                "{}: verify needs a step asserting a marker ABSENT before one asserting it PRESENT (A4)",
                self.name
            ),
        }
        if self.verify.iter().any(|s| s.argv.is_empty()) {
            bail!("{}: every verify step needs argv", self.name);
        }
        // B3: a step handed the marker can pass `present` by echoing its
        // arguments, storing nothing. The read-back must be a separate step.
        if self
            .verify
            .iter()
            .any(|s| s.present.is_some() && s.argv.iter().any(|a| a.contains("{marker}")))
        {
            bail!(
                "{}: a verify step asserting the marker PRESENT must not be given it in argv (B3)",
                self.name
            );
        }
        Ok(())
    }
}

const EMBEDDED: &[&str] = include!(concat!(env!("OUT_DIR"), "/members_embedded.rs"));

fn parse_all() -> Result<Vec<Manifest>> {
    let mut out: Vec<Manifest> = Vec::new();
    for text in EMBEDDED {
        let m = Manifest::parse(text)?;
        if out.iter().any(|o| o.name == m.name) {
            bail!("duplicate member '{}'", m.name);
        }
        out.push(m);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Every embedded member, validated, in name order. A failure here is a build
/// defect (the unit test below catches it in CI), so it panics loudly.
pub fn all() -> &'static [Manifest] {
    static REGISTRY: OnceLock<Vec<Manifest>> = OnceLock::new();
    REGISTRY.get_or_init(|| parse_all().expect("embedded member manifests are invalid"))
}

/// The member named `name`, if one is embedded.
pub fn get(name: &str) -> Option<&'static Manifest> {
    all().iter().find(|m| m.name == name)
}

/// Program names of `name`, for callers that need a `'static` slice.
pub fn programs(name: &str) -> &'static [&'static str] {
    static PROGRAMS: OnceLock<BTreeMap<String, Vec<&'static str>>> = OnceLock::new();
    PROGRAMS
        .get_or_init(|| {
            all()
                .iter()
                .map(|m| {
                    (
                        m.name.clone(),
                        m.programs.iter().map(|p| p.name.as_str()).collect(),
                    )
                })
                .collect()
        })
        .get(name)
        .map_or(&[], Vec::as_slice)
}

/// Prerequisites of `name` as a `'static` slice.
pub fn prerequisites(name: &str) -> &'static [&'static str] {
    static PREREQS: OnceLock<BTreeMap<String, Vec<&'static str>>> = OnceLock::new();
    PREREQS
        .get_or_init(|| {
            all()
                .iter()
                .map(|m| {
                    // What ManifestAdapter::install itself shells out to (the digest
                    // is computed in-process), then the member's own.
                    let mut all: Vec<&'static str> = vec!["curl", "tar"];
                    all.extend(m.prerequisites.iter().map(String::as_str));
                    (m.name.clone(), all)
                })
                .collect()
        })
        .get(name)
        .map_or(&[], Vec::as_slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_member_parses_and_validates() {
        parse_all().expect("embedded manifests");
    }

    fn manifest(verify: &str) -> Result<Manifest> {
        let text = format!(
            r#"name = "demo"
kind = "rust-release"
repo = "owner/demo"
version = "1.2.3"
tag = "demo-v{{version}}"
asset = "demo-{{tag}}-{{target}}.tar.gz"
version_argv = ["--version"]
sums_asset = "SHA256SUMS.txt"
[sha256]
x86_64-unknown-linux-gnu = "{}"
[[programs]]
name = "demo"
identity_argv = ["--help"]
identity_contains = "demo is the demo member"
{verify}"#,
            "a".repeat(64)
        );
        let m: Manifest = toml::from_str(&text)?;
        m.validate()?;
        Ok(m)
    }

    const GOOD: &str = r#"[[verify]]
argv = ["demo", "list"]
absent = "{marker}"
[[verify]]
argv = ["demo", "add", "{marker}"]
[[verify]]
argv = ["demo", "list"]
present = "{marker}"
"#;

    #[test]
    fn a_well_formed_manifest_validates_and_templates_its_asset() {
        let m = manifest(GOOD).unwrap();
        assert_eq!(m.tag(), "demo-v1.2.3");
        assert_eq!(
            m.asset_for("x86_64-unknown-linux-gnu"),
            "demo-demo-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn verify_without_an_absent_then_present_pair_is_refused() {
        let present_only = r#"[[verify]]
argv = ["demo", "list"]
present = "{marker}"
"#;
        assert!(manifest(present_only).is_err());
        let reversed = r#"[[verify]]
argv = ["demo", "list"]
present = "{marker}"
[[verify]]
argv = ["demo", "list"]
absent = "{marker}"
"#;
        assert!(manifest(reversed).is_err());
    }

    #[test]
    fn present_on_the_step_that_is_handed_the_marker_is_refused() {
        // B3: an argument-echoing program would pass this without storing anything.
        let echo_pass = r#"[[verify]]
argv = ["demo", "list"]
absent = "{marker}"
[[verify]]
argv = ["demo", "add", "{marker}"]
present = "{marker}"
"#;
        let e = manifest(echo_pass).unwrap_err().to_string();
        assert!(e.contains("B3"), "{e}");
        // Control: the same steps with a separate read-back validate.
        manifest(GOOD).unwrap();
    }

    #[test]
    fn an_identity_that_is_only_name_and_version_is_refused() {
        // F1: seeds `sd 0.0.2` and chmln/sd `sd 1.0.0` share this shape.
        let mut m = manifest(GOOD).unwrap();
        for weak in ["demo", "demo ", "demo 1.2.3", "v1.2.3"] {
            m.programs[0].identity_contains = weak.into();
            assert!(m.validate().is_err(), "{weak:?} must be refused");
        }
        m.programs[0].identity_contains = "demo is the demo member".into();
        m.validate().unwrap();
    }

    #[test]
    fn an_unsafe_version_or_tag_is_refused() {
        let mut m = manifest(GOOD).unwrap();
        m.version = "1.2.3/../x".into();
        assert!(m.validate().is_err());
        let mut m = manifest(GOOD).unwrap();
        m.tag = "v{version}?x=1".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn a_bad_digest_an_unsafe_name_or_a_shadowing_name_is_refused() {
        let bad_digest = GOOD.to_owned();
        let m = manifest(&bad_digest).unwrap();
        let mut bad = m.clone();
        bad.sha256.insert("x".into(), "ABC".into());
        assert!(bad.validate().is_err());
        for builtin in ToolName::BUILTINS {
            let mut shadow = m.clone();
            shadow.name = builtin.as_str().into();
            assert!(shadow.validate().is_err(), "{}", builtin.as_str());
        }
        let mut unsafe_name = m;
        unsafe_name.name = "../x".into();
        assert!(unsafe_name.validate().is_err());
    }
}

#[cfg(all(test, feature = "fixture-members"))]
mod profile_tests {
    use crate::model::{Profile, ToolName};

    fn fixture() -> ToolName {
        ToolName::parse("fixture-demo").expect("fixture member is embedded")
    }

    #[test]
    fn a_new_plan_for_the_profile_includes_its_members() {
        assert!(Profile::Everything.tools().contains(&fixture()));
        assert!(!Profile::Kg.tools().contains(&fixture()));
    }

    #[test]
    fn a_selection_saved_before_the_member_existed_is_still_accepted() {
        // wu M1: resume after an upgrade that added a member.
        assert!(Profile::Everything.accepts(&Profile::Everything.builtin_tools()));
        assert!(Profile::Everything.accepts(&Profile::Everything.tools()));
    }

    #[test]
    fn an_undeclared_duplicate_or_misplaced_member_is_refused() {
        let mut kg = Profile::Kg.builtin_tools();
        kg.push(fixture());
        assert!(!Profile::Kg.accepts(&kg), "the fixture does not declare kg");
        let mut dup = Profile::Everything.tools();
        dup.push(fixture());
        assert!(!Profile::Everything.accepts(&dup));
        let mut misplaced = vec![fixture()];
        misplaced.extend(Profile::Everything.builtin_tools());
        assert!(!Profile::Everything.accepts(&misplaced));
    }

    #[test]
    fn a_member_round_trips_through_serde_and_an_unknown_name_is_refused() {
        let json = serde_json::to_string(&fixture()).unwrap();
        assert_eq!(json, "\"fixture-demo\"");
        assert_eq!(serde_json::from_str::<ToolName>(&json).unwrap(), fixture());
        assert!(serde_json::from_str::<ToolName>("\"not-a-member\"").is_err());
        assert_eq!(
            serde_json::from_str::<ToolName>("\"desire-path\"").unwrap(),
            ToolName::DesirePath
        );
    }
}
