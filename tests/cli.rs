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
    let intent = root.join("caboodle-intent.toml");
    if !intent.exists() {
        fs::write(
            &intent,
            r#"intended_use = "build a service graph"

[[crew_members]]
name = "Ada"
theme = "navigator"
domain = "services"
role = "answers dependencies"

[[anticipated_questions]]
question = "what depends on the service?"
answer_shape = "entity list"
seed_intent = "service A depends on service B"
sparql = "SELECT ?s WHERE { ?s ?p ?o }"
expected = "fixture-result"
"#,
        )
        .unwrap();
    }
    let mut command = Command::cargo_bin("caboodle").unwrap();
    command
        .current_dir(root)
        .env("PATH", path_with(bin))
        .env("CABOODLE_CAMAYOC_ROOT", root.join("camayoc"))
        .env("CABOODLE_CREEL_ROOT", root.join("creel"))
        .env("QUIPU_SERVER", "http://quipu.test")
        .env("FAKE_QUIPU_IMPORT_LOG", root.join("quipu-import.log"))
        .env("FAKE_CAMAYOC_STATE", root.join("camayoc-ingested"));
    command
}

fn install_fakes(root: &Path, bin: &Path) {
    fake_tool(
        bin,
        "quipu",
        r#"
if [ "${1:-}" = "--version" ]; then echo 'quipu 0.3.27'; exit 0; fi
if [ "${1:-}" = "import" ]; then
  printf '%s\n' "$*" >> "$FAKE_QUIPU_IMPORT_LOG"
  if [ "${FAKE_QUIPU_IMPORT_MODE:-}" = quarantined ]; then
    echo '{"outcome":"quarantined","share_id":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","staging_graph":"urn:quipu:import:quarantine:bbbb","promotion":{"eligible":false,"blockers":["off_vocabulary"]}}'
  else
    echo '{"outcome":"staged","share_id":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","staging_graph":"urn:quipu:import:staging:aaaa","promotion":{"eligible":true,"blockers":[]}}'
  fi
  exit 0
fi
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
if [ "${1:-}" = "--version" ]; then echo 'bobbin 0.10.3'; exit 0; fi
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
        "st",
        "if [ \"${1:-}\" = --version ]; then echo 'st 0.4.0 (test)'; exit 0; fi\nexit 2",
    );
    fake_tool(
        bin,
        "yupana",
        r#"
if [ "${1:-}" = "--version" ]; then echo 'yupana 0.6.4'; exit 0; fi
if [ "${1:-}" = "analyze" ]; then exit 0; fi
if [ "${1:-}" = "callers" ]; then
  if [ -f fixture.rs ]; then echo 'fixture.rs:2 caboodle_yupana_caller';
  else echo 'no definition found'; fi
  exit 0
fi
exit 2
"#,
    );
    fake_tool(
        bin,
        "dp",
        r#"
if [ "${1:-}" = "version" ]; then echo 'dp v0.0.0-caboodle.20260827 (1ca7b36)'; exit 0; fi
db=''
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--db" ]; then shift; db=$1; fi
  if [ "$1" = "ingest" ]; then cat >/dev/null; touch "$db.recorded"; exit 0; fi
  if [ "$1" = "list" ]; then
    if [ -f "$db.recorded" ]; then echo '[{"tool_name":"caboodle_desire_path_marker"}]'; else echo '[]'; fi
    exit 0
  fi
  shift
done
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
    fs::write(
        camayoc.join("REVISION"),
        "f33da14bba7bdd579852f5ddaa5d6328197d806f\n",
    )
    .unwrap();
    fs::write(camayoc.join("scripts/bootstrap.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::write(
        camayoc.join("ontology/core.ttl"),
        "@prefix aegis: <https://example.test/ontology/> .\n",
    )
    .unwrap();
    let creel = root.join("creel");
    fs::create_dir_all(creel.join("app/wasm/pkg")).unwrap();
    fs::write(
        creel.join("REVISION"),
        "57606dcfa0ff72d6c1bb083d70644c9926b181eb\n",
    )
    .unwrap();
    fs::write(creel.join("app/index.html"), "<!doctype html>").unwrap();
    fs::write(creel.join("app/sw.js"), "// service worker").unwrap();
    fs::write(
        creel.join("app/wasm/pkg/creel_quipu_provider_bg.wasm"),
        b"wasm",
    )
    .unwrap();
}

#[test]
fn guided_interview_writes_the_same_reviewed_plan_as_plan_command() {
    let guided_root = tempfile::tempdir().unwrap();
    command(guided_root.path(), guided_root.path())
        .args(["init", "--guided"])
        .write_stdin("crew\nboth\nbuild a service graph\n1\nAda\nnavigator\nservices\nanswers dependencies\n1\nwhat depends on the service?\nentity list\nservice A depends on service B\nSELECT ?s WHERE { ?s ?p ?o }\nfixture-result\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "profile [kg/retrieval/code-intel/crew/everything]",
        ))
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
        .write_stdin("creel\nbuild a service graph\n1\nAda\nnavigator\nservices\nanswers dependencies\n1\nwhat depends on the service?\nentity list\nservice A depends on service B\nSELECT ?s WHERE { ?s ?p ?o }\nfixture-result\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "resuming: profile already answered",
        ));
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("mode = \"creel\""));
}

#[test]
fn guided_interview_resumes_inside_a_crew_member_without_losing_answers() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["init", "--guided"])
        .write_stdin("retrieval\nbuild a service graph\n1\nAda\nnavigator\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("interview paused at end of input"));

    command(root.path(), root.path())
        .args(["init", "--guided"])
        .write_stdin("services\nanswers dependencies\n1\nwhat depends on the service?\nentity list\nservice A depends on service B\nSELECT ?s WHERE { ?s ?p ?o }\nfixture-result\n")
        .assert()
        .success();

    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("name = \"Ada\""));
    assert!(plan.contains("theme = \"navigator\""));
    assert_eq!(plan.matches("[[intent.crew_members]]").count(), 1);
}

#[test]
fn plan_rejects_unshaped_or_secret_bearing_intent() {
    for body in [
        "intended_use = \"graph\"\nanticipated_questions = []\n",
        r#"intended_use = "token=do-not-store-this"
[[anticipated_questions]]
question = "what exists?"
answer_shape = "list"
seed_intent = "fixture"
sparql = "SELECT ?s WHERE { ?s ?p ?o }"
expected = "marker"
"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("bad.toml"), body).unwrap();
        command(root.path(), root.path())
            .args(["plan", "--intent", "bad.toml"])
            .assert()
            .failure();
    }
}

#[test]
fn anticipated_questions_are_executable_and_answer_checked() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin).arg("plan").assert().success();

    fake_tool(
        &bin,
        "quipu",
        "if [ \"${1:-}\" = read ]; then echo fixture-result; exit 0; fi\nexit 2",
    );
    command(root.path(), &bin)
        .arg("verify-questions")
        .assert()
        .success()
        .stdout(predicate::str::contains("question 1: verified"));

    fake_tool(
        &bin,
        "quipu",
        "if [ \"${1:-}\" = read ]; then echo wrong-result; exit 0; fi\nexit 2",
    );
    command(root.path(), &bin)
        .arg("verify-questions")
        .assert()
        .failure()
        .stderr(predicate::str::contains("did not contain"));
}

#[test]
fn guided_interview_rejects_invalid_answers_without_a_plan() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["init", "--guided"])
        .write_stdin("invalid\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid profile"));
    assert!(!root.path().join("caboodle-plan.toml").exists());
}

#[test]
fn code_intel_and_everything_profiles_expand_the_verified_corpus() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);

    command(root.path(), &bin)
        .args(["plan", "--profile", "code-intel", "--output", "code.toml"])
        .assert()
        .success();
    let code = fs::read_to_string(root.path().join("code.toml")).unwrap();
    assert!(code.contains("\"yupana\""));
    assert!(!code.contains("\"desire-path\""));

    command(root.path(), &bin)
        .args(["plan", "--profile", "everything"])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("yupana: verified"))
        .stdout(predicate::str::contains("desire-path: verified"));
}

#[test]
fn everything_plan_can_include_both_crew_runtimes() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["plan", "--profile", "everything", "--crew", "both"])
        .assert()
        .success();
    let body = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(body.contains("mode = \"both\""));
    assert!(body.contains("\"desire-path\""));
    assert!(body.contains("durable_owner = \"shantytown\""));
    assert!(body.contains("burst_owner = \"creel\""));
}

#[test]
fn check_updates_is_green_when_reviewed_versions_run_and_red_on_drift() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["plan", "--profile", "retrieval"])
        .assert()
        .success();
    command(root.path(), &bin)
        .arg("check-updates")
        .assert()
        .success()
        .stdout(predicate::str::contains("bobbin: current (bobbin 0.10.3)"));

    fake_tool(
        &bin,
        "bobbin",
        "if [ \"${1:-}\" = --version ]; then echo 'bobbin 0.8.0'; exit 0; fi\nexit 2",
    );
    command(root.path(), &bin)
        .arg("check-updates")
        .assert()
        .failure()
        .stdout(predicate::str::contains("bobbin: update available"))
        .stderr(predicate::str::contains("pending reviewed updates"));
}

#[test]
fn update_is_idempotent_when_every_selected_release_is_current() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["plan", "--profile", "retrieval"])
        .assert()
        .success();
    command(root.path(), &bin)
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu: current"))
        .stdout(predicate::str::contains("camayoc: current"))
        .stdout(predicate::str::contains("bobbin: current"));
    assert!(!root.path().join(".caboodle/state.json").exists());
}

#[test]
fn expanded_adapter_negative_controls_turn_verification_red() {
    let yupana_root = tempfile::tempdir().unwrap();
    let yupana_bin = yupana_root.path().join("bin");
    fs::create_dir(&yupana_bin).unwrap();
    install_fakes(yupana_root.path(), &yupana_bin);
    fake_tool(
        &yupana_bin,
        "yupana",
        "if [ \"${1:-}\" = --version ]; then echo 'yupana 0.6.4'; exit 0; fi\nif [ \"${1:-}\" = analyze ]; then exit 0; fi\nif [ \"${1:-}\" = callers ]; then echo 'fixture.rs:2 caboodle_yupana_caller'; exit 0; fi\nexit 2",
    );
    command(yupana_root.path(), &yupana_bin)
        .args(["plan", "--profile", "code-intel"])
        .assert()
        .success();
    command(yupana_root.path(), &yupana_bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Yupana negative control unexpectedly found",
        ));

    let dp_root = tempfile::tempdir().unwrap();
    let dp_bin = dp_root.path().join("bin");
    fs::create_dir(&dp_bin).unwrap();
    install_fakes(dp_root.path(), &dp_bin);
    fake_tool(
        &dp_bin,
        "dp",
        "if [ \"${1:-}\" = version ]; then echo 'dp v0.0.0-caboodle.20260827 (1ca7b36)'; exit 0; fi\necho '[{\"tool_name\":\"caboodle_desire_path_marker\"}]'",
    );
    command(dp_root.path(), &dp_bin)
        .args(["plan", "--profile", "everything"])
        .assert()
        .success();
    command(dp_root.path(), &dp_bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Desire Path negative control unexpectedly found",
        ));
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
fn profile_stages_canonical_quipu_shares_without_promoting_them() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    fs::create_dir(root.path().join("team-share")).unwrap();

    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--share",
            "team-share",
            "--quipu-db",
            "knowledge.db",
        ])
        .assert()
        .success();
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("shares = [\"team-share\"]"));
    assert!(plan.contains("quipu_db = \"knowledge.db\""));

    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("share sha256:aaaa"))
        .stdout(predicate::str::contains("promotion eligible: true"));

    let log = fs::read_to_string(root.path().join("quipu-import.log")).unwrap();
    assert_eq!(log.trim(), "import team-share --db knowledge.db");
    assert!(!log.contains("promote"));
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["shares"].as_object().unwrap().len(), 1);
    assert_eq!(
        state["shares"]["sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
            ["promotion_eligible"],
        true
    );
}

#[test]
fn quarantined_share_is_preserved_for_review_and_never_auto_promoted() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    fs::create_dir(root.path().join("foreign-share")).unwrap();
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "kg",
            "--share",
            "foreign-share",
            "--quipu-db",
            "knowledge.db",
        ])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .env("FAKE_QUIPU_IMPORT_MODE", "quarantined")
        .assert()
        .success()
        .stdout(predicate::str::contains("quarantined"))
        .stdout(predicate::str::contains("promotion eligible: false"));
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    let share =
        &state["shares"]["sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"];
    assert_eq!(share["blockers"][0], "off_vocabulary");
    assert!(!fs::read_to_string(root.path().join("quipu-import.log"))
        .unwrap()
        .contains("promote"));
}

#[test]
fn share_selection_requires_an_explicit_quipu_database() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["plan", "--share", "team-share"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--quipu-db"));
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

fn write_creel_contracts(root: &Path, doctor_status: &str, verdict: &str) -> (String, String) {
    let doctor = root.join("creel-doctor.json");
    let admission = root.join("creel-admission.json");
    fs::write(
        &doctor,
        format!(
            r#"{{
  "schema_version": 1,
  "overall": "{doctor_status}",
  "checks": [{{
    "id": "secure-context",
    "status": "{doctor_status}",
    "severity": "required",
    "evidence": "browser reported a secure context",
    "remediation": "serve Creel over HTTPS",
    "redacted": true
  }}]
}}"#
        ),
    )
    .unwrap();
    fs::write(
        &admission,
        format!(
            r#"{{
  "schema_version": 1,
  "verdict": "{verdict}",
  "provider_window": {{"status":"pass","evidence":"window below ceiling"}},
  "device_tab_cap": {{"status":"pass","evidence":"one slot available"}},
  "signal_freshness": {{"status":"pass","evidence":"signals observed now"}},
  "reason": "launch is within the redacted policy limits",
  "redacted": true
}}"#
        ),
    )
    .unwrap();
    (
        doctor.to_string_lossy().into_owned(),
        admission.to_string_lossy().into_owned(),
    )
}

#[test]
fn crew_install_records_shantytown_and_creel_without_crossing_ownership() {
    for mode in ["shantytown", "creel", "both"] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        install_fakes(root.path(), &bin);
        command(root.path(), &bin)
            .args(["plan", "--profile", "crew", "--crew", mode])
            .assert()
            .success();
        command(root.path(), &bin)
            .args(["apply", "--skip-install"])
            .assert()
            .success();

        let state: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(state["crew"].get("shantytown").is_some(), mode != "creel");
        assert_eq!(state["crew"].get("creel").is_some(), mode != "shantytown");
    }
}

#[test]
fn creel_verification_requires_both_external_capability_contracts() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["plan", "--profile", "crew", "--crew", "creel"])
        .assert()
        .success();
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --creel-doctor"));

    let (doctor, admission) = write_creel_contracts(root.path(), "pass", "admit");
    command(root.path(), &bin)
        .args([
            "verify",
            "--creel-doctor",
            &doctor,
            "--creel-admission",
            &admission,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("creel: verified"));
}

#[test]
fn creel_verification_refuses_unknown_doctor_and_policy_refusal() {
    for (doctor_status, verdict, message) in [
        ("unknown", "admit", "required doctor check"),
        ("pass", "refuse", "governor did not admit"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        install_fakes(root.path(), &bin);
        command(root.path(), &bin)
            .args(["plan", "--profile", "crew", "--crew", "creel"])
            .assert()
            .success();
        let (doctor, admission) = write_creel_contracts(root.path(), doctor_status, verdict);
        command(root.path(), &bin)
            .args([
                "verify",
                "--creel-doctor",
                &doctor,
                "--creel-admission",
                &admission,
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(message));
    }
}
