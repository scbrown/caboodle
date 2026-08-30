use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::{
    adapter::{checked, download_https},
    model::{CrewMode, CrewRuntimeState, CrewSelection, State},
};

const SHANTYTOWN_VERSION: &str = "0.4.0";
const SHANTYTOWN_WHEEL_SHA256: &str =
    "afd52cb5e2b8c67eef8fa7e3aa4e8c725419d0f1f3e84184d6a01b52134ca8ac";
const CREEL_REVISION: &str = "57606dcfa0ff72d6c1bb083d70644c9926b181eb";
const CREEL_ARCHIVE_SHA256: &str =
    "22dea10d41e45ab89d6c4e4d2421d38d35369563cc1f3fb26daa4ecc6c2b3aea";

#[derive(Debug, Default)]
pub struct CrewEvidence {
    pub creel_doctor: Option<PathBuf>,
    pub creel_admission: Option<PathBuf>,
}

pub fn apply(selection: &CrewSelection, state: &mut State, skip_install: bool) -> Result<()> {
    if selects_shantytown(selection.mode) {
        if skip_install && shantytown_version().is_err() {
            bail!("Shantytown is not installed and --skip-install was selected");
        }
        install_shantytown()?;
        record(state, "shantytown", shantytown_version()?, false);
        println!("shantytown: applied");
    }
    if selects_creel(selection.mode) {
        if skip_install && !creel_root()?.join("REVISION").is_file() {
            bail!("Creel is not installed and --skip-install was selected");
        }
        install_creel()?;
        record(state, "creel", format!("creel {CREEL_REVISION}"), false);
        println!("creel: applied");
    }
    Ok(())
}

pub fn verify(selection: &CrewSelection, evidence: &CrewEvidence, state: &mut State) -> Result<()> {
    if selects_shantytown(selection.mode) {
        let version = shantytown_version()?;
        record(state, "shantytown", version, true);
        println!("shantytown: verified");
    }
    if selects_creel(selection.mode) {
        verify_creel_bundle()?;
        let doctor = evidence.creel_doctor.as_deref().context(
            "Creel verification requires --creel-doctor from the browser capability preflight",
        )?;
        let admission = evidence.creel_admission.as_deref().context(
            "Creel verification requires --creel-admission from the provider-window governor",
        )?;
        validate_doctor(doctor)?;
        validate_admission(admission)?;
        record(state, "creel", format!("creel {CREEL_REVISION}"), true);
        println!("creel: verified");
    }
    Ok(())
}

pub fn check_updates(selection: &CrewSelection) -> bool {
    let mut current = true;
    if selects_shantytown(selection.mode) {
        match shantytown_version() {
            Ok(version) => println!("shantytown: current ({version})"),
            Err(error) => {
                current = false;
                println!(
                    "shantytown: missing or drifted ({error:#}); reviewed: st {SHANTYTOWN_VERSION}"
                );
            }
        }
    }
    if selects_creel(selection.mode) {
        match verify_creel_bundle() {
            Ok(()) => println!("creel: current (creel {CREEL_REVISION})"),
            Err(error) => {
                current = false;
                println!("creel: missing or drifted ({error:#}); reviewed: creel {CREEL_REVISION}");
            }
        }
    }
    current
}

fn selects_shantytown(mode: CrewMode) -> bool {
    matches!(mode, CrewMode::Shantytown | CrewMode::Both)
}

fn selects_creel(mode: CrewMode) -> bool {
    matches!(mode, CrewMode::Creel | CrewMode::Both)
}

fn record(state: &mut State, name: &str, version: String, verified: bool) {
    let remains_verified = state
        .crew
        .get(name)
        .is_some_and(|old| old.version == version && old.verified);
    state.crew.insert(
        name.to_owned(),
        CrewRuntimeState {
            version,
            applied: true,
            verified: verified || remains_verified,
        },
    );
}

fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is required to install crew runtimes")
}

fn install_shantytown() -> Result<()> {
    if shantytown_version().is_ok() {
        return Ok(());
    }
    let download = tempfile::tempdir().context("create Shantytown download directory")?;
    let wheel = download
        .path()
        .join(format!("shantytown-{SHANTYTOWN_VERSION}-py3-none-any.whl"));
    download_https(
        &format!(
            "https://github.com/scbrown/shantytown/releases/download/v{SHANTYTOWN_VERSION}/shantytown-{SHANTYTOWN_VERSION}-py3-none-any.whl"
        ),
        &wheel,
    )?;
    verify_fixed_checksum(&wheel, SHANTYTOWN_WHEEL_SHA256, "Shantytown wheel")?;
    let root = home()?
        .join(".local/share/caboodle/shantytown")
        .join(format!("v{SHANTYTOWN_VERSION}"));
    if !root.join("bin/python").exists() {
        fs::create_dir_all(root.parent().context("Shantytown root has no parent")?)?;
        checked(
            "python3",
            [OsStr::new("-m"), OsStr::new("venv"), root.as_os_str()],
            None,
        )?;
        checked(
            root.join("bin/pip")
                .to_str()
                .context("Shantytown pip path is not UTF-8")?,
            [
                OsStr::new("install"),
                OsStr::new("--no-deps"),
                wheel.as_os_str(),
            ],
            None,
        )?;
    }
    link_binary(&root.join("bin/st"), "st")
}

fn shantytown_version() -> Result<String> {
    let output = checked("st", ["--version"], None)?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !version.starts_with(&format!("st {SHANTYTOWN_VERSION}")) {
        bail!("Shantytown version is incompatible: {version}");
    }
    Ok(version)
}

fn install_creel() -> Result<()> {
    let root = creel_root()?;
    if root.join("REVISION").exists() {
        return Ok(());
    }
    let download = tempfile::tempdir().context("create Creel download directory")?;
    let archive = download.path().join("creel.tar.gz");
    download_https(
        &format!("https://github.com/scbrown/creel/archive/{CREEL_REVISION}.tar.gz"),
        &archive,
    )?;
    verify_fixed_checksum(&archive, CREEL_ARCHIVE_SHA256, "Creel archive")?;
    fs::create_dir_all(&root)?;
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
    fs::write(root.join("REVISION"), format!("{CREEL_REVISION}\n"))?;
    Ok(())
}

fn creel_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CABOODLE_CREEL_ROOT") {
        return Ok(PathBuf::from(path));
    }
    Ok(home()?
        .join(".local/share/caboodle/creel")
        .join(CREEL_REVISION))
}

fn verify_creel_bundle() -> Result<()> {
    let root = creel_root()?;
    let revision = fs::read_to_string(root.join("REVISION")).context("read Creel revision")?;
    if revision.trim() != CREEL_REVISION {
        bail!("installed Creel revision does not match the reviewed bundle");
    }
    for path in [
        "app/index.html",
        "app/sw.js",
        "app/wasm/pkg/creel_quipu_provider_bg.wasm",
    ] {
        if !root.join(path).is_file() {
            bail!("installed Creel bundle is missing {path}");
        }
    }
    Ok(())
}

fn verify_fixed_checksum(path: &Path, expected: &str, label: &str) -> Result<()> {
    let output = checked("sha256sum", [path.as_os_str()], None)?;
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .context("sha256sum returned no digest")?
        .to_owned();
    if actual != expected {
        bail!("{label} checksum mismatch");
    }
    Ok(())
}

fn link_binary(target: &Path, name: &str) -> Result<()> {
    let bin_dir = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or(home()?.join(".cargo"))
        .join("bin");
    fs::create_dir_all(&bin_dir)?;
    let link = bin_dir.join(name);
    if link.exists() || link.symlink_metadata().is_ok() {
        bail!("refusing to replace existing {}", link.display());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, &link)
        .with_context(|| format!("link {name} into {}", bin_dir.display()))?;
    #[cfg(not(unix))]
    bail!("crew runtime installation currently requires a Unix host");
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DoctorContract {
    schema_version: u32,
    overall: CheckStatus,
    checks: Vec<DoctorCheck>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DoctorCheck {
    id: String,
    status: CheckStatus,
    severity: Severity,
    evidence: String,
    remediation: String,
    redacted: bool,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Required,
    Advisory,
}

fn validate_doctor(path: &Path) -> Result<()> {
    let contract: DoctorContract = read_json(path, "Creel doctor")?;
    if contract.schema_version != 1 {
        bail!(
            "unsupported Creel doctor schema {}",
            contract.schema_version
        );
    }
    if contract.checks.is_empty() {
        bail!("Creel doctor returned no checks");
    }
    let mut ids = HashSet::new();
    for check in &contract.checks {
        if check.id.trim().is_empty() || !ids.insert(&check.id) {
            bail!("Creel doctor check IDs must be non-empty and unique");
        }
        if !check.redacted
            || check.evidence.trim().is_empty()
            || check.remediation.trim().is_empty()
        {
            bail!(
                "Creel doctor check {} lacks redacted evidence/remediation",
                check.id
            );
        }
        if check.severity == Severity::Required && check.status != CheckStatus::Pass {
            bail!("Creel required doctor check {} did not pass", check.id);
        }
    }
    if contract.overall != CheckStatus::Pass {
        bail!("Creel doctor aggregate is not pass");
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionContract {
    schema_version: u32,
    verdict: AdmissionVerdict,
    provider_window: AdmissionSignal,
    device_tab_cap: AdmissionSignal,
    signal_freshness: AdmissionSignal,
    reason: String,
    redacted: bool,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum AdmissionVerdict {
    Admit,
    Refuse,
    Unknown,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionSignal {
    status: CheckStatus,
    evidence: String,
}

fn validate_admission(path: &Path) -> Result<()> {
    let contract: AdmissionContract = read_json(path, "Creel admission")?;
    if contract.schema_version != 1 {
        bail!(
            "unsupported Creel admission schema {}",
            contract.schema_version
        );
    }
    if !contract.redacted || contract.reason.trim().is_empty() {
        bail!("Creel admission reason must be present and redacted");
    }
    for (name, signal) in [
        ("provider_window", &contract.provider_window),
        ("device_tab_cap", &contract.device_tab_cap),
        ("signal_freshness", &contract.signal_freshness),
    ] {
        if signal.status != CheckStatus::Pass || signal.evidence.trim().is_empty() {
            bail!("Creel admission signal {name} is not a measured pass");
        }
    }
    if contract.verdict != AdmissionVerdict::Admit {
        bail!("Creel governor did not admit this launch");
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T> {
    let body = fs::read_to_string(path).with_context(|| format!("read {label} contract"))?;
    serde_json::from_str(&body).with_context(|| format!("parse {label} contract"))
}
