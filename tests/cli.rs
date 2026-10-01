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
        .env("HOME", root)
        .env_remove("QUIPU_AUTH_TOKEN")
        .env_remove("QUIPU_AUTH_TOKEN_FILE")
        .env("PATH", path_with(bin))
        .env("CARGO_HOME", root.join("cargo-home"))
        .env("CABOODLE_CAMAYOC_ROOT", root.join("camayoc"))
        .env("CABOODLE_CREEL_ROOT", root.join("creel"))
        .env("QUIPU_SERVER", "http://quipu.test")
        .env("FAKE_QUIPU_IMPORT_LOG", root.join("quipu-import.log"))
        .env("FAKE_MODEL_FETCH_LOG", root.join("model-fetch.log"))
        .env("FAKE_CAMAYOC_STATE", root.join("camayoc-ingested"))
        // The committed copy of the reviewed Quechua release (aegis-1i5h1j.3):
        // its digest is still checked against the pin, so no network is needed.
        .env(
            "CABOODLE_QUECHUA_FILE",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/vocabulary/quechua-ns-v0.1.0.ttl"),
        );
    command
}

fn install_fakes(root: &Path, bin: &Path) {
    // The fixture stack member (fixture-members feature, aegis-1i5h1j) joins the
    // everything profile. Put its REAL program from the committed release on
    // PATH, so profile tests run its real hermetic verify.
    #[cfg(feature = "fixture-members")]
    install_fixture_member(bin);
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
if [ "${1:-}" = "knot" ]; then
  file=$2; db=''
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--db" ]; then shift; db=$1; fi
    shift || true
  done
  cp "$file" "$db.knot"
  exit 0
fi
if [ "${1:-}" = "read" ]; then
  query=$2; db=''
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--db" ]; then shift; db=$1; fi
    shift || true
  done
  case "$query" in
    ASK*)
      # Answer from what `knot` stored: the version literal, or a declared term.
      if [ ! -f "$db.knot" ]; then echo false; exit 0; fi
      case "$query" in
        *versionInfo*)
          v=$(printf '%s' "$query" | sed -n 's/.*versionInfo> "\([^"]*\)".*/\1/p')
          if grep -q "owl:versionInfo \"$v\"" "$db.knot"; then echo true; else echo false; fi ;;
        *)
          term=$(printf '%s' "$query" | sed -n 's/.*quechua\/ns#\([A-Za-z0-9_]*\)>.*/\1/p')
          if grep -q "ns#$term>" "$db.knot"; then echo true; else echo false; fi ;;
      esac
      exit 0 ;;
  esac
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
if [ "${1:-}" = "--version" ]; then echo 'bobbin 0.25.2'; exit 0; fi
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
        // `ops hooks register <file>` answers like st: `<name>: installed`, the
        // name read from the bundle it was given (aegis-u1ybxo), and logs it.
        // `ops hooks check --json` reports every registered bundle configured.
        "if [ \"${1:-}\" = --version ]; then echo 'st 0.4.0 (test)'; exit 0; fi\n\
         if [ \"${1:-} ${2:-} ${3:-}\" = 'ops hooks register' ]; then\n\
           n=$(sed -n 's/.*\"name\": *\"\\([^\"]*\\)\".*/\\1/p' \"$4\" | head -n 1)\n\
           v=$(sed -n 's/.*\"version\": *\"\\([^\"]*\\)\".*/\\1/p' \"$4\" | head -n 1)\n\
           echo \"$n\" >> \"$(dirname \"$0\")/st-registered.log\"\n\
           echo \"$n $v\" >> \"$(dirname \"$0\")/st-versions.log\"\n\
           echo \"$n: installed\"; exit 0\n\
         fi\n\
         if [ \"${1:-} ${2:-} ${3:-}\" = 'ops hooks check' ]; then\n\
           printf '{\"schema\":\"st.hook-check/1\",\"registry_errors\":[],\"items\":['\n\
           sep=''\n\
           if [ -f \"$(dirname \"$0\")/st-versions.log\" ]; then\n\
             while read -r n v; do\n\
               printf '%s{\"bundle\":\"%s\",\"version\":\"%s\",\"configured\":\"ok\",\"live\":\"ok\"}' \"$sep\" \"$n\" \"$v\"; sep=','\n\
             done < \"$(dirname \"$0\")/st-versions.log\"\n\
           fi\n\
           printf ']}\\n'; exit 0\n\
         fi\nexit 2",
    );
    // Its registry starts holding every shipped bundle, as on a host that was
    // applied: verify asserts the bundles, and tests about other steps must
    // not trip on them. A test about registration clears this first.
    let mut seeded = String::new();
    for entry in fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("hook-bundles")).unwrap() {
        let bundle: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(entry.unwrap().path()).unwrap()).unwrap();
        seeded.push_str(&format!(
            "{} {}\n",
            bundle["name"].as_str().unwrap(),
            bundle["version"].as_str().unwrap()
        ));
    }
    fs::write(bin.join("st-versions.log"), seeded).unwrap();
    fake_tool(
        bin,
        "yupana",
        r#"
if [ "${1:-}" = "--version" ]; then echo 'yupana 0.10.6'; exit 0; fi
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
if [ "${1:-}" = "version" ]; then echo 'dp v0.3.2 (1f457ec)'; exit 0; fi
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
    // seeds (members/seeds.toml): the crew and everything profiles select it,
    // so every plan for them installs and verifies `sd`. Identity comes from
    // --help and the version from --version, exactly as the manifest reads them;
    // list/create keep the verify marker in a file in the hermetic root. The
    // version is the PINNED one, read from the embedded manifest: a hardcoded
    // version would turn every bump-member PR red (caboodle#50).
    let seeds = caboodle::members::get("seeds").expect("seeds is an embedded member");
    fake_tool(
        bin,
        "sd",
        &format!(
            r#"
case "${{1:-}}" in
  --help) echo 'seeds is the beads-compatible tracker for the quipu stack (test)'; exit 0 ;;
  --version) echo 'sd {version} (seeds)'; exit 0 ;;
  create) printf '%s\n' "$2" >> .sd-fake-seeds; echo "created $2"; exit 0 ;;
  list) cat .sd-fake-seeds 2>/dev/null || echo 'no matching seeds'; exit 0 ;;
esac
exit 2
"#,
            version = seeds.version
        ),
    );
    // shuttle (members/shuttle.toml), same rules: identity from --help, the
    // PINNED version from `version`. A run only reaches "claimed" in status
    // after `advance`, so the fake keeps the manifest's read-back meaningful.
    let shuttle = caboodle::members::get("shuttle").expect("shuttle is an embedded member");
    fake_tool(
        bin,
        "shuttle",
        &format!(
            r#"
case "${{1:-}}" in
  --help) echo 'The `shuttle` CLI (test)'; exit 0 ;;
  version) echo 'shuttle {version}'; exit 0 ;;
  define) cat >/dev/null; exit 0 ;;
  start) echo "$5" >> .shuttle-fake-started; exit 0 ;;
  advance) grep -qx "$6" .shuttle-fake-started && printf '%s\tcaboodle-verify\tclaimed\topen\n' "$6" >> .shuttle-fake-runs; exit 0 ;;
  status) cat .shuttle-fake-runs 2>/dev/null || true; exit 0 ;;
esac
exit 2
"#,
            version = shuttle.version
        ),
    );
    fake_tool(
        bin,
        "curl",
        r#"
args=$*
case "$args" in
  *"models.test"*)
    out=''
    while [ "$#" -gt 0 ]; do
      if [ "$1" = "--output" ]; then shift; out=$1; fi
      shift || true
    done
    printf '%s' "${FAKE_MODEL_BODY:-caboodle-model-fixture}" > "$out"
    printf '%s\n' fetched >> "$FAKE_MODEL_FETCH_LOG"
    ;;
  *"/version"*)
    if [ "${FAKE_QUIPU_LANCEDB:-}" = present ]; then echo '{"version":"0.3.27","features":{"lancedb":true,"onnx":true}}'
    elif [ "${FAKE_QUIPU_LANCEDB:-}" = absent ]; then echo '{"version":"0.3.27","features":{"lancedb":false,"onnx":true}}'
    else echo '{"version":"0.3.27"}'; fi
    ;;
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
        "f443bf19974d06ab662c8ae3deeccfbb1e082f48\n",
    )
    .unwrap();
    // Behaves like the real bootstrap when nothing answers: starts a server process
    // under $CLAUDE_PROJECT_DIR/.quipu and records its pid, plus the server it was
    // pointed at so tests can prove verification never used QUIPU_SERVER.
    fs::write(
        camayoc.join("scripts/bootstrap.sh"),
        "#!/bin/sh\nprintf '%s\\n' \"$QUIPU_SERVER\" >> \"$HOME/camayoc-bootstrap.log\"\nmkdir -p \"$CLAUDE_PROJECT_DIR/.quipu\"\nsleep 300 >/dev/null 2>&1 &\necho $! > \"$CLAUDE_PROJECT_DIR/.quipu/server.pid\"\necho $! >> \"$HOME/camayoc-server-pids.log\"\nexit 0\n",
    )
    .unwrap();
    fs::write(
        camayoc.join("ontology/core.ttl"),
        "@prefix aegis: <https://example.test/ontology/> .\n",
    )
    .unwrap();
    let creel = root.join("creel");
    fs::create_dir_all(creel.join("app/wasm/pkg")).unwrap();
    fs::write(
        creel.join("REVISION"),
        "0003aee9b1eec512e59d13b64a6c5a4d3b8b55d6\n",
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
fn guided_interview_offers_a_self_test_question_that_verifies_on_a_fresh_box() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    // Enter at the question-count prompt: a newcomer has no ontology to write SPARQL for.
    command(root.path(), &bin)
        .args(["init", "--guided"])
        .write_stdin(
            "kg\nbuild a service graph\n1\nAda\nnavigator\nservices\nanswers dependencies\n\n",
        )
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "using the built-in self-test question",
        ));
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("caboodle-verify-roundtrip"), "{plan}");

    // It must pass without any store of the user's, even when pointed at one
    // that does not exist: the self-test seeds and reads its own scratch store.
    command(root.path(), &bin)
        .args(["verify-questions", "--db"])
        .arg(root.path().join("no-such-user-store.db"))
        .assert()
        .success()
        .stdout(predicate::str::contains("question 1: verified"));
    assert!(!root.path().join("no-such-user-store.db").exists());
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
        .stdout(predicate::str::contains("bobbin: current (bobbin 0.25.2)"));

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
fn check_updates_reads_quipu_from_cargo_home_before_a_shadowing_path() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let cargo_bin = root.path().join("cargo/bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(&cargo_bin).unwrap();
    install_fakes(root.path(), &bin);
    fake_tool(
        &bin,
        "quipu",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu 0.3.7'; exit 0; fi\nexit 2",
    );
    fake_tool(
        &bin,
        "quipu-server",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu-server 0.3.7'; exit 0; fi\nexit 2",
    );
    fake_tool(
        &cargo_bin,
        "quipu",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu 0.3.27'; exit 0; fi\nexit 2",
    );
    fake_tool(
        &cargo_bin,
        "quipu-server",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu-server 0.3.27'; exit 0; fi\nexit 2",
    );

    command(root.path(), &bin)
        .env("CARGO_HOME", root.path().join("cargo"))
        .args(["plan", "--profile", "kg"])
        .assert()
        .success();
    command(root.path(), &bin)
        .env("CARGO_HOME", root.path().join("cargo"))
        .arg("check-updates")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "quipu: current (quipu 0.3.27; quipu-server 0.3.27)",
        ));
}

#[test]
fn check_updates_rejects_stale_path_despite_current_cargo_copy() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let cargo_bin = root.path().join("cargo/bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(&cargo_bin).unwrap();
    install_fakes(root.path(), &bin);
    fake_tool(
        &bin,
        "dp",
        "if [ \"${1:-}\" = version ]; then echo 'dp c964ea2 (c964ea2)'; exit 0; fi\nexit 2",
    );
    fake_tool(
        &cargo_bin,
        "dp",
        "if [ \"${1:-}\" = version ]; then echo 'dp v0.3.2 (1f457ec)'; exit 0; fi\nexit 2",
    );

    command(root.path(), &bin)
        .env("CARGO_HOME", root.path().join("cargo"))
        .args(["plan", "--profile", "everything"])
        .assert()
        .success();
    command(root.path(), &bin)
        .env("CARGO_HOME", root.path().join("cargo"))
        .arg("check-updates")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "dp version is not the CABOODLE-pinned revision: dp c964ea2",
        ));
}

#[test]
fn check_updates_accepts_current_path_despite_stale_cargo_copy() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let cargo_bin = root.path().join("cargo/bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(&cargo_bin).unwrap();
    install_fakes(root.path(), &bin);
    fake_tool(
        &bin,
        "dp",
        "if [ \"${1:-}\" = version ]; then echo 'dp 0.3.2 (1f457ec)'; exit 0; fi\nexit 2",
    );
    fake_tool(
        &cargo_bin,
        "dp",
        "if [ \"${1:-}\" = version ]; then echo 'dp stale (1ca7b36)'; exit 0; fi\nexit 2",
    );

    command(root.path(), &bin)
        .env("CARGO_HOME", root.path().join("cargo"))
        .args(["plan", "--profile", "everything"])
        .assert()
        .success();
    command(root.path(), &bin)
        .env("CARGO_HOME", root.path().join("cargo"))
        .arg("check-updates")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "desire-path: current (dp 0.3.2 (1f457ec))",
        ));
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
        "if [ \"${1:-}\" = --version ]; then echo 'yupana 0.10.6'; exit 0; fi\nif [ \"${1:-}\" = analyze ]; then exit 0; fi\nif [ \"${1:-}\" = callers ]; then echo 'fixture.rs:2 caboodle_yupana_caller'; exit 0; fi\nexit 2",
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
        "if [ \"${1:-}\" = version ]; then echo 'dp v0.3.2 (1f457ec)'; exit 0; fi\necho '[{\"tool_name\":\"caboodle_desire_path_marker\"}]'",
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
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("[stack_config.quipu.owl]"));
    assert!(plan.contains("reactive_materialize = true"));

    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu: verified"))
        .stdout(predicate::str::contains("bobbin: verified"));

    let configured = fs::read_to_string(root.path().join(".config/bobbin/config.toml")).unwrap();
    assert!(configured.contains("[quipu.owl]"));
    assert!(configured.contains("reactive_materialize = true"));

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

fn retrieval_plan_with_bobbin(
    version: &str,
    functional: bool,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    if !functional || version != "0.25.2" {
        // Any subcommand but --version fails like a clap argument error, which is
        // what an old bobbin says to the reviewed release's verification contract.
        fake_tool(
            &bin,
            "bobbin",
            &format!(
                "if [ \"${{1:-}}\" = --version ]; then echo 'bobbin {version}'; exit 0; fi\n\
                 echo \"error: unexpected argument '--source' found\" >&2; exit 2"
            ),
        );
    }
    command(root.path(), &bin)
        .args(["plan", "--profile", "retrieval"])
        .assert()
        .success();
    (root, bin)
}

#[test]
fn apply_skip_install_refuses_a_stale_tool_naming_both_versions() {
    let (root, bin) = retrieval_plan_with_bobbin("0.1.0", false);
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("0.1.0"))
        .stderr(predicate::str::contains("0.25.2"));
}

#[test]
fn apply_skip_install_accepts_the_reviewed_release() {
    // Control for the test above: without it, "fails on a skew" is
    // indistinguishable from "apply --skip-install always fails".
    let (root, bin) = retrieval_plan_with_bobbin("0.25.2", true);
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bobbin: applied"));
}

#[test]
fn apply_declines_to_downgrade_a_tool_ahead_of_the_pin() {
    let (root, bin) = retrieval_plan_with_bobbin("0.99.0", false);
    command(root.path(), &bin)
        .args(["apply"])
        .assert()
        .success()
        .stdout(predicate::str::contains("bobbin: NOT converged"));
    // Nothing was fetched or replaced.
    assert!(!root.path().join(".local/share/caboodle/bobbin").exists());
}

#[test]
fn verify_names_a_version_skew_before_the_tools_own_error() {
    let (root, bin) = retrieval_plan_with_bobbin("0.1.0", false);
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "VERSION SKEW: bobbin 0.1.0 is installed",
        ))
        .stderr(predicate::str::contains("unexpected argument '--source'"));
}

#[test]
fn verify_does_not_claim_a_skew_when_the_reviewed_release_fails() {
    // Control: the skew message must not fire on every verification failure.
    let (root, bin) = retrieval_plan_with_bobbin("0.25.2", false);
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("bobbin functional verification"))
        .stderr(predicate::str::contains("VERSION SKEW").not());
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
        .stdout(predicate::str::contains("promotion eligible: true"))
        .stdout(predicate::str::contains(
            "vocabulary: quechua v0.1.0 loaded into knowledge.db",
        ));
    // aegis-1i5h1j.3: the pinned Quechua release was knotted into the plan's
    // store, byte-for-byte the reviewed asset.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/vocabulary/quechua-ns-v0.1.0.ttl");
    assert_eq!(
        fs::read(root.path().join("knowledge.db.knot")).unwrap(),
        fs::read(fixture).unwrap()
    );

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
fn camayoc_verification_uses_its_own_scratch_server_and_stops_it() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg"])
        .assert()
        .success();
    command(root.path(), &bin).arg("verify").assert().success();

    let servers = fs::read_to_string(root.path().join("camayoc-bootstrap.log")).unwrap();
    assert!(!servers.is_empty(), "camayoc bootstrap never ran");
    for server in servers.lines() {
        assert!(
            server.starts_with("http://127.0.0.1:"),
            "bootstrap was pointed at {server}, not a scratch localhost server"
        );
        assert!(
            !server.contains("quipu.test"),
            "verification reached QUIPU_SERVER"
        );
    }
    assert!(
        !root.path().join(".quipu").exists(),
        "verification left a .quipu store in the working directory"
    );
    let pids = fs::read_to_string(root.path().join("camayoc-server-pids.log")).unwrap();
    for pid in pids.lines() {
        // SIGTERM delivery and reaping are asynchronous. Keep the strict
        // disappearance assertion, but allow the child a bounded exit window.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let alive = std::process::Command::new("kill")
                .args(["-0", pid])
                .status()
                .unwrap()
                .success();
            if !alive {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "scratch quipu-server {pid} was left running"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
fn a_plan_naming_an_unknown_member_is_refused_by_name() {
    // wu M1: a member a later caboodle removed must refuse with a named message.
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("removed.toml"),
        "schema_version = 1\nprofile = \"everything\"\ntools = [\"quipu\", \"retired-member\"]\n",
    )
    .unwrap();
    command(root.path(), root.path())
        .args(["apply", "--plan", "removed.toml", "--skip-install"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown tool 'retired-member'"))
        .stderr(predicate::str::contains(
            "stack member of this caboodle build",
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
        .args(["project-settings", "--policy-only"])
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
            .args(["project-settings", "--policy-only"])
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

#[test]
fn plan_records_the_quipu_flavor_only_when_it_departs_from_release() {
    let root = tempfile::tempdir().unwrap();
    command(root.path(), root.path())
        .args(["plan", "--profile", "kg"])
        .assert()
        .success();
    let default = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(!default.contains("quipu_flavor"));

    command(root.path(), root.path())
        .args([
            "plan",
            "--profile",
            "kg",
            "--quipu-flavor",
            "lancedb",
            "--output",
            "lancedb.toml",
        ])
        .assert()
        .success();
    let flavored = fs::read_to_string(root.path().join("lancedb.toml")).unwrap();
    assert!(flavored.contains("quipu_flavor = \"lancedb\""));
}

#[test]
fn lancedb_flavor_is_proven_by_the_compile_map_and_refused_without_it() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg", "--quipu-flavor", "lancedb"])
        .assert()
        .success();

    command(root.path(), &bin)
        .arg("verify")
        .env("FAKE_QUIPU_LANCEDB", "present")
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu: verified"));

    command(root.path(), &bin)
        .arg("verify")
        .env("FAKE_QUIPU_LANCEDB", "absent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("compiled without it"));

    // A server that reports no compile map cannot prove the flavor either way;
    // that must read as a refusal, never as a pass.
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no per-feature compile map"));
}

#[test]
fn release_flavor_verification_never_consults_the_compile_map() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg"])
        .assert()
        .success();
    // FAKE_QUIPU_LANCEDB stays unset: if the release flavor asked /version it
    // would see no compile map and go red, so green proves it never asked.
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu: verified"));
}

const MODEL_FIXTURE_SHA256: &str =
    "4c0087274c62d351549815a5b54dd28e827ed1a5d383b8c1d01b9f737616a9e2";

fn write_model_spec(root: &Path) -> String {
    let spec = root.join("embedding-model.toml");
    fs::write(
        &spec,
        format!(
            r#"destination = "models"

[[artifacts]]
name = "model.onnx"
url = "https://models.test/model.onnx"
sha256 = "{MODEL_FIXTURE_SHA256}"
"#
        ),
    )
    .unwrap();
    spec.to_string_lossy().into_owned()
}

#[test]
fn embedding_model_artifacts_are_fetched_pinned_idempotent_and_recorded() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    let spec = write_model_spec(root.path());
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg", "--embedding-model", &spec])
        .assert()
        .success();
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("[embedding_model]"));
    assert!(plan.contains(MODEL_FIXTURE_SHA256));

    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "embedding-model model.onnx: provisioned",
        ));
    assert_eq!(
        fs::read(root.path().join("models/model.onnx")).unwrap(),
        b"caboodle-model-fixture"
    );
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["models"]["model.onnx"]["provisioned"], true);
    assert_eq!(state["models"]["model.onnx"]["verified"], false);
    assert_eq!(
        state["models"]["model.onnx"]["sha256"],
        MODEL_FIXTURE_SHA256
    );

    // The artifact is already pinned on disk, so a rerun records it as
    // current without another download.
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "embedding-model model.onnx: current",
        ));
    let fetches = fs::read_to_string(root.path().join("model-fetch.log")).unwrap();
    assert_eq!(fetches.lines().count(), 1);

    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "embedding-model model.onnx: digest re-checked",
        ));
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["models"]["model.onnx"]["verified"], true);
}

#[test]
fn embedding_model_checksum_mismatch_refuses_and_deletes_the_download() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    let spec = write_model_spec(root.path());
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg", "--embedding-model", &spec])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .env("FAKE_MODEL_BODY", "tampered-model-bytes")
        .assert()
        .failure()
        .stderr(predicate::str::contains("model.onnx checksum mismatch"))
        .stderr(predicate::str::contains(
            "embedding-model provisioning step",
        ));
    // The refused bytes must be gone: no artifact and no staging leftovers.
    assert!(!root.path().join("models/model.onnx").exists());
    assert_eq!(fs::read_dir(root.path().join("models")).unwrap().count(), 0);
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    assert!(state["models"].as_object().unwrap().is_empty());
}

#[test]
fn embedding_model_verify_goes_red_when_bytes_drift_on_disk() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    let spec = write_model_spec(root.path());
    command(root.path(), &bin)
        .args(["plan", "--profile", "kg", "--embedding-model", &spec])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success();

    fs::write(root.path().join("models/model.onnx"), b"drifted-bytes").unwrap();
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("model.onnx drifted on disk"));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn release_fixture(root: &Path, bin: &Path, broken: bool, bad_checksum: bool) {
    install_fakes(root, bin);
    let stage = root.join("release-stage");
    fs::create_dir(&stage).unwrap();
    let body = if broken {
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'bobbin 0.25.3'; exit 0; fi\nexit 9\n"
            .to_owned()
    } else {
        fs::read_to_string(bin.join("bobbin"))
            .unwrap()
            .replace("0.25.2", "0.25.3")
    };
    fs::write(stage.join("bobbin"), body).unwrap();
    fs::set_permissions(stage.join("bobbin"), fs::Permissions::from_mode(0o755)).unwrap();
    let archive = root.join("bobbin-v0.25.3-x86_64-unknown-linux-gnu.tar.gz");
    assert!(std::process::Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .arg("bobbin")
        .status()
        .unwrap()
        .success());
    use sha2::{Digest, Sha256};
    let digest = if bad_checksum {
        "0".repeat(64)
    } else {
        format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()))
    };
    fs::write(
        root.join("SHA256SUMS.txt"),
        format!("{digest}  bobbin-v0.25.3-x86_64-unknown-linux-gnu.tar.gz\n"),
    )
    .unwrap();
    fs::write(root.join("latest.json"), r#"{"tag_name":"v0.25.3","draft":false,"prerelease":false,"assets":[{"name":"bobbin-v0.25.3-x86_64-unknown-linux-gnu.tar.gz"},{"name":"SHA256SUMS.txt"}]}"#).unwrap();
    fake_tool(
        bin,
        "curl",
        r#"
url=''; output=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output|-o) shift; output=$1 ;;
    https://*) url=$1 ;;
  esac
  shift
done
case "$url" in
  */releases/latest) cat "$HOME/latest.json" ;;
  */SHA256SUMS.txt) cp "$HOME/SHA256SUMS.txt" "$output" ;;
  */bobbin-v0.25.3-x86_64-unknown-linux-gnu.tar.gz) cp "$HOME/bobbin-v0.25.3-x86_64-unknown-linux-gnu.tar.gz" "$output" ;;
  *) echo "unexpected URL" >&2; exit 99 ;;
esac
"#,
    );
    command(root, bin)
        .args(["plan", "--profile", "retrieval"])
        .assert()
        .success();
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn published_release_update_ignores_stale_pin_and_keeps_backup() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    release_fixture(root.path(), &bin, false, false);
    let before = fs::read(bin.join("bobbin")).unwrap();
    command(root.path(), &bin)
        .args(["update-release", "--tool", "bobbin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed and verified v0.25.3"));
    let state = fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap();
    assert!(state.contains("0.25.3"));
    let backups: Vec<_> = fs::read_dir(root.path().join(".caboodle/release-backups/bobbin"))
        .unwrap()
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::read(backups[0].as_ref().unwrap().path()).unwrap(),
        before
    );
    command(root.path(), &bin)
        .args(["update-release", "--tool", "bobbin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("current and verified"));
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn published_release_bad_checksum_and_failed_functional_proof_preserve_previous() {
    for (broken, bad_checksum, message) in [
        (false, true, "SHA256 mismatch"),
        (true, false, "previous artifact restored"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        release_fixture(root.path(), &bin, broken, bad_checksum);
        let before = fs::read(bin.join("bobbin")).unwrap();
        command(root.path(), &bin)
            .args(["update-release", "--tool", "bobbin"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(message));
        assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
        assert!(!root.path().join(".caboodle/state.json").exists());
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn published_release_ahead_and_hold_never_downgrade() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    release_fixture(root.path(), &bin, false, false);
    fake_tool(&bin, "bobbin", "echo 'bobbin 9.0.0'");
    let before = fs::read(bin.join("bobbin")).unwrap();
    command(root.path(), &bin)
        .args(["update-release", "--tool", "bobbin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("refusing downgrade"));
    assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
    let hold = root.path().join("hold");
    fs::write(&hold, "").unwrap();
    fs::remove_file(root.path().join("latest.json")).unwrap();
    command(root.path(), &bin)
        .env("CABOODLE_HOLD_FILE", &hold)
        .args(["update-release", "--tool", "bobbin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("held"));
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn published_release_interrupted_update_recovers_even_during_hold() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    release_fixture(root.path(), &bin, false, false);
    let before = fs::read(bin.join("bobbin")).unwrap();
    let state_dir = root.path().join(".caboodle");
    fs::create_dir_all(&state_dir).unwrap();
    let backup = state_dir.join("old");
    fs::write(&backup, &before).unwrap();
    use sha2::{Digest, Sha256};
    let sha = format!("{:x}", Sha256::digest(&before));
    fs::write(state_dir.join("state.release-pending.json"), serde_json::json!({"tool":"bobbin", "destination":bin.join("bobbin"), "backup":backup, "sha256":sha}).to_string()).unwrap();
    fake_tool(&bin, "bobbin", "exit 99");
    let hold = root.path().join("hold");
    fs::write(&hold, "").unwrap();
    command(root.path(), &bin)
        .env("CABOODLE_HOLD_FILE", &hold)
        .args(["update-release", "--tool", "bobbin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recovered interrupted update"));
    assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
    assert!(!state_dir.join("state.release-pending.json").exists());
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn reviewed_pin_update_cannot_undo_newer_published_binary() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    release_fixture(root.path(), &bin, false, false);
    fake_tool(&bin, "bobbin", "echo 'bobbin 0.25.3'");
    let before = fs::read(bin.join("bobbin")).unwrap();
    command(root.path(), &bin)
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing reviewed-pin downgrade"));
    assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn published_release_missing_assets_and_ambiguous_identity_never_install() {
    for mode in ["missing", "equal", "unreadable", "check"] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        release_fixture(root.path(), &bin, false, false);
        if mode == "missing" {
            fs::write(
                root.path().join("latest.json"),
                r#"{"tag_name":"v0.25.3","draft":false,"prerelease":false,"assets":[]}"#,
            )
            .unwrap();
        } else if mode == "equal" {
            fake_tool(&bin, "bobbin", "echo 'bobbin 0.25.3'");
        } else if mode == "unreadable" {
            fake_tool(&bin, "bobbin", "echo 'bobbin unknown'");
        }
        let before = fs::read(bin.join("bobbin")).unwrap();
        let mut cmd = command(root.path(), &bin);
        cmd.args(["update-release", "--tool", "bobbin"]);
        if mode == "check" {
            cmd.arg("--check").assert().success();
        } else {
            cmd.assert().failure();
        }
        assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
        assert!(!root.path().join(".caboodle/state.json").exists());
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn published_release_can_update_the_installer_itself() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    release_fixture(root.path(), &bin, false, false);
    fake_tool(&bin, "caboodle", "echo 'caboodle 0.2.0'");
    let stage = root.path().join("self-stage");
    fs::create_dir(&stage).unwrap();
    fake_tool(&stage, "caboodle", "if [ \"$1\" = --version ]; then echo 'caboodle 0.2.1'; else echo '--tool bobbin yupana desire-path'; fi");
    let name = "caboodle-v0.2.1-x86_64-unknown-linux-gnu.tar.gz";
    let archive = root.path().join(name);
    assert!(std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .arg("caboodle")
        .status()
        .unwrap()
        .success());
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()));
    fs::write(
        root.path().join(format!("{name}.sha256")),
        format!("{digest}  {name}\n"),
    )
    .unwrap();
    fs::write(root.path().join("latest.json"), serde_json::json!({"tag_name":"v0.2.1","draft":false,"prerelease":false,"assets":[{"name":name},{"name":format!("{name}.sha256")}]}).to_string()).unwrap();
    fake_tool(
        &bin,
        "curl",
        r#"
url=''; output=''
while [ "$#" -gt 0 ]; do
 case "$1" in --output|-o) shift; output=$1 ;; https://*) url=$1 ;; esac
 shift
done
case "$url" in */releases/latest) cat "$HOME/latest.json" ;; *) cp "$HOME/${url##*/}" "$output" ;; esac
"#,
    );
    command(root.path(), &bin)
        .arg("update-self")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "caboodle: installed and verified v0.2.1",
        ));
    assert!(fs::read_to_string(root.path().join(".caboodle/state.json"))
        .unwrap()
        .contains("caboodle 0.2.1"));
}

#[test]
fn doctor_reports_blockers_without_changing_anything() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let empty = root.path().join("empty-bin");
    fs::create_dir_all(&empty).unwrap();
    Command::cargo_bin("caboodle")
        .unwrap()
        .current_dir(root.path())
        .env_clear()
        .env("HOME", &home)
        .env("PATH", &empty)
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("no plan at caboodle-plan.toml"))
        .stdout(predicate::str::contains("FAIL install directory"))
        .stdout(predicate::str::contains("FAIL prerequisite curl"))
        .stdout(predicate::str::contains(
            "FAIL prerequisite go: not on PATH, needed by desire-path",
        ))
        .stdout(predicate::str::contains("blockers"));
    assert!(!home.exists(), "doctor must not create install directories");
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        1,
        "doctor must not write plan or state files"
    );
}

#[test]
fn missing_plan_points_at_the_interview() {
    let root = tempfile::tempdir().unwrap();
    Command::cargo_bin("caboodle")
        .unwrap()
        .current_dir(root.path())
        .arg("install")
        .assert()
        .failure()
        .stderr(predicate::str::contains("caboodle init --guided"));
}

/// aegis-70qlhs: a stale copy earlier on PATH must turn verify RED even when
/// the managed copy in `$CARGO_HOME/bin` is current. Versions are read from
/// the managed copy, so without this check verify reported the tool current
/// while the shell ran something else. A different file with the same version
/// is only a note.
#[test]
fn verify_refuses_a_stale_binary_that_shadows_the_managed_copy() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    let managed_dir = root.path().join("cargo-home").join("bin");
    fs::create_dir_all(&managed_dir).unwrap();
    let managed = managed_dir.join("yupana");
    fs::copy(bin.join("yupana"), &managed).unwrap();

    command(root.path(), &bin)
        .args(["plan", "--profile", "code-intel"])
        .assert()
        .success();
    // Same build at two paths: verified, with a note naming both.
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .success()
        .stdout(predicate::str::contains("yupana: verified"))
        .stdout(predicate::str::contains("both report the same version"));

    // The PATH copy goes stale; the managed copy stays current.
    fake_tool(
        &bin,
        "yupana",
        &format!(
            "if [ \"${{1:-}}\" = --version ]; then echo 'yupana 0.6.4'; exit 0; fi\nexec {} \"$@\"",
            managed.display()
        ),
    );
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("yupana is SHADOWED"))
        .stderr(predicate::str::contains("yupana 0.6.4"))
        .stderr(predicate::str::contains(managed.display().to_string()));
}

#[test]
fn project_settings_delegates_to_rig_without_an_install_plan() {
    let root = tempfile::tempdir().unwrap();
    fake_tool(
        root.path(),
        "st",
        r#"
printf '%s\n' "$@" > st-args
printf '%s\n' '{"version":1,"owner":"shantytown","agents":[{"name":"ada","harness":"claude","servers":["bobbin","yupana","forgejo","homelab"]}]}'
"#,
    );
    command(root.path(), root.path())
        .args([
            "project-settings",
            "--root",
            "rig with spaces",
            "--agent",
            "ada",
            "--registry",
            "quipu",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered: ada (4 MCP servers"));
    assert_eq!(
        fs::read_to_string(root.path().join("st-args")).unwrap(),
        "--root\nrig with spaces\n--registry\nquipu\nops\nprovision\n--json\n--\nada\n"
    );
    assert!(!root.path().join("caboodle-plan.toml").exists());
    assert!(!root.path().join("caboodle-settings").exists());
}

#[test]
fn project_settings_preserves_owner_refusal() {
    let root = tempfile::tempdir().unwrap();
    fake_tool(
        root.path(),
        "st",
        "echo 'no crew on this rig yet - run st fleet init / st agent new first' >&2\nexit 1",
    );
    command(root.path(), root.path())
        .arg("project-settings")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no crew on this rig yet"))
        .stdout(predicate::str::contains("registered:").not());
}

#[test]
fn project_settings_rejects_empty_or_invalid_success_receipts() {
    for receipt in [
        "not-json-secret-marker",
        r#"{"version":1,"owner":"shantytown","agents":[{"name":"ada","servers":["bobbin"]},{"name":"bad","servers":[]}]}"#,
        r#"{"version":1,"owner":"shantytown","agents":[]}"#,
        r#"{"version":2,"owner":"shantytown","agents":[{"name":"ada","servers":["bobbin"]}]}"#,
        r#"{"version":1,"owner":"other","agents":[{"name":"ada","servers":["bobbin"]}]}"#,
        r#"{"version":1,"owner":"shantytown","agents":[{"name":"ada","servers":[]}]}"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        fake_tool(root.path(), "st", &format!("printf '%s\\n' '{receipt}'"));
        command(root.path(), root.path())
            .arg("project-settings")
            .assert()
            .failure()
            .stdout(predicate::str::contains("registered:").not())
            .stderr(predicate::str::contains("secret-marker").not());
    }
}

#[test]
fn install_continues_after_failure_invalidates_old_proof_and_resumes() {
    let (root, bin) = retrieval_plan_with_bobbin("0.25.2", true);
    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success();
    fake_tool(&bin, "quipu", "echo 'broken quipu' >&2; exit 1");
    fake_tool(&bin, "curl", "echo 'artifact unavailable' >&2; exit 22");
    let config = root.path().join(".config/bobbin/config.toml");
    fs::write(&config, "# unchanged after partial apply\n").unwrap();
    command(root.path(), &bin)
        .args(["install"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("bobbin: applied"))
        .stdout(predicate::str::contains(": verified").not())
        .stderr(predicate::str::contains("1 tool(s) failed to apply"))
        .stderr(predicate::str::contains("artifact unavailable"));
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(".caboodle/state.json")).unwrap(),
    )
    .unwrap();
    assert!(state["tools"].get("quipu").is_none());
    assert_eq!(state["tools"]["bobbin"]["applied"], true);
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        "# unchanged after partial apply\n"
    );
    install_fakes(root.path(), &bin);
    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu: verified"))
        .stdout(predicate::str::contains("bobbin: verified"));
}

#[test]
fn apply_reports_all_version_failures_and_still_applies_later_tools() {
    let (root, bin) = retrieval_plan_with_bobbin("0.25.2", true);
    fake_tool(&bin, "quipu", "echo 'broken quipu' >&2; exit 1");
    fs::write(root.path().join("camayoc/REVISION"), "").unwrap();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("bobbin: applied"))
        .stderr(predicate::str::contains("2 tool(s) failed to apply"))
        .stderr(predicate::str::contains("quipu version read-back"))
        .stderr(predicate::str::contains("camayoc version read-back"));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn retrying_release_fixture(root: &Path, bin: &Path, bad_checksum: bool) {
    release_fixture(root, bin, false, bad_checksum);
    fs::rename(bin.join("curl"), bin.join("curl.fixture")).unwrap();
    fake_tool(
        bin,
        "curl",
        r#"
url=''; out=''; previous=''
for arg do
  if [ "$previous" = --output ]; then out=$arg; fi
  case "$arg" in https://*) url=$arg ;; esac
  previous=$arg
done
if [ -n "$out" ]; then
  case "$*" in *'--connect-timeout 10 --max-time 120'*) ;; *) exit 99 ;; esac
  asset=${url##*/}
  printf '%s\n' "$asset" >> "$HOME/fetches"
  case "$asset" in
    *"$FAKE_RETRY_ASSET"*)
      count=0
      [ ! -f "$HOME/attempts" ] || count=$(cat "$HOME/attempts")
      count=$((count + 1))
      printf '%s' "$count" > "$HOME/attempts"
      if [ "$count" -le "$FAKE_RETRY_FAILURES" ]; then
        # A partial body from the failed transfer must never reach unpacking.
        printf '%s' 'partial error response' > "$out"
        printf '%s' "$FAKE_RETRY_HTTP"
        echo 'fixture fetch failure' >&2
        exit "$FAKE_RETRY_CODE"
      fi
      ;;
  esac
fi
exec "$HOME/bin/curl.fixture" "$@"
"#,
    );
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn release_download_recovers_visibly_from_server_and_transport_errors() {
    for (code, http, asset) in [
        (22, "500", ".tar.gz"),
        (22, "504", "SHA256SUMS.txt"),
        (7, "000", ".tar.gz"),
        (18, "200", ".tar.gz"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        retrying_release_fixture(root.path(), &bin, false);
        command(root.path(), &bin)
            .env("FAKE_RETRY_ASSET", asset)
            .env("FAKE_RETRY_CODE", code.to_string())
            .env("FAKE_RETRY_HTTP", http)
            .env("FAKE_RETRY_FAILURES", "1")
            .args(["update-release", "--tool", "bobbin"])
            .assert()
            .success()
            .stderr(
                predicate::str::contains("attempt 1/3")
                    .and(predicate::str::contains("retrying in 1s"))
                    .and(predicate::str::contains("succeeded on attempt 2/3")),
            )
            .stdout(predicate::str::contains("installed and verified v0.25.3"));
        assert_eq!(
            fs::read_to_string(root.path().join("attempts")).unwrap(),
            "2"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("fetches"))
                .unwrap()
                .lines()
                .count(),
            3
        );
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn release_download_exhaustion_and_terminal_errors_preserve_installed_binary() {
    for (code, http, attempts, message) in [
        (22, "503", "3", "retry limit exhausted"),
        (28, "000", "3", "retry limit exhausted"),
        (22, "404", "1", "terminal failure"),
        (22, "403", "1", "terminal failure"),
        (60, "000", "1", "terminal failure"),
        (23, "200", "1", "terminal failure"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        retrying_release_fixture(root.path(), &bin, false);
        let before = fs::read(bin.join("bobbin")).unwrap();
        let result = command(root.path(), &bin)
            .env("FAKE_RETRY_ASSET", ".tar.gz")
            .env("FAKE_RETRY_CODE", code.to_string())
            .env("FAKE_RETRY_HTTP", http)
            .env("FAKE_RETRY_FAILURES", "9")
            .args(["update-release", "--tool", "bobbin"])
            .assert()
            .failure()
            .stderr(
                predicate::str::contains(message)
                    .and(predicate::str::contains("fixture fetch failure")),
            );
        if attempts == "1" {
            result.stderr(predicate::str::contains("retrying").not());
        } else {
            result.stderr(predicate::str::contains("retrying in 2s"));
        }
        assert_eq!(
            fs::read_to_string(root.path().join("attempts")).unwrap(),
            attempts
        );
        assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
        assert!(!root.path().join(".caboodle/state.json").exists());
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn release_download_recovery_never_retries_a_checksum_mismatch() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    retrying_release_fixture(root.path(), &bin, true);
    let before = fs::read(bin.join("bobbin")).unwrap();
    command(root.path(), &bin)
        .env("FAKE_RETRY_ASSET", "SHA256SUMS.txt")
        .env("FAKE_RETRY_CODE", "22")
        .env("FAKE_RETRY_HTTP", "504")
        .env("FAKE_RETRY_FAILURES", "1")
        .args(["update-release", "--tool", "bobbin"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("succeeded on attempt 2/3")
                .and(predicate::str::contains("SHA256 mismatch")),
        );
    assert_eq!(
        fs::read_to_string(root.path().join("fetches"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    assert_eq!(
        fs::read_to_string(root.path().join("attempts")).unwrap(),
        "2"
    );
    assert_eq!(fs::read(bin.join("bobbin")).unwrap(), before);
    assert!(!root.path().join(".caboodle/state.json").exists());
}

/// aegis-nvw6ye: Quipu MCP writes must carry a bearer supplied by a
/// headersHelper, never a secret stored in the Claude configuration.
fn quipu_mcp_fixture(root: &Path, bin: &Path) {
    install_fakes(root, bin);
    fs::rename(bin.join("curl"), bin.join("curl.fixture")).unwrap();
    fake_tool(
        bin,
        "curl",
        r#"
case "$*" in *"/mcp"*) ;; *) exec "$(dirname "$0")/curl.fixture" "$@" ;; esac
out=''; config=''; previous=''
for arg do
  if [ "$previous" = --output ]; then out=$arg; fi
  if [ "$previous" = --config ]; then config=$arg; fi
  previous=$arg
done
refused='data: {"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"error\":\"unauthorized\",\"reason\":\"missing_or_invalid_bearer_token\"}"}],"isError":true}}'
parsed='data: {"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"error\":\"RDF parse error: fixture\"}"}],"isError":true}}'
if [ "${FAKE_MCP_OPEN:-}" = 1 ]; then printf '%s\n' "$parsed" > "$out"
elif [ -n "$config" ] && grep -q 'Bearer good-token"' "$config"; then printf '%s\n' "$parsed" > "$out"
else printf '%s\n' "$refused" > "$out"; fi
"#,
    );
    fake_tool(
        bin,
        "claude",
        r#"
printf '%s\n' "$*" >> "$HOME/claude.log"
if [ "$1 $2" = "mcp add-json" ]; then
  if [ "${FAKE_CLAUDE_ADD_FAIL:-}" = all ]; then echo 'add refused' >&2; exit 1; fi
  case "$4" in *headersHelper*) if [ "${FAKE_CLAUDE_ADD_FAIL:-}" = 1 ]; then echo 'add refused' >&2; exit 1; fi ;; esac
  printf '{"mcpServers":{"%s":%s}}\n' "$3" "$4" > "$HOME/.claude.json"; exit 0
fi
if [ "$1 $2" = "mcp remove" ]; then printf '{"mcpServers":{}}\n' > "$HOME/.claude.json"; exit 0; fi
exit 2
"#,
    );
    fs::create_dir_all(root.join(".config/quipu")).unwrap();
    fs::write(root.join(".config/quipu/token"), "good-token\n").unwrap();
}

#[test]
fn quipu_mcp_install_provisions_a_headers_helper_and_verify_proves_the_write() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    // The Mac's broken state: a direct entry with no headersHelper.
    fs::write(
        root.path().join(".claude.json"),
        r#"{"mcpServers":{"quipu":{"type":"http","url":"http://quipu.example/mcp"}}}"#,
    )
    .unwrap();

    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .assert()
        .success();
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains("[quipu_mcp]"), "{plan}");

    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("had no headersHelper"))
        .stdout(predicate::str::contains(
            "quipu mcp: authenticated MCP write reached",
        ));

    let helper = root.path().join(".local/bin/quipu-mcp-headers");
    let mode = fs::metadata(&helper).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755);
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join(".claude.json")).unwrap())
            .unwrap();
    assert_eq!(
        config["mcpServers"]["quipu"]["url"],
        "http://quipu.example/mcp"
    );
    assert_eq!(
        config["mcpServers"]["quipu"]["headersHelper"],
        helper.to_str().unwrap()
    );
    // The secret is in neither file.
    for path in [
        &helper,
        &root.path().join(".claude.json"),
        &root.path().join("caboodle-plan.toml"),
    ] {
        assert!(
            !fs::read_to_string(path).unwrap().contains("good-token"),
            "{}",
            path.display()
        );
    }

    // Idempotent: a second apply registers nothing.
    let adds = |root: &Path| {
        fs::read_to_string(root.join("claude.log"))
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("mcp add-json"))
            .count()
    };
    assert_eq!(adds(root.path()), 1);
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("quipu mcp: already provisioned"));
    assert_eq!(adds(root.path()), 1);
}

#[test]
fn quipu_mcp_verify_fails_when_the_headers_helper_is_removed() {
    // The bead's success metric: verify must go red, not stay green, when the
    // entry reverts to a bare URL.
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success();
    fs::write(
        root.path().join(".claude.json"),
        r#"{"mcpServers":{"quipu":{"type":"http","url":"http://quipu.example/mcp"}}}"#,
    )
    .unwrap();
    command(root.path(), &bin)
        .args(["verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "FAIL: the user-scope quipu MCP entry has no headersHelper",
        ));

    // A shadowing local-scope entry without the helper is also red.
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success();
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join(".claude.json")).unwrap())
            .unwrap();
    config["projects"] = serde_json::json!({"/work": {"mcpServers": {"quipu": {"type": "http", "url": "http://quipu.example/mcp"}}}});
    fs::write(root.path().join(".claude.json"), config.to_string()).unwrap();
    command(root.path(), &bin)
        .args(["verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "local scope for /work has no headersHelper",
        ));
}

#[test]
fn quipu_mcp_verify_fails_on_a_refused_token_and_is_unknown_without_a_control() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success();

    fs::write(root.path().join(".config/quipu/token"), "wrong-token\n").unwrap();
    command(root.path(), &bin)
        .args(["verify"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("FAIL:").and(predicate::str::contains(
                "refused the headersHelper's bearer",
            )),
        );

    fs::write(root.path().join(".config/quipu/token"), "good-token\n").unwrap();
    command(root.path(), &bin)
        .args(["verify"])
        .env("FAKE_MCP_OPEN", "1")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("UNKNOWN:").and(predicate::str::contains("NO credential")),
        );

    fs::remove_file(root.path().join(".config/quipu/token")).unwrap();
    command(root.path(), &bin)
        .args(["verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL:").and(predicate::str::contains("no Quipu token")));
}

#[test]
fn quipu_mcp_plan_refuses_credentials_in_the_url() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    for bad in [
        "http://user:secret@quipu.example",
        "quipu.example",
        "http://quipu.example/mcp?token=x",
    ] {
        command(root.path(), &bin)
            .args(["plan", "--profile", "retrieval", "--quipu-mcp-url", bad])
            .assert()
            .failure();
    }
}

#[test]
fn quipu_mcp_restores_the_previous_entry_when_the_add_fails() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    let bare = r#"{"type":"http","url":"http://quipu.example/mcp"}"#;
    fs::write(
        root.path().join(".claude.json"),
        format!(r#"{{"mcpServers":{{"quipu":{bare}}}}}"#),
    )
    .unwrap();
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .env("FAKE_CLAUDE_ADD_FAIL", "1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("the previous entry was restored"));
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.path().join(".claude.json")).unwrap())
            .unwrap();
    assert_eq!(
        config["mcpServers"]["quipu"],
        serde_json::from_str::<serde_json::Value>(bare).unwrap()
    );
}

#[test]
fn quipu_mcp_bakes_the_planned_token_file_and_warns_on_shell_only_tokens() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    // This host keeps its token somewhere other than ~/.config/quipu/token.
    fs::remove_file(root.path().join(".config/quipu/token")).unwrap();
    let host_file = root.path().join("host-convention/quipu_token");
    fs::create_dir_all(host_file.parent().unwrap()).unwrap();
    fs::write(&host_file, "good-token\n").unwrap();

    // Taken from QUIPU_AUTH_TOKEN_FILE at plan time, visible in the plan.
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .env("QUIPU_AUTH_TOKEN_FILE", &host_file)
        .assert()
        .success();
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(plan.contains(host_file.to_str().unwrap()), "{plan}");
    // Claude Code's environment has no token variable: the baked default works.
    command(root.path(), &bin)
        .args(["install", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "quipu mcp: authenticated MCP write reached",
        ))
        .stdout(predicate::str::contains("WARNING").not());
    let helper = fs::read_to_string(root.path().join(".local/bin/quipu-mcp-headers")).unwrap();
    assert!(helper.contains(host_file.to_str().unwrap()) && !helper.contains("good-token"));

    // A plan WITHOUT the baked path, where only this shell's variable finds the
    // token, passes but says Claude Code must see that variable too.
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success();
    command(root.path(), &bin)
        .args(["verify"])
        .env("QUIPU_AUTH_TOKEN_FILE", &host_file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "WARNING: the helper found a token only through QUIPU_AUTH_TOKEN_FILE",
        ));
}

#[test]
fn quipu_mcp_errors_never_print_a_static_bearer() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    quipu_mcp_fixture(root.path(), &bin);
    let secret = "static-s3cr3t";
    let hand_made = format!(
        r#"{{"mcpServers":{{"quipu":{{"type":"http","url":"http://quipu.example/mcp","headers":{{"Authorization":"Bearer {secret}"}}}}}}}}"#
    );
    command(root.path(), &bin)
        .args([
            "plan",
            "--profile",
            "retrieval",
            "--quipu-mcp-url",
            "http://quipu.example",
        ])
        .assert()
        .success();

    // Both the planned add and the restore fail: the error names the entry.
    fs::write(root.path().join(".claude.json"), &hand_made).unwrap();
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .env("FAKE_CLAUDE_ADD_FAIL", "all")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "restoring the previous entry failed",
        ))
        .stderr(predicate::str::contains("<redacted>"))
        .stderr(predicate::str::contains(secret).not());

    // verify's "differs" message shows the found entry.
    let differs = format!(
        r#"{{"mcpServers":{{"quipu":{{"type":"http","url":"http://quipu.example/mcp","headersHelper":"/elsewhere","headers":{{"Authorization":"Bearer {secret}"}}}}}}}}"#
    );
    fs::write(root.path().join(".claude.json"), differs).unwrap();
    command(root.path(), &bin)
        .args(["verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("differs from the plan"))
        .stderr(predicate::str::contains(secret).not());
}

#[test]
fn doctor_warns_when_path_runs_a_different_caboodle() {
    // aegis-nvw6ye.1: an older copy earlier on PATH answered plain `caboodle`.
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    fake_tool(&bin, "caboodle", "echo 'caboodle 0.2.2 (stale0000000)'");
    command(root.path(), &bin)
        .arg("doctor")
        .assert()
        .stdout(predicate::str::contains("warn caboodle on PATH: PATH runs"))
        .stdout(predicate::str::contains("caboodle 0.2.2 (stale0000000)"));

    // The same build on PATH is fine.
    let real = assert_cmd::cargo::cargo_bin("caboodle");
    fs::remove_file(bin.join("caboodle")).unwrap();
    std::os::unix::fs::symlink(&real, bin.join("caboodle")).unwrap();
    command(root.path(), &bin)
        .arg("doctor")
        .assert()
        .stdout(predicate::str::contains("caboodle on PATH").not());
}

#[test]
fn version_names_the_commit() {
    let out = Command::cargo_bin("caboodle")
        .unwrap()
        .arg("--version")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let (_, rest) = text
        .trim()
        .split_once(" (")
        .expect("version carries a commit");
    assert!(rest.ends_with(')') && rest.len() > 1, "{text}");
}

/// The fixture member's program, extracted from the committed, digest-pinned
/// release tarball, onto `bin`.
#[cfg(feature = "fixture-members")]
fn install_fixture_member(bin: &Path) {
    let archive = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/releases/fixture-demo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz");
    let unpack = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(unpack.path())
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::copy(
        unpack.path().join("fixture-demo-0.1.0/fixture-demo"),
        bin.join("fixture-demo"),
    )
    .unwrap();
}

/// aegis-z1u9s0: apply must not report a member applied while an older copy
/// of the SAME member earlier on PATH is what the shell runs. A member's
/// version is read from the managed copy, so apply printed "converged" and
/// exited 0 while a plain `sd` still ran the previous release (measured on the
/// Mac). Identity checks pass a stale copy of the same program; only verify
/// compared what PATH runs, and apply is the step that says done.
#[cfg(feature = "fixture-members")]
#[test]
fn apply_refuses_a_stale_member_copy_that_shadows_the_managed_install() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(root.path(), &bin);
    let managed_dir = root.path().join("cargo-home").join("bin");
    fs::create_dir_all(&managed_dir).unwrap();
    let managed = managed_dir.join("fixture-demo");
    fs::rename(bin.join("fixture-demo"), &managed).unwrap();

    command(root.path(), &bin)
        .args(["plan", "--profile", "everything"])
        .assert()
        .success();
    // Control: no other copy on PATH, and apply succeeds.
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fixture-demo: applied"));

    // An older copy of the same member, earlier on PATH: same identity, other version.
    fake_tool(
        &bin,
        "fixture-demo",
        &format!(
            "if [ \"${{1:-}}\" = --version ]; then echo 'fixture-demo 0.0.9'; exit 0; fi\nexec {} \"$@\"",
            managed.display()
        ),
    );
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("fixture-demo: applied").not())
        .stderr(predicate::str::contains("fixture-demo is SHADOWED"))
        .stderr(predicate::str::contains("fixture-demo 0.0.9"))
        .stderr(predicate::str::contains(managed.display().to_string()));
}

#[test]
fn apply_registers_exactly_the_hook_bundles_of_the_plans_tools() {
    // aegis-u1ybxo scope (a): apply registers each shipped bundle whose tool the
    // plan installs, through st's generic `ops hooks register`, and no other.
    // Two hosts: a fresh one (the measured Mac) without the quipu session-capture
    // script, where quipu must NOT be registered, and a Gas Town host that has it.
    use std::os::unix::fs::PermissionsExt;
    for gas_town in [false, true] {
        let (root, bin) = retrieval_plan_with_bobbin("0.25.2", true);
        if gas_town {
            let hooks = root.path().join(".gt/hooks");
            fs::create_dir_all(&hooks).unwrap();
            let script = hooks.join("quipu-session-capture.sh");
            fs::write(&script, "#!/bin/sh\n").unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
        let out = command(root.path(), &bin)
            .args(["apply", "--skip-install"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let out = String::from_utf8(out).unwrap();
        let mut registered: Vec<String> = fs::read_to_string(bin.join("st-registered.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        registered.sort();
        let in_plan: Vec<String> = ["bobbin", "desire-path", "quipu", "yupana"]
            .into_iter()
            .filter(|t| plan.contains(&format!("\"{t}\"")))
            .map(str::to_string)
            .collect();
        assert!(
            in_plan.contains(&"bobbin".to_string()) && in_plan.contains(&"quipu".to_string()),
            "control: the plan installs bobbin and quipu\n{plan}"
        );
        let mut expected: Vec<String> = in_plan
            .iter()
            .filter(|t| gas_town || t.as_str() != "quipu")
            .cloned()
            .collect();
        expected.sort();
        assert_eq!(
            registered, expected,
            "gas_town={gas_town}: registered exactly the plan's applicable bundles"
        );
        for name in &expected {
            assert!(
                out.contains(&format!("hook bundle {name}: installed")),
                "{out}"
            );
        }
        if !gas_town {
            assert!(
                out.contains("quipu: NOT registered on this host")
                    && out.contains("$HOME/.gt/hooks/quipu-session-capture.sh"),
                "the skip is named, not silent\n{out}"
            );
        }
    }
}

#[test]
fn verify_asserts_the_hook_bundles_without_a_crew_plan_and_before_the_tools() {
    // aegis-u1ybxo: apply registers the plan's bundles whenever st is present,
    // crew mode or not, so verify must assert them on the same terms. It used to
    // assert them only inside crew verification, which a plan without a crew
    // section never reaches, and only after every tool had verified, so on the
    // measured Mac a bobbin skew failed first and the bundles went unasserted.
    let (root, bin) = retrieval_plan_with_bobbin("0.25.2", true);
    let plan = fs::read_to_string(root.path().join("caboodle-plan.toml")).unwrap();
    assert!(!plan.contains("[crew]"), "control: no crew section\n{plan}");
    fs::remove_file(bin.join("st-versions.log")).unwrap();

    // Before apply registered anything: verify FAILS naming the bundle.
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("bobbin: not registered with st"));

    // After apply: verify asserts each applicable bundle.
    command(root.path(), &bin)
        .args(["apply", "--skip-install"])
        .assert()
        .success();
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "hook bundle bobbin: configured in",
        ))
        .stdout(predicate::str::contains(
            "hook bundle quipu: not applicable on this host",
        ));

    // A skewed tool still fails verify, but the hooks verdict is printed first.
    fake_tool(
        &bin,
        "bobbin",
        "if [ \"${1:-}\" = --version ]; then echo 'bobbin 0.17.0'; exit 0; fi\nexit 101",
    );
    command(root.path(), &bin)
        .arg("verify")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "hook bundle bobbin: configured in",
        ))
        .stderr(predicate::str::contains("VERSION SKEW"));
}
