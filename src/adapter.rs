use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::model::ToolName;

pub trait Adapter {
    fn name(&self) -> ToolName;
    fn install(&self) -> Result<()>;
    fn version(&self) -> Result<String>;
    fn verify(&self) -> Result<()>;
}

pub fn adapter(name: ToolName) -> Box<dyn Adapter> {
    match name {
        ToolName::Quipu => Box::new(Quipu),
        ToolName::Camayoc => Box::new(Camayoc),
        ToolName::Bobbin => Box::new(Bobbin),
    }
}

fn output<I, S>(program: &str, args: I, cwd: Option<&Path>) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<OsString> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect();
    let mut command = Command::new(program);
    command.args(&args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().with_context(|| {
        format!(
            "run {} {}",
            program,
            args.iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )
    })
}

pub(crate) fn checked<I, S>(program: &str, args: I, cwd: Option<&Path>) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let result = output(program, args, cwd)?;
    if !result.status.success() {
        bail!(
            "{} failed ({}): {}",
            program,
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(result)
}

fn read_version(program: &str) -> Result<String> {
    let result = checked(program, ["--version"], None)?;
    let version = String::from_utf8_lossy(&result.stdout).trim().to_owned();
    if version.is_empty() {
        bail!("{} --version returned no version", program);
    }
    Ok(version)
}

fn require_minimum_version(output: &str, minimum: (u64, u64, u64), program: &str) -> Result<()> {
    let raw = output
        .split_whitespace()
        .nth(1)
        .with_context(|| format!("{program} --version omitted a semantic version"))?;
    let mut parts = raw.trim_start_matches('v').split('.');
    let found = (
        parts.next().and_then(|value| value.parse().ok()),
        parts.next().and_then(|value| value.parse().ok()),
        parts.next().and_then(|value| value.parse().ok()),
    );
    let found = match found {
        (Some(major), Some(minor), Some(patch)) => (major, minor, patch),
        _ => bail!("{program} returned an invalid semantic version: {raw}"),
    };
    if found < minimum {
        bail!(
            "{program} {}.{}.{} is too old; CABOODLE requires at least {}.{}.{}",
            found.0,
            found.1,
            found.2,
            minimum.0,
            minimum.1,
            minimum.2
        );
    }
    Ok(())
}

struct Quipu;

impl Adapter for Quipu {
    fn name(&self) -> ToolName {
        ToolName::Quipu
    }

    fn install(&self) -> Result<()> {
        checked(
            "cargo",
            [
                "install",
                "quipu-ai",
                "--version",
                "0.3.27",
                "--locked",
                "--features",
                "full",
                "--bins",
            ],
            None,
        )?;
        Ok(())
    }

    fn version(&self) -> Result<String> {
        let client = read_version("quipu")?;
        require_minimum_version(&client, (0, 3, 27), "quipu")?;
        let server = read_version("quipu-server")?;
        require_minimum_version(&server, (0, 3, 27), "quipu-server")?;
        Ok(format!("{client}; {server}"))
    }

    fn verify(&self) -> Result<()> {
        let root = tempfile::tempdir().context("create quipu verification directory")?;
        let db = root.path().join("verify.db");
        let episode = root.path().join("episode.json");
        let marker = "caboodle-verify-roundtrip";
        fs::write(
            &episode,
            format!(
                r#"{{"name":"caboodle verification","episode_body":"functional round trip","source":"caboodle","group_id":"caboodle-verification","nodes":[{{"name":"{marker}","type":"Verification","description":"caboodle isolated verification"}}],"edges":[]}}"#
            ),
        )
        .context("write quipu verification episode")?;

        let query = format!(
            "SELECT ?s ?label WHERE {{ ?s <http://www.w3.org/2000/01/rdf-schema#label> ?label . FILTER(?label = \"{marker}\") }}"
        );
        let before = checked(
            "quipu",
            [
                OsStr::new("read"),
                OsStr::new(&query),
                OsStr::new("--db"),
                db.as_os_str(),
            ],
            None,
        )?;
        if String::from_utf8_lossy(&before.stdout).contains(marker) {
            bail!("quipu negative control unexpectedly found the verification node");
        }

        checked(
            "quipu",
            [
                OsStr::new("episode"),
                episode.as_os_str(),
                OsStr::new("--db"),
                db.as_os_str(),
            ],
            None,
        )?;
        let after = checked(
            "quipu",
            [
                OsStr::new("read"),
                OsStr::new(&query),
                OsStr::new("--db"),
                db.as_os_str(),
            ],
            None,
        )?;
        if !String::from_utf8_lossy(&after.stdout).contains(marker) {
            bail!("quipu episode landed but read-back did not find the verification node");
        }
        Ok(())
    }
}

struct Bobbin;

struct Camayoc;

impl Adapter for Camayoc {
    fn name(&self) -> ToolName {
        ToolName::Camayoc
    }

    fn install(&self) -> Result<()> {
        install_camayoc_bundle()
    }

    fn version(&self) -> Result<String> {
        let revision = fs::read_to_string(camayoc_root()?.join("REVISION"))
            .context("read installed Camayoc revision")?;
        let revision = revision.trim();
        if revision.is_empty() {
            bail!("installed Camayoc revision is empty");
        }
        Ok(format!("camayoc {revision}"))
    }

    fn verify(&self) -> Result<()> {
        let root = camayoc_root()?;
        checked(
            "bash",
            [root.join("scripts/bootstrap.sh").as_os_str()],
            None,
        )?;
        verify_camayoc_first_ingest(&root)
    }
}

impl Adapter for Bobbin {
    fn name(&self) -> ToolName {
        ToolName::Bobbin
    }

    fn install(&self) -> Result<()> {
        install_bobbin_release()
    }

    fn version(&self) -> Result<String> {
        read_version("bobbin")
    }

    fn verify(&self) -> Result<()> {
        let root = tempfile::tempdir().context("create bobbin verification repository")?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock predates Unix epoch")?
            .as_nanos();
        let repo = format!("caboodle-verify-{}-{nonce}", std::process::id());
        let marker = format!("caboodle_verify_marker_{}_{nonce}", std::process::id());
        checked("git", ["init", "--quiet"], Some(root.path()))?;
        checked("bobbin", ["init"], Some(root.path()))?;
        fs::write(
            root.path().join("control.rs"),
            "/// Proves the negative-control index is live.\npub fn caboodle_control() -> usize {\n    1\n}\n",
        )
        .context("write bobbin negative-control fixture")?;
        checked(
            "bobbin",
            [
                "index",
                ".",
                "--source",
                ".",
                "--repo",
                &repo,
                "--skip-calibrate",
            ],
            Some(root.path()),
        )?;

        let before = checked(
            "bobbin",
            ["grep", &marker, "--repo", &repo, "--json"],
            Some(root.path()),
        )?;
        if bobbin_result_count(&before.stdout)? != 0 {
            bail!("bobbin negative control unexpectedly found the fixture marker");
        }

        fs::write(
            root.path().join("fixture.rs"),
            format!(
                "/// A unique CABOODLE verification routine.\npub fn {marker}() -> &'static str {{\n    \"indexed\"\n}}\n"
            ),
        )
        .context("write bobbin verification fixture")?;
        checked(
            "bobbin",
            [
                "index",
                ".",
                "--source",
                ".",
                "--repo",
                &repo,
                "--skip-calibrate",
            ],
            Some(root.path()),
        )?;
        let after = checked(
            "bobbin",
            ["grep", &marker, "--repo", &repo, "--json"],
            Some(root.path()),
        )?;
        if bobbin_result_count(&after.stdout)? == 0 {
            bail!("bobbin indexed the fixture but search did not return its marker");
        }
        Ok(())
    }
}

fn bobbin_result_count(stdout: &[u8]) -> Result<u64> {
    let response: serde_json::Value =
        serde_json::from_slice(stdout).context("parse bobbin search JSON")?;
    response["count"]
        .as_u64()
        .context("bobbin search JSON omitted numeric count")
}

fn install_bobbin_release() -> Result<()> {
    const VERSION: &str = "0.9.0";
    let target = match (env::consts::ARCH, env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        (arch, os) => bail!("bobbin has no CABOODLE release target for {arch}-{os}"),
    };
    let archive = format!("bobbin-v{VERSION}-{target}.tar.gz");
    let base = format!("https://github.com/scbrown/bobbin/releases/download/v{VERSION}");
    let download = tempfile::tempdir().context("create bobbin download directory")?;
    let archive_path = download.path().join(&archive);
    let sums_path = download.path().join("SHA256SUMS.txt");
    download_https(&format!("{base}/{archive}"), &archive_path)?;
    download_https(&format!("{base}/SHA256SUMS.txt"), &sums_path)?;
    verify_checksum(&archive_path, &sums_path)?;

    let home = env::var_os("HOME").context("HOME is required to install bobbin")?;
    let install_root = PathBuf::from(&home)
        .join(".local/share/caboodle/bobbin")
        .join(format!("v{VERSION}"));
    fs::create_dir_all(&install_root)
        .with_context(|| format!("create bobbin install root {}", install_root.display()))?;
    checked(
        "tar",
        [
            OsStr::new("-xzf"),
            archive_path.as_os_str(),
            OsStr::new("--strip-components=1"),
            OsStr::new("-C"),
            install_root.as_os_str(),
        ],
        None,
    )?;

    let bin_dir = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".cargo"))
        .join("bin");
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("create binary directory {}", bin_dir.display()))?;
    let link = bin_dir.join("bobbin");
    if link.exists() || link.symlink_metadata().is_ok() {
        bail!(
            "refusing to replace existing {}; remove or repair it explicitly",
            link.display()
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(install_root.join("bobbin"), &link)
        .with_context(|| format!("link bobbin into {}", link.display()))?;
    #[cfg(not(unix))]
    bail!("bobbin release installation currently requires a Unix host");
    Ok(())
}

const CAMAYOC_REVISION: &str = "f33da14bba7bdd579852f5ddaa5d6328197d806f";
const CAMAYOC_ARCHIVE_SHA256: &str =
    "e3e9ebb2975dd6c518c051930f5dd9b61560046195793356cde8b62d42483086";

fn camayoc_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CABOODLE_CAMAYOC_ROOT") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is required to install Camayoc")?;
    Ok(PathBuf::from(home)
        .join(".local/share/caboodle/camayoc")
        .join(CAMAYOC_REVISION))
}

fn install_camayoc_bundle() -> Result<()> {
    let root = camayoc_root()?;
    if root.join("REVISION").exists() {
        return Ok(());
    }
    let download = tempfile::tempdir().context("create Camayoc download directory")?;
    let archive = download.path().join("camayoc.tar.gz");
    download_https(
        &format!("https://github.com/scbrown/camayoc/archive/{CAMAYOC_REVISION}.tar.gz"),
        &archive,
    )?;
    let digest = checked("sha256sum", [archive.as_os_str()], None)?;
    let digest = String::from_utf8_lossy(&digest.stdout);
    if digest.split_whitespace().next() != Some(CAMAYOC_ARCHIVE_SHA256) {
        bail!("Camayoc archive checksum mismatch");
    }
    fs::create_dir_all(&root)
        .with_context(|| format!("create Camayoc install root {}", root.display()))?;
    checked(
        "tar",
        [
            OsStr::new("-xzf"),
            archive.as_os_str(),
            OsStr::new("--strip-components=1"),
            OsStr::new("-C"),
            root.as_os_str(),
        ],
        None,
    )?;
    fs::write(root.join("REVISION"), format!("{CAMAYOC_REVISION}\n"))
        .context("write installed Camayoc revision")?;
    Ok(())
}

fn verify_camayoc_first_ingest(root: &Path) -> Result<()> {
    let server = env::var("QUIPU_SERVER").unwrap_or_else(|_| "http://localhost:3030".to_owned());
    let namespace = camayoc_aegis_namespace(&root.join("ontology/core.ttl"))?;
    let control = "caboodle-camayoc-control-must-stay-absent";
    let marker = "caboodle-camayoc-first-ingest-v1";

    if label_count(&server, &namespace, control)? != 0 {
        bail!("Camayoc negative control unexpectedly exists");
    }

    let turtle = format!(
        "@prefix aegis: <{namespace}> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         aegis:{marker} a aegis:Verification ; rdfs:label \"{marker}\" ; \
         aegis:sourceKind \"observed\" ; aegis:falsifier \"the marker remains absent after ingest\" .\n"
    );
    let payload = json!({
        "turtle": turtle,
        "actor": "caboodle",
        "source": "caboodle Camayoc first-ingest verification"
    });
    if label_count(&server, &namespace, marker)? == 0 {
        let first = curl_json(&format!("{server}/knot"), &payload)?;
        if first.get("count").and_then(Value::as_u64).unwrap_or(0) == 0 {
            bail!("Camayoc first ingest wrote no triples");
        }
    }
    if label_count(&server, &namespace, marker)? == 0 {
        bail!("Camayoc first ingest was not retrievable");
    }
    curl_json(&format!("{server}/knot"), &payload)?;
    if label_count(&server, &namespace, marker)? != 1 {
        bail!("Camayoc idempotent replay wrote duplicate triples");
    }
    Ok(())
}

fn camayoc_aegis_namespace(path: &Path) -> Result<String> {
    let body = fs::read_to_string(path)
        .with_context(|| format!("read Camayoc ontology {}", path.display()))?;
    body.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("@prefix aegis: <")
                .and_then(|rest| rest.strip_suffix("> ."))
                .map(str::to_owned)
        })
        .context("Camayoc ontology does not declare the aegis namespace")
}

fn label_count(server: &str, namespace: &str, label: &str) -> Result<u64> {
    let query = format!(
        "SELECT ?s WHERE {{ ?s <http://www.w3.org/2000/01/rdf-schema#label> \"{label}\" . FILTER(STRSTARTS(STR(?s), \"{namespace}\")) }}"
    );
    let response = curl_json(&format!("{server}/query"), &json!({"query": query}))?;
    response["count"]
        .as_u64()
        .context("Quipu query response omitted numeric count")
}

fn curl_json(url: &str, payload: &Value) -> Result<Value> {
    let body = payload.to_string();
    let mut auth = None;
    if let Ok(token) = env::var("QUIPU_AUTH_TOKEN") {
        let file = tempfile::NamedTempFile::new().context("create temporary Quipu auth config")?;
        fs::write(
            file.path(),
            format!("header = \"Authorization: Bearer {token}\"\n"),
        )
        .context("write temporary Quipu auth config")?;
        auth = Some(file);
    }
    let mut args = vec![
        OsString::from("--fail"),
        OsString::from("--silent"),
        OsString::from("--show-error"),
        OsString::from("--request"),
        OsString::from("POST"),
        OsString::from("--header"),
        OsString::from("Content-Type: application/json"),
        OsString::from("--data"),
        OsString::from(body),
    ];
    if let Some(file) = &auth {
        args.push(OsString::from("--config"));
        args.push(file.path().as_os_str().to_owned());
    }
    args.push(OsString::from(url));
    let result = checked("curl", args, None)?;
    serde_json::from_slice(&result.stdout)
        .with_context(|| format!("parse JSON response from {url}"))
}

pub(crate) fn download_https(url: &str, destination: &Path) -> Result<()> {
    checked(
        "curl",
        [
            OsStr::new("--fail"),
            OsStr::new("--location"),
            OsStr::new("--proto"),
            OsStr::new("=https"),
            OsStr::new("--tlsv1.2"),
            OsStr::new("--output"),
            destination.as_os_str(),
            OsStr::new(url),
        ],
        None,
    )?;
    Ok(())
}

fn verify_checksum(archive: &Path, sums: &Path) -> Result<()> {
    let filename = archive
        .file_name()
        .and_then(OsStr::to_str)
        .context("bobbin archive filename is not UTF-8")?;
    let expected = fs::read_to_string(sums)
        .context("read Bobbin checksum manifest")?
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let listed = fields.next()?.trim_start_matches('*');
            (listed == filename).then(|| digest.to_owned())
        })
        .with_context(|| format!("checksum manifest does not cover {filename}"))?;
    let actual = checked("sha256sum", [archive.as_os_str()], None)?;
    let actual = String::from_utf8_lossy(&actual.stdout)
        .split_whitespace()
        .next()
        .context("sha256sum returned no digest")?
        .to_owned();
    if actual != expected {
        bail!("checksum mismatch for {filename}");
    }
    Ok(())
}
