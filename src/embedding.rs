//! Checksum-pinned embedding-model artifact provisioning.
//!
//! Quipu deliberately bundles no model weights (its onnx feature ships the
//! runtime only), so the first hour of a box needs the artifacts fetched from
//! somewhere — and a fetch trusted on its exit code is exactly the banner
//! this repo exists to refuse. Every artifact is pinned by sha256 in the
//! reviewed plan; a mismatched download is deleted, named, and fatal.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingModel {
    /// Directory that receives every artifact; created on demand.
    pub destination: PathBuf,
    pub artifacts: Vec<ModelArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelArtifact {
    pub name: String,
    pub url: String,
    pub sha256: String,
}

impl EmbeddingModel {
    pub fn read(path: &Path) -> Result<Self> {
        let body = fs::read_to_string(path)
            .with_context(|| format!("read embedding model spec {}", path.display()))?;
        let model: Self = toml::from_str(&body)
            .with_context(|| format!("parse embedding model spec {}", path.display()))?;
        model.validate()?;
        Ok(model)
    }

    pub fn validate(&self) -> Result<()> {
        if self.destination.as_os_str().is_empty() {
            bail!("embedding model destination must not be empty");
        }
        if self.artifacts.is_empty() {
            bail!("embedding model section names no artifacts");
        }
        let mut names = BTreeSet::new();
        for artifact in &self.artifacts {
            // A name is a file name, never a path: a plan that says
            // "../../etc/shadow" must fail at review time, not at fetch time.
            if artifact.name.is_empty()
                || artifact.name.contains('/')
                || artifact.name.contains('\\')
                || artifact.name == "."
                || artifact.name == ".."
            {
                bail!(
                    "embedding model artifact name {:?} must be a bare file name",
                    artifact.name
                );
            }
            if !names.insert(artifact.name.clone()) {
                bail!("duplicate embedding model artifact {:?}", artifact.name);
            }
            if !artifact.url.starts_with("https://") {
                bail!(
                    "embedding model artifact {} must use an https:// url",
                    artifact.name
                );
            }
            if artifact.sha256.len() != 64
                || !artifact
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                bail!(
                    "embedding model artifact {} sha256 must be 64 lowercase hex characters",
                    artifact.name
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactOutcome {
    /// Downloaded this run and proven against the pinned digest.
    Fetched,
    /// Already on disk with a matching digest; recorded, not re-downloaded.
    Current,
}

#[derive(Debug)]
pub struct ProvisionedArtifact {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub outcome: ArtifactOutcome,
}

/// Fetch every artifact through `fetch`, refusing checksum mismatches. An
/// artifact already on disk with a matching digest is a recorded no-op, so a
/// rerun converges instead of re-downloading model weights.
pub fn provision<F>(model: &EmbeddingModel, mut fetch: F) -> Result<Vec<ProvisionedArtifact>>
where
    F: FnMut(&str, &Path) -> Result<()>,
{
    model.validate()?;
    fs::create_dir_all(&model.destination).with_context(|| {
        format!(
            "create embedding model destination {}",
            model.destination.display()
        )
    })?;
    let mut provisioned = Vec::new();
    for artifact in &model.artifacts {
        let path = model.destination.join(&artifact.name);
        if path.is_file() && sha256_file(&path)? == artifact.sha256 {
            provisioned.push(ProvisionedArtifact {
                name: artifact.name.clone(),
                path,
                sha256: artifact.sha256.clone(),
                outcome: ArtifactOutcome::Current,
            });
            continue;
        }
        // Download beside the destination so the final rename is atomic and a
        // mismatched download can never be observed at the artifact path.
        let staged = tempfile::NamedTempFile::new_in(&model.destination).with_context(|| {
            format!(
                "stage embedding model download in {}",
                model.destination.display()
            )
        })?;
        fetch(&artifact.url, staged.path())
            .with_context(|| format!("fetch embedding model artifact {}", artifact.name))?;
        let downloaded = sha256_file(staged.path())?;
        if downloaded != artifact.sha256 {
            // Dropping `staged` deletes the mismatched bytes; nothing
            // untrusted remains on disk after this refusal.
            bail!(
                "embedding model artifact {} checksum mismatch: plan pins {}, download hashed {}; the download was deleted",
                artifact.name,
                artifact.sha256,
                downloaded
            );
        }
        staged
            .persist(&path)
            .map_err(|error| error.error)
            .with_context(|| format!("place embedding model artifact {}", path.display()))?;
        provisioned.push(ProvisionedArtifact {
            name: artifact.name.clone(),
            path,
            sha256: artifact.sha256.clone(),
            outcome: ArtifactOutcome::Fetched,
        });
    }
    Ok(provisioned)
}

/// Re-hash every provisioned artifact against the plan's pinned digest.
///
/// This proves the bytes on disk are still the reviewed bytes. It does NOT
/// prove an embed-and-search round-trip: the installed Quipu exposes no
/// reviewed embeddings contract for caboodle to drive yet, and a proof that
/// cannot fail would be banner inflation. The book states exactly this split.
pub fn verify(model: &EmbeddingModel) -> Result<()> {
    model.validate()?;
    for artifact in &model.artifacts {
        let path = model.destination.join(&artifact.name);
        if !path.is_file() {
            bail!(
                "embedding model artifact {} is missing from {}",
                artifact.name,
                model.destination.display()
            );
        }
        let found = sha256_file(&path)?;
        if found != artifact.sha256 {
            bail!(
                "embedding model artifact {} drifted on disk: plan pins {}, file hashed {}",
                artifact.name,
                artifact.sha256,
                found
            );
        }
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("open {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).with_context(|| format!("hash {}", path.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_model(destination: &Path, sha256: &str) -> EmbeddingModel {
        EmbeddingModel {
            destination: destination.to_path_buf(),
            artifacts: vec![ModelArtifact {
                name: "model.onnx".to_owned(),
                url: "https://models.test/model.onnx".to_owned(),
                sha256: sha256.to_owned(),
            }],
        }
    }

    fn digest_of(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    #[test]
    fn sha256_file_matches_the_published_test_vector() {
        // FIPS 180-2 vector for sha256("abc"): the other tests here derive
        // expectations from this same hasher, so one external anchor keeps a
        // hasher bug from validating itself.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("abc");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn provision_fetches_once_and_is_idempotent_afterwards() {
        let root = tempfile::tempdir().unwrap();
        let model = fixture_model(&root.path().join("dest"), &digest_of(b"weights"));
        let mut fetches = 0;
        let mut fetch = |_: &str, path: &Path| {
            fetches += 1;
            fs::write(path, b"weights").map_err(Into::into)
        };
        let first = provision(&model, &mut fetch).unwrap();
        assert_eq!(first[0].outcome, ArtifactOutcome::Fetched);
        let second = provision(&model, &mut fetch).unwrap();
        assert_eq!(second[0].outcome, ArtifactOutcome::Current);
        assert_eq!(fetches, 1);
        assert_eq!(
            fs::read(root.path().join("dest/model.onnx")).unwrap(),
            b"weights"
        );
    }

    #[test]
    fn provision_refuses_a_checksum_mismatch_and_deletes_the_download() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("dest");
        let model = fixture_model(&destination, &digest_of(b"expected"));
        let error = provision(&model, |_, path| {
            fs::write(path, b"tampered").map_err(Into::into)
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("model.onnx checksum mismatch"), "{error}");
        assert!(error.contains(&digest_of(b"expected")), "{error}");
        // The refused bytes must be gone: neither the artifact path nor a
        // stray staging file may survive the failure.
        assert!(!destination.join("model.onnx").exists());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    }

    #[test]
    fn provision_replaces_a_drifted_artifact_atomically() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("dest");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("model.onnx"), b"stale").unwrap();
        let model = fixture_model(&destination, &digest_of(b"weights"));
        let provisioned = provision(&model, |_, path| {
            fs::write(path, b"weights").map_err(Into::into)
        })
        .unwrap();
        assert_eq!(provisioned[0].outcome, ArtifactOutcome::Fetched);
        assert_eq!(
            fs::read(destination.join("model.onnx")).unwrap(),
            b"weights"
        );
    }

    #[test]
    fn verify_goes_red_on_drift_or_absence_and_green_on_pinned_bytes() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("dest");
        let model = fixture_model(&destination, &digest_of(b"weights"));
        let missing = verify(&model).unwrap_err().to_string();
        assert!(missing.contains("model.onnx is missing"), "{missing}");

        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("model.onnx"), b"weights").unwrap();
        verify(&model).unwrap();

        fs::write(destination.join("model.onnx"), b"weightz").unwrap();
        let drifted = verify(&model).unwrap_err().to_string();
        assert!(drifted.contains("model.onnx drifted on disk"), "{drifted}");
    }

    #[test]
    fn validation_refuses_paths_plain_http_and_unpinned_digests() {
        let sha = digest_of(b"weights");
        let mut traversal = fixture_model(Path::new("dest"), &sha);
        traversal.artifacts[0].name = "../escape.onnx".to_owned();
        assert!(traversal.validate().is_err());

        let mut insecure = fixture_model(Path::new("dest"), &sha);
        insecure.artifacts[0].url = "http://models.test/model.onnx".to_owned();
        assert!(insecure.validate().is_err());

        let short = fixture_model(Path::new("dest"), "abc123");
        assert!(short.validate().is_err());
        let uppercase = fixture_model(Path::new("dest"), &sha.to_ascii_uppercase());
        assert!(uppercase.validate().is_err());

        let empty = EmbeddingModel {
            destination: PathBuf::from("dest"),
            artifacts: Vec::new(),
        };
        assert!(empty.validate().is_err());
    }
}
