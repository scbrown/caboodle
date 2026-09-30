//! Bump a member's reviewed pin from its published release (wu M2, aegis-1i5h1j).
//!
//! A member is pinned to one version and its per-target digests, so a new
//! member release reaches users only through a caboodle change. This writes
//! that change: it reads the release, downloads the published sums file AND
//! every reviewed target's asset, requires each asset's bytes to hash to its
//! published line, and rewrites only `version` and the `[sha256]` values of the
//! manifest (comments and layout kept) for a human to review and merge.
//! Before recording anything it unpacks the host target's asset and requires
//! the programs to answer their identity and the new version (malcolm S1). It
//! never installs anything and never runs at a user's install time.

use std::{fs, path::Path};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::adapter::download_https;
use crate::members::Manifest;
use crate::release_update::{checksum, hash, stable_version, text};

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
}

/// What a bump did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Already pinned to the published release; nothing written.
    Current { version: String },
    /// The manifest now pins `to`; review and merge it.
    Bumped {
        from: String,
        to: String,
        digests: Vec<(String, String)>,
    },
}

/// The version a release `tag` carries under the manifest's tag template.
fn version_of(manifest: &Manifest, tag: &str) -> Result<String> {
    let (before, after) = manifest
        .tag
        .split_once("{version}")
        .context("tag template has no {version}")?;
    let version = tag
        .strip_prefix(before)
        .and_then(|rest| rest.strip_suffix(after))
        .with_context(|| {
            format!(
                "release tag {tag:?} does not match the manifest's tag template {:?}",
                manifest.tag
            )
        })?;
    stable_version(version)?;
    Ok(version.to_owned())
}

fn release(manifest: &Manifest, tag: Option<&str>) -> Result<Release> {
    let path = match tag {
        Some(tag) => {
            if tag.is_empty()
                || !tag
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                bail!("unsafe release tag {tag:?}");
            }
            format!("tags/{tag}")
        }
        None => "latest".to_owned(),
    };
    let raw = text(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            "--proto",
            "=https",
            "-H",
            "Accept: application/vnd.github+json",
            &format!(
                "https://api.github.com/repos/{}/releases/{path}",
                manifest.repo
            ),
        ],
    )?;
    let release: Release = serde_json::from_str(&raw).context("parse published release")?;
    if release.draft || release.prerelease {
        bail!("refusing draft/prerelease {}", release.tag_name);
    }
    Ok(release)
}

/// Bump the manifest at `path` to the latest published release, or to `tag`.
pub fn bump(path: &Path, tag: Option<&str>) -> Result<Outcome> {
    let original = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let current = Manifest::parse(&original)?;
    let release = release(&current, tag)?;
    let version = version_of(&current, &release.tag_name)?;
    let (was, now) = (
        stable_version(&current.version)
            .with_context(|| format!("current pin {} is not comparable", current.version))?,
        stable_version(&version)?,
    );
    if now == was {
        return Ok(Outcome::Current { version });
    }
    if now < was {
        bail!(
            "{}: published {} is older than the pinned {}; refusing to downgrade the reviewed pin",
            current.name,
            release.tag_name,
            current.version
        );
    }
    let mut next = current.clone();
    next.version = version.clone();
    let has = |name: &str| release.assets.iter().filter(|a| a.name == name).count() == 1;
    if !has(&next.sums_asset) {
        bail!(
            "{}: release {} lacks a unique {}",
            next.name,
            release.tag_name,
            next.sums_asset
        );
    }
    let base = format!(
        "https://github.com/{}/releases/download/{}",
        next.repo, release.tag_name
    );
    let scratch = tempfile::tempdir().context("create bump download directory")?;
    let sums_path = scratch.path().join(&next.sums_asset);
    download_https(&format!("{base}/{}", next.sums_asset), &sums_path)?;
    let sums = fs::read_to_string(&sums_path).context("read published sums")?;
    // The reviewed target set is the one already pinned: a bump never adds or
    // silently drops a platform.
    // S1: the host's own asset is unpacked and run before anything is
    // recorded, so the bump fails here rather than on every fleet host.
    let host = crate::adapter::release_target(&current)
        .context("bump-member must run on a pinned target so it can prove the release")?;
    let mut proved = false;
    let mut digests = Vec::new();
    for target in current.sha256.keys() {
        let asset = next.asset_for(target);
        if !has(&asset) {
            bail!(
                "{}: release {} lacks a unique asset {asset} for pinned target {target}",
                next.name,
                release.tag_name
            );
        }
        let published =
            checksum(&sums, &asset).with_context(|| format!("{} for {target}", next.sums_asset))?;
        let local = scratch.path().join(&asset);
        download_https(&format!("{base}/{asset}"), &local)?;
        let actual = hash(&local)?;
        if actual != published {
            bail!(
                "{}: {asset} hashes to {actual} but {} publishes {published}; refusing to record either",
                next.name,
                next.sums_asset
            );
        }
        if target == host {
            crate::adapter::prove_release(&next, &local)?;
            proved = true;
        }
        digests.push((target.clone(), published));
    }

    if !proved {
        bail!(
            "{}: the {host} release was not proved; refusing to record",
            next.name
        );
    }
    let mut document: toml_edit::DocumentMut =
        original.parse().context("parse manifest for editing")?;
    document["version"] = toml_edit::value(version.clone());
    for (target, digest) in &digests {
        document["sha256"][target.as_str()] = toml_edit::value(digest.clone());
    }
    let written = document.to_string();
    // Read back what will be written before replacing the reviewed file.
    let check = Manifest::parse(&written)?;
    if check.version != version
        || digests.iter().any(|(t, d)| check.sha256.get(t) != Some(d))
        || check.sha256.len() != current.sha256.len()
    {
        bail!(
            "{}: rewritten manifest does not read back as the bump",
            next.name
        );
    }
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staged = tempfile::NamedTempFile::new_in(dir)?;
    fs::write(staged.path(), &written)?;
    staged
        .persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(Outcome::Bumped {
        from: current.version,
        to: version,
        digests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(tag: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"name = "demo"
kind = "rust-release"
repo = "owner/demo"
version = "1.2.3"
tag = "{tag}"
asset = "demo-{{tag}}-{{target}}.tar.gz"
version_argv = ["--version"]
sums_asset = "SHA256SUMS.txt"
[sha256]
x86_64-unknown-linux-gnu = "{}"
[[programs]]
name = "demo"
identity_argv = ["--help"]
identity_contains = "demo is the demo member"
[[verify]]
argv = ["demo", "list"]
absent = "{{marker}}"
[[verify]]
argv = ["demo", "add", "{{marker}}"]
[[verify]]
argv = ["demo", "list"]
present = "{{marker}}"
"#,
            "a".repeat(64)
        ))
        .unwrap()
    }

    #[test]
    fn a_tag_yields_its_version_only_under_the_manifest_template() {
        // A1: seeds tags are `seeds-ai-v<version>`, not derivable in general.
        let m = manifest("seeds-ai-v{version}");
        assert_eq!(version_of(&m, "seeds-ai-v0.0.3").unwrap(), "0.0.3");
        assert!(version_of(&m, "v0.0.3").is_err());
        assert!(version_of(&m, "seeds-ai-v0.0.3-rc1").is_err());
        assert_eq!(
            version_of(&manifest("v{version}"), "v2.0.0").unwrap(),
            "2.0.0"
        );
    }
}
