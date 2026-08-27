#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

fn fake_tool(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn path_with(bin: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&existing)))
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn command(root: &Path, bin: &Path) -> Command {
    let mut command = Command::cargo_bin("caboodle").unwrap();
    command
        .current_dir(root)
        .env("PATH", path_with(bin))
        .env("CABOODLE_CAMAYOC_ROOT", root.join("camayoc"))
        .env("QUIPU_SERVER", "http://quipu.test")
        .env("FAKE_CAMAYOC_STATE", root.join("camayoc-ingested"));
    command
}

fn install_fakes(root: &Path, bin: &Path) {
    fake_tool(
        bin,
        "quipu",
        r#"
if [ "${1:-}" = "--version" ]; then echo 'quipu 0.3.27'; exit 0; fi
if [ "${1:-}" = "episode" ]; then
  db=''
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--db" ]; then shift; db=$1; fi
    shift || true
  done
  touch "$db"
  exit 0
fi
if [ "${1:-}" = "read" ]; then
  db=''
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--db" ]; then shift; db=$1; fi
    shift || true
  done
  [ ! -f "$db" ] || echo 'caboodle-verify-roundtrip'
  exit 0
fi
exit 2
"#,
    );
    fake_tool(
        bin,
        "quipu-server",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu-server 0.3.27'; exit 0; fi\nexit 2",
    );
    fake_tool(
        bin,
        "bobbin",
        r#"
if [ "${1:-}" = "--version" ]; then echo 'bobbin 0.test'; exit 0; fi
if [ "${1:-}" = "init" ]; then mkdir -p .bobbin; exit 0; fi
if [ "${1:-}" = "index" ]; then
  if [ -f fixture.rs ]; then cp fixture.rs .bobbin/indexed; else : > .bobbin/indexed; fi
  exit 0
fi
if [ "${1:-}" = "grep" ]; then
  if grep -q caboodle_verify_marker_ .bobbin/indexed 2>/dev/null; then echo '{"count":1,"results":[{"file_path":"fixture.rs"}]}';
  else echo '{"count":0,"results":[]}'; fi
  exit 0
fi
exit 2
"#,
    );
    fake_tool(
        bin,
        "curl",
        r#"
args=$*
case "$args" in
  *"/query"*)
    case "$args" in
      *caboodle-camayoc-control-must-stay-absent*)
        if [ "${FAKE_CAMAYOC_MODE:-}" = control-present ]; then echo '{"count":1,"rows":[{"s":"bad-control"}]}'
        else echo '{"count":0,"rows":[]}'; fi
        ;;
      *)
        if [ "${FAKE_CAMAYOC_MODE:-}" = not-retrievable ]; then echo '{"count":0,"rows":[]}'
        elif [ -f "$FAKE_CAMAYOC_STATE.duplicate" ]; then echo '{"count":2,"rows":[{"s":"marker"},{"s":"marker"}]}'
        elif [ -f "$FAKE_CAMAYOC_STATE" ]; then echo '{"count":1,"rows":[{"s":"marker"}]}'
        else echo '{"count":0,"rows":[]}'; fi
        ;;
    esac
    ;;
  *"/knot"*)
    if [ "${FAKE_CAMAYOC_MODE:-}" = duplicate-replay ] && [ -f "$FAKE_CAMAYOC_STATE" ]; then touch "$FAKE_CAMAYOC_STATE.duplicate"; echo '{"count":4,"tx_id":2}'
    elif [ "${FAKE_CAMAYOC_MODE:-}" = duplicate-replay ]; then touch "$FAKE_CAMAYOC_STATE"; echo '{"count":4,"tx_id":1}'
    elif [ -f "$FAKE_CAMAYOC_STATE" ]; then echo '{"count":0,"tx_id":0}'
    else touch "$FAKE_CAMAYOC_STATE"; echo '{"count":4,"tx_id":1}'; fi
    ;;
  *) exit 2 ;;
esac
"#,
    );
    let camayoc = root.join("camayoc");
    fs::create_dir_all(camayoc.join("scripts")).unwrap();
    fs::create_dir_all(camayoc.join("ontology")).unwrap();
    fs::write(camayoc.join("REVISION"), "test-revision\n").unwrap();
    fs::write(camayoc.join("scripts/bootstrap.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::write(
        camayoc.join("ontology/core.ttl"),
        "@prefix aegis: <https://example.test/ontology/> .\n",
    )
    .unwrap();
}

#[test]
fn guided_interview_writes_the_same_reviewed_plan_as_plan_command() {
    let guided_root = tempfile::tempdir().unwrap();
    command(guided_root.path(), guided_root.path())
        .args(["init", "--guided"])
        .write_stdin("crew\nboth\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("profile [kg/retrieval/crew]"))
        .stdout(predicate::str::contains(
            "crew [shantytown/creel/both/standalone]",
        ));

    let direct_root = tempfile::tempdir().unwrap();
    command(direct_root.path(), direct_root.path())
        .args(["plan", "--profile", "crew", "--crew", "both"])
        .assert()
        .success();

    assert_eq!(
        fs::read(guided_root.path().join("caboodle-plan.toml")).unwrap(),
        fs::read(direct_root.path().join("caboodle-plan.toml")).unwrap()
    );
    assert!(!guided_root.path().join(".caboodle/interview.toml").exists());
}

#[test]
fn guided_interview_resumes_after_input_ends() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["init", "--guided"])
        .write_stdin("crew\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("interview paused at end of input"));

    let draft = fs::read_to_string(root.path().join(".caboodle/interview.toml")).unwrap();
    assert!(draft.contains("profile = \"crew\""));

    command(root.path(), root.path())
        .args(["init", "--guided"])
        .write_stdin("creel\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "resuming: profile already answered",
        ));
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("mode = \"creel\""));
}

#[test]
fn guided_interview_rejects_invalid_answers_without_a_plan() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["init", "--guided"])
        .write_stdin("everything\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid profile"));
    assert!(!root.path().join("caboodle-plan.toml").exists());
}

#[test]
fn plan_install_verify_is_resumable() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);

    command(root.path(), &bin)
        .args(["plan", "--profile", "retrieval"])
        .assert()
        .success()
        .stdout(predicate::str::contains("caboodle-plan.toml"));

    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu: verified"))
        .stdout(predicate::str::contains("bobbin: verified"));

    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success();

    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["tools"]["quipu"]["verified"], true);
    assert_eq!(state["tools"]["bobbin"]["verified"], true);
}

#[test]
fn camayoc_verification_refuses_broken_control_retrieval_and_replay() {
    for (mode, message) in [
        ("control-present", "negative control unexpectedly exists"),
        ("not-retrievable", "first ingest was not retrievable"),
        ("duplicate-replay", "idempotent replay wrote duplicate"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        install_fakes(root.path(), &bin);
        command(root.path(), &bin)
            .args(["plan", "--profile", "kg"])
            .assert()
            .success();
        command(root.path(), &bin)
            .arg("verify")
            .env("FAKE_CAMAYOC_MODE", mode)
            .assert()
            .failure()
            .stderr(predicate::str::contains(message));
    }
}

#[test]
fn verify_names_the_failing_adapter() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    fake_tool(
        &bin,
        "quipu",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu 0.3.27'; exit 0; fi\nexit 9",
    );

    command(root.path(), &bin)
        .args(["plan", "--profile", "kg"])
        .assert()
        .success();
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("quipu functional verification"));
}

#[test]
fn apply_refuses_a_quipu_too_old_for_camayoc() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    fake_tool(
        &bin,
        "quipu",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu 0.3.7'; exit 0; fi\nexit 9",
    );
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg"])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires at least 0.3.27"));
}

#[test]
fn crew_profile_records_each_runtime_choice() {
    for mode in ["shantytown", "creel", "both", "standalone"] {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join(format!("{mode}.toml"));
        command(root.path(), root.path())
            .args([
                "plan",
                "--profile",
                "crew",
                "--crew",
                mode,
                "--output",
                output.to_str().unwrap(),
            ])
            .assert()
            .success();
        let body = fs::read_to_string(output).unwrap();
        assert!(body.contains(&format!("mode = \"{mode}\"")));
    }
}

#[test]
fn both_mode_names_owners_and_explicit_handoff() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["plan", "--profile", "crew", "--crew", "both"])
        .assert()
        .success();

    let body = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(body.contains("durable_owner = \"shantytown\""));
    assert!(body.contains("burst_owner = \"creel\""));
    assert!(body.contains("routing = \"explicit-handoff\""));
}

#[test]
fn plan_rejects_unknown_crew_mode_and_invalid_both_contract() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["plan", "--profile", "crew", "--crew", "unknown"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'unknown'"));

    fs::write(
        root.path().join("invalid.toml"),
        r#"schema_version = 1
profile = "crew"
tools = ["quipu", "camayoc", "bobbin"]

[crew]
mode = "both"
durable_owner = "creel"
burst_owner = "shantytown"
routing = "single-owner"

[crew.policy]
identity_source = "quipu"
tools = ["quipu", "camayoc", "bobbin"]
"#,
    )
    .unwrap();
    command(root.path(), root.path())
        .args(["apply", "--plan", "invalid.toml", "--skip-install"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "declared ownership/routing contract",
        ));
}

#[test]
fn both_settings_share_policy_but_keep_security_adapter_owned() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["plan", "--profile", "crew", "--crew", "both"])
        .assert()
        .success();
    command(root.path(), root.path())
        .arg("project-settings")
        .assert()
        .success();

    let shantytown: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.path()
                .join("caboodle-settings/shantytown.settings.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let creel: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join("caboodle-settings/creel.settings.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(shantytown["shared"], creel["shared"]);
    assert_eq!(shantytown["hooks"], "adapter-emitted");
    assert_eq!(shantytown["filesystem"], "host-workspace");
    assert!(shantytown.get("credential_policy").is_none());
    assert_eq!(creel["credential_policy"], "browser-byo-write-only");
    assert_eq!(creel["browser_permissions"], "operator-granted");
    assert!(creel.get("hooks").is_none());
}

#[test]
fn projection_emits_only_the_selected_harness() {
    for (mode, present, absent) in [
        (
            "shantytown",
            "shantytown.settings.json",
            "creel.settings.json",
        ),
        ("creel", "creel.settings.json", "shantytown.settings.json"),
    ] {
        let root = tempfile::tempdir().unwrap();
        command(root.path(), root.path())
            .args(["plan", "--profile", "crew", "--crew", mode])
            .assert()
            .success();
        command(root.path(), root.path())
            .arg("project-settings")
            .assert()
            .success();
        assert!(root
            .path()
            .join("caboodle-settings")
            .join(present)
            .is_file());
        assert!(!root.path().join("caboodle-settings").join(absent).exists());
    }
}
