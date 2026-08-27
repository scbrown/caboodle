use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};

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

fn checked<I, S>(program: &str, args: I, cwd: Option<&Path>) -> Result<Output>
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

struct Quipu;

impl Adapter for Quipu {
    fn name(&self) -> ToolName {
        ToolName::Quipu
    }

    fn install(&self) -> Result<()> {
        checked(
            "cargo",
            ["install", "quipu-ai", "--version", "0.3.27", "--locked"],
            None,
        )?;
        Ok(())
    }

    fn version(&self) -> Result<String> {
        read_version("quipu")
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

fn download_https(url: &str, destination: &Path) -> Result<()> {
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
