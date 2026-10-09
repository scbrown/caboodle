//! The Quechua vocabulary, pinned and loaded into the stack's Quipu
//! (aegis-1i5h1j.3).
//!
//! Quechua publishes the stack's shared terms. Pages serves a copy that follows
//! main; a RELEASE asset is immutable, so caboodle pins one by the SHA-256 it was
//! reviewed with (the same rule as every member: the release's own sums file is
//! never trusted at install time). Install caches the verified file and knots its
//! declarations into the plan's Quipu store; verify proves, in a throwaway
//! store, that the pinned file declares the pinned version, that a known term
//! resolves, and (the control) that an absent one does not.

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

use crate::adapter::{checked, download_https};

/// The reviewed release. Bump all three together, from the release's
/// SHA256SUMS.txt, re-derived by downloading and hashing the asset.
pub const QUECHUA_VERSION: &str = "0.1.0";
pub const QUECHUA_SHA256: &str = "eb0d7a10c86fcd9e5ad0a5f08a27bd6370f9439a951a9ea690a5919e4064d9bd";
const NS: &str = "https://scbrown.github.io/quechua/ns";

/// A term every release declares; verify's positive arm.
const KNOWN_TERM: &str = "WorkflowRun";

fn asset() -> String {
    format!("quechua-ns-v{QUECHUA_VERSION}.ttl")
}

/// Where the verified release is cached. `CABOODLE_QUECHUA_FILE` overrides it,
/// for tests and for an offline install from a reviewed copy.
pub fn cached_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CABOODLE_QUECHUA_FILE") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is required for the Quechua vocabulary")?;
    Ok(PathBuf::from(home)
        .join(".local/share/caboodle/vocabulary")
        .join(asset()))
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Ensure the reviewed release is cached, downloading it if absent, and accept
/// it only on the pinned digest. Returns the cached path.
pub fn ensure_cached() -> Result<PathBuf> {
    let path = cached_path()?;
    if !path.exists() {
        let dir = path.parent().context("vocabulary cache has no parent")?;
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let staged = tempfile::NamedTempFile::new_in(dir)?;
        download_https(
            &format!(
                "https://github.com/scbrown/quechua/releases/download/v{QUECHUA_VERSION}/{}",
                asset()
            ),
            staged.path(),
        )?;
        if sha256_file(staged.path())? != QUECHUA_SHA256 {
            bail!("Quechua v{QUECHUA_VERSION} release checksum mismatch; nothing cached");
        }
        staged
            .persist(&path)
            .map_err(|e| e.error)
            .with_context(|| format!("cache {}", path.display()))?;
    }
    if sha256_file(&path)? != QUECHUA_SHA256 {
        bail!(
            "{} does not hash to the reviewed Quechua v{QUECHUA_VERSION} digest; refusing it",
            path.display()
        );
    }
    Ok(path)
}

/// Knot the vocabulary's declarations into the store at `db`.
pub fn load(db: &Path) -> Result<()> {
    load_at(db, Path::new("quipu"))
}

fn load_at(db: &Path, client: &Path) -> Result<()> {
    let file = ensure_cached()?;
    checked(
        client,
        [
            OsStr::new("knot"),
            file.as_os_str(),
            OsStr::new("--db"),
            db.as_os_str(),
        ],
        None,
    )
    .with_context(|| format!("load Quechua v{QUECHUA_VERSION} into {}", db.display()))?;
    Ok(())
}

fn ask(db: &Path, query: &str, client: &Path) -> Result<bool> {
    let out = checked(
        client,
        [
            OsStr::new("read"),
            OsStr::new(query),
            OsStr::new("--db"),
            db.as_os_str(),
        ],
        None,
    )?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => bail!("unexpected ASK answer from quipu: {other:?}"),
    }
}

fn declared(db: &Path, term: &str, client: &Path) -> Result<bool> {
    ask(
        db,
        &format!(
            "ASK {{ {{ <{NS}#{term}> a <http://www.w3.org/2000/01/rdf-schema#Class> }} UNION \
             {{ <{NS}#{term}> a <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> }} }}"
        ),
        client,
    )
}

/// Verify in a throwaway store. The fresh store must NOT declare the known
/// term (control 1), then after loading the pinned release it must declare
/// the pinned version and the known term, and must still not declare a term
/// absent from every release (control 2).
pub fn verify() -> Result<()> {
    verify_at(Path::new("quipu"))
}

pub(crate) fn verify_at(client: &Path) -> Result<()> {
    let root = tempfile::tempdir().context("create Quechua verification directory")?;
    let db = root.path().join("vocabulary.db");
    if declared(&db, KNOWN_TERM, client).unwrap_or(false) {
        bail!("Quechua control failed: an empty store already declares {KNOWN_TERM}");
    }
    load_at(&db, client)?;
    let version = ask(
        &db,
        &format!(
            "ASK {{ <{NS}> <http://www.w3.org/2002/07/owl#versionInfo> \"{QUECHUA_VERSION}\" }}"
        ),
        client,
    )?;
    if !version {
        bail!("the loaded Quechua file does not declare owl:versionInfo {QUECHUA_VERSION}");
    }
    if !declared(&db, KNOWN_TERM, client)? {
        bail!("Quechua v{QUECHUA_VERSION} loaded but quechua:{KNOWN_TERM} does not resolve");
    }
    let absent = "CaboodleVerifyAbsentTerm";
    if declared(&db, absent, client)? {
        bail!("Quechua control failed: an undeclared term resolved");
    }
    Ok(())
}
