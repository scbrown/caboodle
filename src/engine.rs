use std::{ffi::OsString, path::Path};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::{
    adapter::adapter,
    configuration,
    crew::{self, CrewEvidence},
    embedding, emission,
    model::{ModelArtifactState, Plan, ShareState, State, ToolState},
};

#[derive(Deserialize)]
struct ImportPromotion {
    eligible: bool,
    blockers: Vec<String>,
}

#[derive(Deserialize)]
struct ImportResult {
    outcome: String,
    share_id: String,
    staging_graph: String,
    promotion: ImportPromotion,
}

/// What `apply` should do about an installed tool whose version is not the one
/// this Caboodle build reviewed (aegis-5ctwu3).
///
/// Split out as a pure function ON PURPOSE: the act it decides is a network
/// download, so a test that drove `apply` end to end would be measuring GitHub
/// rather than the decision. Every branch below is covered by a unit test; the
/// branch that could not be, would not have been.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Convergence {
    /// Installed IS the reviewed release. Nothing to do.
    Current,
    /// Installed is strictly older. Converge upward, then prove by read-back.
    Converge,
    /// Installed is at or ahead of the reviewed release, or the two versions
    /// are not comparable. Do NOT install — that would be a downgrade or a
    /// replacement on a guess (aegis-48dvl3). Say so instead.
    Decline,
    /// Skewed, and the operator asked for no installs. A MISSING tool already
    /// fails under `--skip-install`; a skewed one reporting "applied" was the
    /// inconsistency this bead is about.
    RefuseSkipInstall,
}

pub(crate) fn decide_convergence(
    installed: &str,
    desired: &str,
    is_current: bool,
    skip_install: bool,
) -> Convergence {
    if is_current {
        return Convergence::Current;
    }
    if skip_install {
        return Convergence::RefuseSkipInstall;
    }
    match crate::release_update::behind_reviewed(installed, desired) {
        Some(true) => Convergence::Converge,
        // `None` is "cannot compare", NOT "stale". Treating it as stale is how
        // an unrecognised build gets replaced on a guess.
        Some(false) | None => Convergence::Decline,
    }
}

pub fn apply(plan: &Plan, state_path: &Path, skip_install: bool) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
        let desired = adapter.desired_version();
        // aegis-5ctwu3: PRESENCE IS NOT CONVERGENCE. Until this match looked at
        // `is_current`, a binary that merely EXISTED was reported "applied" and
        // left at whatever version it was — bobbin sat at 0.1.0 while this build
        // reviewed 0.17.0, and `caboodle install` then failed its own verify with
        // `error: unexpected argument '--source' found`. The installer knew the
        // newer contract, verified against it, and had never installed it; the
        // operator got a clap error instead of "your bobbin is 16 minors old".
        // `st doctor` had it right the whole time ("bobbin 0.1.0 installed —
        // 0.17.0 available (STALE)"), which is the tell that this was never a
        // detection problem.
        let version = match adapter.version() {
            Ok(installed) if adapter.is_current(&installed) => installed,

            // Known-wrong under --skip-install. The flag means "do not install",
            // not "call it applied anyway", and a MISSING tool already fails
            // here — a SKEWED one reporting success was the inconsistency.
            Ok(installed) => {
                match decide_convergence(&installed, &desired, false, skip_install) {
                    Convergence::Current => installed,
                    Convergence::RefuseSkipInstall => bail!(
                        "{} is installed at {installed} but this Caboodle build reviewed {desired}; \
                         --skip-install refuses to converge it. Install {desired}, or drop \
                         --skip-install to let apply converge it.",
                        name.as_str()
                    ),
                    Convergence::Decline => {
                        println!(
                            "{}: NOT converged — installed {installed}, reviewed {desired}. \
                             Apply does not downgrade or replace an unrecognised version; \
                             use `caboodle update-release` deliberately if that is what you want.",
                            name.as_str()
                        );
                        installed
                    }
                    Convergence::Converge => {
                        eprintln!(
                            "{}: stale ({installed}); converging to reviewed {desired}",
                            name.as_str()
                        );
                        adapter.install().with_context(|| {
                            format!("{} convergence to reviewed {desired}", name.as_str())
                        })?;
                        let after = adapter.version().with_context(|| {
                            format!("{} version read-back after convergence", name.as_str())
                        })?;
                        // The read-back is the proof, not the install's exit code.
                        if !adapter.is_current(&after) {
                            bail!(
                                "{} did not reach reviewed release {desired} (got {after}); \
                                 refusing to record it as applied",
                                name.as_str()
                            );
                        }
                        println!("{}: converged {installed} -> {after}", name.as_str());
                        after
                    }
                }
            }

            Err(error) if !skip_install => {
                eprintln!("{}: not installed ({error:#}); installing", name.as_str());
                adapter
                    .install()
                    .with_context(|| {
                        format!(
                            "{} install step (run `caboodle doctor` to list every blocker on this host)",
                            name.as_str()
                        )
                    })?;
                adapter
                    .version()
                    .with_context(|| format!("{} version read-back after install", name.as_str()))?
            }
            Err(error) => {
                return Err(error).context(format!("{} version read-back", name.as_str()))
            }
        };
        let remains_verified = state
            .tools
            .get(name.as_str())
            .is_some_and(|previous| previous.version == version && previous.verified);
        state.tools.insert(
            name.as_str().to_owned(),
            ToolState {
                version,
                applied: true,
                verified: remains_verified,
            },
        );
        state.write(state_path)?;
        emission::queue_transition(
            state_path,
            name.as_str(),
            "applied",
            &state.tools[name.as_str()].version,
        )?;
        println!("{}: applied", name.as_str());
    }
    if let Some(config) = &plan.stack_config {
        configuration::apply(config)?;
        println!("stack configuration: applied");
    }
    if let Some(model) = &plan.embedding_model {
        let provisioned = embedding::provision(model, crate::adapter::download_https)
            .context("embedding-model provisioning step")?;
        for artifact in provisioned {
            // Like tools, a re-fetch of identical pinned bytes keeps its
            // verified mark; anything else must earn it again.
            let remains_verified = state
                .models
                .get(&artifact.name)
                .is_some_and(|previous| previous.sha256 == artifact.sha256 && previous.verified);
            state.models.insert(
                artifact.name.clone(),
                ModelArtifactState {
                    path: artifact.path.clone(),
                    sha256: artifact.sha256.clone(),
                    provisioned: true,
                    verified: remains_verified,
                },
            );
            state.write(state_path)?;
            emission::queue_transition(
                state_path,
                &format!("embedding-model/{}", artifact.name),
                "applied",
                &artifact.sha256,
            )?;
            let outcome = match artifact.outcome {
                embedding::ArtifactOutcome::Fetched => "provisioned",
                embedding::ArtifactOutcome::Current => "current",
            };
            println!("embedding-model {}: {outcome}", artifact.name);
        }
    }
    if let Some(selection) = &plan.crew {
        crew::apply(selection, &mut state, skip_install)?;
        state.write(state_path)?;
        for (name, runtime) in &state.crew {
            emission::queue_transition(state_path, name, "applied", &runtime.version)?;
        }
    }
    consume_shares(plan, &mut state, state_path)?;
    Ok(state)
}

fn consume_shares(plan: &Plan, state: &mut State, state_path: &Path) -> Result<()> {
    let Some(db) = plan.quipu_db.as_deref() else {
        return Ok(());
    };
    for share in &plan.shares {
        let result = crate::adapter::checked(
            "quipu",
            [
                OsString::from("import"),
                share.as_os_str().to_owned(),
                OsString::from("--db"),
                db.as_os_str().to_owned(),
            ],
            None,
        )
        .with_context(|| format!("import canonical Quipu share {}", share.display()))?;
        let imported: ImportResult = serde_json::from_slice(&result.stdout)
            .with_context(|| format!("parse Quipu import result for {}", share.display()))?;
        if !matches!(
            imported.outcome.as_str(),
            "staged" | "quarantined" | "unchanged"
        ) {
            bail!(
                "Quipu returned unknown share import outcome {:?} for {}",
                imported.outcome,
                share.display()
            );
        }
        state.shares.insert(
            imported.share_id.clone(),
            ShareState {
                path: share.clone(),
                staging_graph: imported.staging_graph,
                outcome: imported.outcome.clone(),
                promotion_eligible: imported.promotion.eligible,
                blockers: imported.promotion.blockers,
            },
        );
        state.write(state_path)?;
        println!(
            "share {}: {} (promotion eligible: {})",
            imported.share_id, imported.outcome, imported.promotion.eligible
        );
    }
    Ok(())
}

/// Refuse to call a tool verified when the copy PATH runs is not the copy that
/// was verified (aegis-70qlhs). A different file with the same version is a
/// note, not a failure: it runs the same build.
fn check_path_resolution(name: crate::model::ToolName) -> Result<()> {
    let path = std::env::var_os("PATH");
    for &program in crate::adapter::programs(name) {
        match crate::adapter::path_resolution(program, path.as_deref()) {
            crate::adapter::PathResolution::Managed => {}
            crate::adapter::PathResolution::NotOnPath => println!(
                "{}: note: `{program}` is not on PATH; add the install directory to PATH to run it",
                name.as_str()
            ),
            crate::adapter::PathResolution::SameBuild { runs, managed } => println!(
                "{}: note: PATH runs {} rather than {}, but both report the same version",
                name.as_str(),
                runs.display(),
                managed.display()
            ),
            crate::adapter::PathResolution::Shadowed {
                runs,
                runs_version,
                managed,
                managed_version,
            } => anyhow::bail!(
                "{program} is SHADOWED: PATH runs {} ({}), not the verified {} ({}). \
                 Remove the stale copy or put {} earlier on PATH",
                runs.display(),
                runs_version.trim(),
                managed.display(),
                managed_version.trim(),
                managed
                    .parent()
                    .map(|d| d.display().to_string())
                    .unwrap_or_default()
            ),
        }
    }
    Ok(())
}

pub fn verify(plan: &Plan, state_path: &Path, evidence: &CrewEvidence) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
        let version = adapter
            .version()
            .with_context(|| format!("{} version read-back", name.as_str()))?;
        // aegis-5ctwu3 item 3. A verification failure on a SKEWED tool used to
        // surface the tool's own complaint — `error: unexpected argument
        // '--source' found` — which points at an argument, not at the cause. We
        // verify against the contract of the release we reviewed, so when the
        // installed version is not that release, the skew IS the finding and
        // must be said first.
        adapter.verify().with_context(|| {
            let desired = adapter.desired_version();
            if adapter.is_current(&version) {
                format!("{} functional verification", name.as_str())
            } else {
                format!(
                    "{} functional verification — VERSION SKEW: {version} is installed, \
                     this Caboodle build reviewed and verifies against {desired}. The error \
                     below is most likely that skew, not a broken tool. Converge with \
                     `caboodle apply` (or `caboodle update`), then re-verify.",
                    name.as_str()
                )
            }
        })?;
        check_path_resolution(name)?;
        state.tools.insert(
            name.as_str().to_owned(),
            ToolState {
                version,
                applied: true,
                verified: true,
            },
        );
        state.write(state_path)?;
        emission::queue_transition(
            state_path,
            name.as_str(),
            "verified",
            &state.tools[name.as_str()].version,
        )?;
        println!("{}: verified", name.as_str());
    }
    if let Some(model) = &plan.embedding_model {
        // This is deliberately a re-hash, not an embed round-trip; see
        // embedding::verify for why claiming more would be a banner.
        embedding::verify(model).context("embedding-model artifact re-hash step")?;
        for artifact in &model.artifacts {
            state.models.insert(
                artifact.name.clone(),
                ModelArtifactState {
                    path: model.destination.join(&artifact.name),
                    sha256: artifact.sha256.clone(),
                    provisioned: true,
                    verified: true,
                },
            );
            state.write(state_path)?;
            emission::queue_transition(
                state_path,
                &format!("embedding-model/{}", artifact.name),
                "verified",
                &artifact.sha256,
            )?;
            println!("embedding-model {}: digest re-checked", artifact.name);
        }
    }
    if let Some(selection) = &plan.crew {
        crew::verify(selection, evidence, &mut state)?;
        state.write(state_path)?;
        for (name, runtime) in &state.crew {
            emission::queue_transition(state_path, name, "verified", &runtime.version)?;
        }
    }
    Ok(state)
}

/// Compare the running toolchain to the releases reviewed by this Caboodle
/// build. Returns true only when every selected tool is current.
pub fn check_updates(plan: &Plan) -> Result<bool> {
    plan.validate()?;
    let mut current = true;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
        let desired = adapter.desired_version();
        match adapter.version() {
            Ok(installed) if adapter.is_current(&installed) => {
                println!("{}: current ({installed})", name.as_str());
            }
            Ok(installed) => {
                current = false;
                println!(
                    "{}: update available (installed: {installed}; reviewed: {desired})",
                    name.as_str()
                );
            }
            Err(error) => {
                current = false;
                println!(
                    "{}: missing or unreadable ({error:#}); reviewed: {desired}",
                    name.as_str()
                );
            }
        }
    }
    if let Some(selection) = &plan.crew {
        current &= crew::check_updates(selection);
    }
    Ok(current)
}

/// Converge drifted tools to the reviewed releases and functionally verify
/// each changed tool before recording it as current.
pub fn update(plan: &Plan, state_path: &Path, evidence: &CrewEvidence) -> Result<State> {
    plan.validate()?;
    let mut state = State::read(state_path)?;
    for &name in &plan.tools {
        let adapter = adapter(name, plan.quipu_flavor);
        let before = adapter.version().ok();
        if before
            .as_deref()
            .is_some_and(|installed| adapter.is_current(installed))
        {
            println!("{}: current", name.as_str());
            continue;
        }
        #[cfg(unix)]
        crate::release_update::guard_reviewed_update(name, &adapter.desired_version())?;
        eprintln!(
            "{}: converging {:?} -> {}",
            name.as_str(),
            before,
            adapter.desired_version()
        );
        adapter
            .install()
            .with_context(|| format!("{} reviewed update install", name.as_str()))?;
        let version = adapter
            .version()
            .with_context(|| format!("{} version read-back after update", name.as_str()))?;
        if !adapter.is_current(&version) {
            bail!(
                "{} update did not reach reviewed release {} (got {})",
                name.as_str(),
                adapter.desired_version(),
                version
            );
        }
        adapter
            .verify()
            .with_context(|| format!("{} verification after update", name.as_str()))?;
        state.tools.insert(
            name.as_str().to_owned(),
            ToolState {
                version: version.clone(),
                applied: true,
                verified: true,
            },
        );
        state.write(state_path)?;
        emission::queue_transition(state_path, name.as_str(), "updated", &version)?;
        println!("{}: updated and verified", name.as_str());
    }
    if let Some(selection) = &plan.crew {
        if !crew::check_updates(selection) {
            crew::apply(selection, &mut state, false)?;
            crew::verify(selection, evidence, &mut state)?;
            state.write(state_path)?;
            for (name, runtime) in &state.crew {
                emission::queue_transition(state_path, name, "updated", &runtime.version)?;
            }
        }
    }
    Ok(state)
}

pub fn verify_questions(plan: &Plan, db: Option<&Path>) -> Result<()> {
    plan.validate()?;
    let intent = plan.intent.as_ref().context(
        "plan has no intended-use/question contract; regenerate it through the Phase 2 interview",
    )?;
    for (index, contract) in intent.anticipated_questions.iter().enumerate() {
        let mut args = vec![OsString::from("read"), OsString::from(&contract.sparql)];
        if let Some(db) = db {
            args.push(OsString::from("--db"));
            args.push(db.as_os_str().to_owned());
        }
        let result = crate::adapter::checked("quipu", args, None)
            .with_context(|| format!("anticipated question {} query", index + 1))?;
        let answer = String::from_utf8_lossy(&result.stdout);
        if !answer.contains(&contract.expected) {
            anyhow::bail!(
                "anticipated question {} was executable but its answer did not contain {:?}: {}",
                index + 1,
                contract.expected,
                contract.question
            );
        }
        println!("question {}: verified — {}", index + 1, contract.question);
    }
    Ok(())
}

#[cfg(test)]
mod convergence_tests {
    use super::{decide_convergence, Convergence};

    const REVIEWED: &str = "bobbin 0.16.2";

    #[test]
    fn current_is_left_alone_even_under_skip_install() {
        // Unreachable from apply today (is_current is matched first), which is why
        // this branch is covered here rather than deleted as apparently dead.
        assert_eq!(
            decide_convergence(REVIEWED, REVIEWED, true, true),
            Convergence::Current
        );
        assert_eq!(
            decide_convergence(REVIEWED, REVIEWED, true, false),
            Convergence::Current
        );
    }

    #[test]
    fn stale_converges_and_skip_install_refuses_instead_of_claiming_applied() {
        assert_eq!(
            decide_convergence("bobbin 0.1.0", REVIEWED, false, false),
            Convergence::Converge
        );
        assert_eq!(
            decide_convergence("bobbin 0.1.0", REVIEWED, false, true),
            Convergence::RefuseSkipInstall
        );
    }

    #[test]
    fn ahead_or_incomparable_is_declined_not_replaced() {
        assert_eq!(
            decide_convergence("bobbin 0.99.0", REVIEWED, false, false),
            Convergence::Decline
        );
        assert_eq!(
            decide_convergence("bobbin dev", REVIEWED, false, false),
            Convergence::Decline
        );
    }
}
