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

use crate::model::{QuipuFlavor, ToolName};

pub trait Adapter {
    fn name(&self) -> ToolName;
    /// Release identity reviewed and pinned by this Caboodle build.
    fn desired_version(&self) -> String;
    fn install(&self) -> Result<()>;
    fn version(&self) -> Result<String>;
    fn verify(&self) -> Result<()>;

    fn is_current(&self, installed: &str) -> bool {
        installed == self.desired_version()
    }
}

pub fn adapter(name: ToolName, quipu_flavor: QuipuFlavor) -> Box<dyn Adapter> {
    match name {
        ToolName::Quipu => Box::new(Quipu {
            flavor: quipu_flavor,
        }),
        ToolName::Camayoc => Box::new(Camayoc),
        ToolName::Bobbin => Box::new(Bobbin),
        ToolName::Yupana => Box::new(Yupana),
        ToolName::DesirePath => Box::new(DesirePath),
    }
}

fn output<P, I, S>(program: P, args: I, cwd: Option<&Path>) -> Result<Output>
where
    P: AsRef<OsStr>,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<OsString> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect();
    let program = program.as_ref();
    let mut command = Command::new(program);
    command.args(&args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().with_context(|| {
        format!(
            "run {} {}",
            program.to_string_lossy(),
            args.iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        )
    })
}

pub(crate) fn checked<P, I, S>(program: P, args: I, cwd: Option<&Path>) -> Result<Output>
where
    P: AsRef<OsStr>,
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let program = program.as_ref();
    let result = output(program, args, cwd)?;
    if !result.status.success() {
        bail!(
            "{} failed ({}): {}",
            program.to_string_lossy(),
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

fn read_cargo_version(program: &str) -> Result<String> {
    let program_path = cargo_program(program);
    read_version(
        program_path
            .to_str()
            .with_context(|| format!("{} path is not valid UTF-8", program_path.display()))?,
    )
}

fn cargo_program(program: &str) -> PathBuf {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    if let Some(path) = cargo_home.map(|home| home.join("bin").join(program)) {
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from(program)
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

struct Quipu {
    flavor: QuipuFlavor,
}

const QUIPU_VERSION: &str = "0.3.27";

/// The reviewed feature set per flavor. Both flavors build the identical
/// pinned revision — only the feature list differs — so choosing `lancedb`
/// can never smuggle in an unreviewed Quipu.
fn quipu_features(flavor: QuipuFlavor) -> &'static str {
    match flavor {
        QuipuFlavor::Release => "full",
        QuipuFlavor::Lancedb => "full,lancedb",
    }
}

impl Adapter for Quipu {
    fn name(&self) -> ToolName {
        ToolName::Quipu
    }

    fn desired_version(&self) -> String {
        match self.flavor {
            QuipuFlavor::Release => format!("quipu {QUIPU_VERSION}; quipu-server {QUIPU_VERSION}"),
            QuipuFlavor::Lancedb => {
                format!("quipu {QUIPU_VERSION}; quipu-server {QUIPU_VERSION} (+lancedb)")
            }
        }
    }

    fn is_current(&self, _installed: &str) -> bool {
        // version() already enforces the reviewed minimum for both binaries;
        // a newer compatible Quipu release is not a downgrade candidate.
        // Flavor drift is invisible here — `quipu --version` cannot say which
        // features were compiled in — so verify() is what catches it.
        true
    }

    fn install(&self) -> Result<()> {
        checked("cargo", quipu_install_args(self.flavor), None)?;
        Ok(())
    }

    fn version(&self) -> Result<String> {
        // `cargo install` writes these binaries under CARGO_HOME, which may be
        // later on PATH than a legacy Quipu install. Read back the location we
        // actually update so a successful install cannot be reported as stale.
        let client = read_cargo_version("quipu")?;
        require_minimum_version(&client, (0, 3, 27), "quipu")?;
        let server = read_cargo_version("quipu-server")?;
        require_minimum_version(&server, (0, 3, 27), "quipu-server")?;
        Ok(format!("{client}; {server}"))
    }

    fn verify(&self) -> Result<()> {
        if self.flavor == QuipuFlavor::Lancedb {
            verify_quipu_lancedb_feature()?;
        }
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

fn quipu_install_args(flavor: QuipuFlavor) -> [&'static str; 8] {
    [
        "install",
        "quipu-ai",
        "--version",
        QUIPU_VERSION,
        "--locked",
        "--features",
        quipu_features(flavor),
        "--bins",
    ]
}

/// The plan asked for the lancedb flavor. `quipu --version` cannot say which
/// features were compiled in, so ask the running server's per-feature compile
/// map — the only read-back that can prove the flavor, as opposed to trusting
/// that `cargo install --features lancedb` exited zero.
fn verify_quipu_lancedb_feature() -> Result<()> {
    let server = env::var("QUIPU_SERVER").unwrap_or_else(|_| "http://localhost:3030".to_owned());
    let version = curl_get_json(&format!("{server}/version"))
        .context("read the Quipu server per-feature compile map")?;
    require_compiled_feature(&version, "lancedb")
}

fn require_compiled_feature(version: &Value, feature: &str) -> Result<()> {
    let compiled = version
        .get("features")
        .and_then(Value::as_object)
        .with_context(|| {
            format!(
                "Quipu /version returned no per-feature compile map; cannot prove the {feature} flavor"
            )
        })?;
    if compiled.get(feature).and_then(Value::as_bool) != Some(true) {
        bail!(
            "the plan requires the {feature} flavor but the running Quipu was compiled without it; \
             reinstall the reviewed revision with `cargo install quipu-ai --version {QUIPU_VERSION} \
             --locked --features full,{feature} --bins`"
        );
    }
    Ok(())
}

struct Bobbin;
struct Yupana;
struct DesirePath;

struct Camayoc;
const BOBBIN_VERSION: &str = "0.10.3";

impl Adapter for Camayoc {
    fn name(&self) -> ToolName {
        ToolName::Camayoc
    }

    fn desired_version(&self) -> String {
        format!("camayoc {CAMAYOC_REVISION}")
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

    fn desired_version(&self) -> String {
        format!("bobbin {BOBBIN_VERSION}")
    }

    fn is_current(&self, installed: &str) -> bool {
        installed.strip_prefix("bobbin ").is_some_and(|rest| {
            rest == BOBBIN_VERSION || rest.starts_with(&format!("{BOBBIN_VERSION} "))
        })
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

const YUPANA_VERSION: &str = "0.6.4";
const YUPANA_ARCHIVE_SHA256: &str =
    "f227b965741851dff8f3bc59dbb80c80a0bd80d1469739b596c2eac0b36bcca2";

impl Adapter for Yupana {
    fn name(&self) -> ToolName {
        ToolName::Yupana
    }

    fn desired_version(&self) -> String {
        format!("yupana {YUPANA_VERSION}")
    }

    fn install(&self) -> Result<()> {
        if (env::consts::ARCH, env::consts::OS) != ("x86_64", "linux") {
            bail!(
                "Yupana v{YUPANA_VERSION} has no checksummed CABOODLE release for {}-{}",
                env::consts::ARCH,
                env::consts::OS
            );
        }
        let archive_name = format!("yupana-v{YUPANA_VERSION}-x86_64-linux-gnu.tar.gz");
        let root = tempfile::tempdir().context("create Yupana download directory")?;
        let archive = root.path().join(&archive_name);
        download_https(
            &format!("https://github.com/scbrown/yupana/releases/download/v{YUPANA_VERSION}/{archive_name}"),
            &archive,
        )?;
        let digest = checked("sha256sum", [archive.as_os_str()], None)?;
        if String::from_utf8_lossy(&digest.stdout)
            .split_whitespace()
            .next()
            != Some(YUPANA_ARCHIVE_SHA256)
        {
            bail!("Yupana release checksum mismatch");
        }
        checked(
            "tar",
            [
                OsStr::new("-xzf"),
                archive.as_os_str(),
                OsStr::new("-C"),
                root.path().as_os_str(),
            ],
            None,
        )?;
        let home = env::var_os("HOME").context("HOME is required to install Yupana")?;
        let bin = env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".cargo"))
            .join("bin");
        fs::create_dir_all(&bin).with_context(|| format!("create {}", bin.display()))?;
        fs::copy(root.path().join("yupana"), bin.join("yupana"))
            .context("install checksummed Yupana binary")?;
        Ok(())
    }

    fn version(&self) -> Result<String> {
        let version = read_version("yupana")?;
        require_minimum_version(&version, (0, 6, 4), "yupana")?;
        Ok(version)
    }

    fn verify(&self) -> Result<()> {
        let root = tempfile::tempdir().context("create Yupana verification repository")?;
        let state = root.path().join("state");
        fs::write(
            root.path().join("control.rs"),
            "pub fn caboodle_yupana_control() -> usize { 1 }\n",
        )?;
        yupana_checked(root.path(), &state, ["analyze", "."])?;
        let before = yupana_checked(
            root.path(),
            &state,
            ["callers", "caboodle_yupana_target", "."],
        )?;
        if String::from_utf8_lossy(&before.stdout).contains("caboodle_yupana_caller") {
            bail!("Yupana negative control unexpectedly found the fixture caller");
        }
        fs::write(
            root.path().join("fixture.rs"),
            "pub fn caboodle_yupana_target() -> usize { 1 }\npub fn caboodle_yupana_caller() -> usize { caboodle_yupana_target() }\n",
        )?;
        yupana_checked(root.path(), &state, ["analyze", "."])?;
        let after = yupana_checked(
            root.path(),
            &state,
            ["callers", "caboodle_yupana_target", "."],
        )?;
        if !String::from_utf8_lossy(&after.stdout).contains("caboodle_yupana_caller") {
            bail!("Yupana analyzed the fixture but callers did not return its caller");
        }
        Ok(())
    }
}

fn yupana_checked<I, S>(cwd: &Path, state: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let home = cwd.join("home");
    fs::create_dir_all(&home)?;
    let mut command = Command::new("yupana");
    command
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state);
    let result = command
        .output()
        .context("run isolated Yupana verification")?;
    if !result.status.success() {
        bail!(
            "isolated Yupana verification failed ({}): {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(result)
}

const DESIRE_PATH_REVISION: &str = "1ca7b36a73a6c931ac962dbfd093455f85f2d8ca";
const DESIRE_PATH_VERSION: &str = "v0.0.0-caboodle.20260827";

impl Adapter for DesirePath {
    fn name(&self) -> ToolName {
        ToolName::DesirePath
    }

    fn desired_version(&self) -> String {
        format!("dp {DESIRE_PATH_VERSION} ({})", &DESIRE_PATH_REVISION[..7])
    }

    fn install(&self) -> Result<()> {
        let home = env::var_os("HOME").context("HOME is required to install Desire Path")?;
        let bin = env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".cargo"))
            .join("bin");
        fs::create_dir_all(&bin).with_context(|| format!("create {}", bin.display()))?;
        let ldflags = format!(
            "-X github.com/scbrown/desire-path/internal/cli.Version={DESIRE_PATH_VERSION} -X github.com/scbrown/desire-path/internal/cli.Commit={DESIRE_PATH_REVISION}"
        );
        let mut command = Command::new("go");
        command
            .args([
                "install",
                "-ldflags",
                &ldflags,
                &format!("github.com/scbrown/desire-path/cmd/dp@{DESIRE_PATH_REVISION}"),
            ])
            .env("GOBIN", &bin);
        let result = command
            .output()
            .context("build pinned Desire Path revision")?;
        if !result.status.success() {
            bail!(
                "Desire Path install failed ({}): {}",
                result.status,
                String::from_utf8_lossy(&result.stderr).trim()
            );
        }
        Ok(())
    }

    fn version(&self) -> Result<String> {
        let result = checked(cargo_program("dp"), ["version"], None)?;
        let version = String::from_utf8_lossy(&result.stdout).trim().to_owned();
        if !version.contains(DESIRE_PATH_VERSION) || !version.contains(&DESIRE_PATH_REVISION[..7]) {
            bail!("dp version is not the CABOODLE-pinned revision: {version}");
        }
        Ok(version)
    }

    fn verify(&self) -> Result<()> {
        let dp = cargo_program("dp");
        let root = tempfile::tempdir().context("create Desire Path verification directory")?;
        let db = root.path().join("desires.db");
        let marker = "caboodle_desire_path_marker";
        let before = checked(
            &dp,
            [
                "--db",
                db.to_str().context("Desire Path DB path is not UTF-8")?,
                "--json",
                "list",
            ],
            Some(root.path()),
        )?;
        if String::from_utf8_lossy(&before.stdout).contains(marker) {
            bail!("Desire Path negative control unexpectedly found the marker");
        }
        let mut child = Command::new(&dp)
            .args([
                "--db",
                db.to_str().unwrap(),
                "ingest",
                "--source",
                "claude-code",
            ])
            .current_dir(root.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("start Desire Path ingest verification")?;
        use std::io::Write as _;
        child
            .stdin
            .take()
            .context("open Desire Path verification stdin")?
            .write_all(
                format!(r#"{{"tool_name":"{marker}","error":"fixture failure"}}"#).as_bytes(),
            )?;
        let ingested = child.wait_with_output()?;
        if !ingested.status.success() {
            bail!(
                "Desire Path ingest verification failed ({}): {}",
                ingested.status,
                String::from_utf8_lossy(&ingested.stderr).trim()
            );
        }
        let after = checked(
            &dp,
            ["--db", db.to_str().unwrap(), "--json", "list"],
            Some(root.path()),
        )?;
        if !String::from_utf8_lossy(&after.stdout).contains(marker) {
            bail!("Desire Path ingested the fixture but its reader path did not return it");
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
    let target = match (env::consts::ARCH, env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        (arch, os) => bail!("bobbin has no CABOODLE release target for {arch}-{os}"),
    };
    let archive = format!("bobbin-v{BOBBIN_VERSION}-{target}.tar.gz");
    let base = format!("https://github.com/scbrown/bobbin/releases/download/v{BOBBIN_VERSION}");
    let download = tempfile::tempdir().context("create bobbin download directory")?;
    let archive_path = download.path().join(&archive);
    let sums_path = download.path().join("SHA256SUMS.txt");
    download_https(&format!("{base}/{archive}"), &archive_path)?;
    download_https(&format!("{base}/SHA256SUMS.txt"), &sums_path)?;
    verify_checksum(&archive_path, &sums_path)?;

    let home = env::var_os("HOME").context("HOME is required to install bobbin")?;
    let install_root = PathBuf::from(&home)
        .join(".local/share/caboodle/bobbin")
        .join(format!("v{BOBBIN_VERSION}"));
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
    let target = install_root.join("bobbin");
    ensure_bobbin_link(&link, &target)?;
    Ok(())
}

fn ensure_bobbin_link(link: &Path, target: &Path) -> Result<()> {
    if !target.is_file() {
        bail!("Bobbin release archive omitted {}", target.display());
    }
    match link.symlink_metadata() {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                && fs::read_link(link).ok().as_deref() == Some(target) =>
        {
            return Ok(());
        }
        Ok(_) => {
            bail!(
                "refusing to replace existing {}; remove or repair it explicitly",
                link.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("inspect {}", link.display())),
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)
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
    curl_json_request(url, Some(payload.to_string()))
}

fn curl_get_json(url: &str) -> Result<Value> {
    curl_json_request(url, None)
}

fn curl_json_request(url: &str, body: Option<String>) -> Result<Value> {
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
    ];
    if let Some(body) = body {
        args.extend([
            OsString::from("--request"),
            OsString::from("POST"),
            OsString::from("--header"),
            OsString::from("Content-Type: application/json"),
            OsString::from("--data"),
            OsString::from(body),
        ]);
    }
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

#[cfg(test)]
mod flavor_tests {
    use super::{quipu_install_args, require_compiled_feature};
    use crate::model::QuipuFlavor;
    use serde_json::json;

    #[test]
    fn install_args_add_only_the_lancedb_feature_to_the_reviewed_pin() {
        let release = quipu_install_args(QuipuFlavor::Release);
        let lancedb = quipu_install_args(QuipuFlavor::Lancedb);
        assert_eq!(release[6], "full");
        assert_eq!(lancedb[6], "full,lancedb");
        // Everything but the feature list must be identical, so the lancedb
        // flavor stays pinned to the same reviewed revision.
        assert_eq!(release[..6], lancedb[..6]);
        assert_eq!(release[7..], lancedb[7..]);
        assert!(release.contains(&"--locked"));
    }

    #[test]
    fn compile_map_proof_requires_a_true_lancedb_entry() {
        require_compiled_feature(&json!({"features": {"lancedb": true}}), "lancedb").unwrap();

        let absent = require_compiled_feature(&json!({"features": {"onnx": true}}), "lancedb")
            .unwrap_err()
            .to_string();
        assert!(absent.contains("compiled without it"), "{absent}");

        let disabled =
            require_compiled_feature(&json!({"features": {"lancedb": false}}), "lancedb")
                .unwrap_err()
                .to_string();
        assert!(disabled.contains("compiled without it"), "{disabled}");

        // A server too old to report the map proves nothing; that is a
        // refusal, not a pass.
        let unmapped = require_compiled_feature(&json!({"version": "0.3.27"}), "lancedb")
            .unwrap_err()
            .to_string();
        assert!(
            unmapped.contains("no per-feature compile map"),
            "{unmapped}"
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::ensure_bobbin_link;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn bobbin_link_accepts_its_restored_cache_entry_but_refuses_foreign_paths() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("release/bobbin");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "binary fixture").unwrap();

        let link = root.path().join("bin/bobbin");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&target, &link).unwrap();
        ensure_bobbin_link(&link, &target).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), target);

        let foreign = root.path().join("foreign");
        fs::write(&foreign, "do not replace").unwrap();
        fs::remove_file(&link).unwrap();
        symlink(&foreign, &link).unwrap();
        let error = ensure_bobbin_link(&link, &target).unwrap_err().to_string();
        assert!(error.contains("refusing to replace existing"));
        assert_eq!(fs::read_link(&link).unwrap(), foreign);
    }
}
