#![cfg(unix)]
use caboodle::{
    adapter::{adapter_for_plan, path_resolution, PathResolution},
    model::{Plan, Profile, ToolName},
};
use std::{env, fs, os::unix::fs::PermissionsExt, path::Path};
fn tool(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let file = dir.join(name);
    fs::write(&file, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
fn external_identity_roundtrip_and_ownership_controls() {
    let root = tempfile::tempdir().unwrap();
    let external = root.path().join("external");
    let cargo = root.path().join("cargo");
    let managed = cargo.join("bin");
    let body = r#"
if [ "$1" = --version ]; then echo 'quipu 0.11.1'; exit; fi
verb=$1; shift
query=$1; shift
while [ $# -gt 0 ]; do
 if [ "$1" = --db ]; then db=$2; break; fi
 shift
done
case "$verb" in
 episode) touch "$db.marker" ;;
 knot) touch "$db.vocab" ;;
 read)
  case "$query" in
   *CaboodleVerifyAbsentTerm*) echo false ;;
   ASK*) if [ -f "$db.vocab" ]; then echo true; else echo false; fi ;;
   *) if [ -f "$db.marker" ]; then echo caboodle-verify-roundtrip; fi ;;
  esac ;;
esac
"#;
    tool(&external, "quipu", body);
    tool(&external, "quipu-server", "echo 'quipu-server 0.11.1'");
    tool(&managed, "quipu", "echo 'quipu 0.3.27'");
    tool(&managed, "quipu-server", "echo 'quipu-server 0.3.27'");
    let original_path = env::var_os("PATH").unwrap();
    let path = env::join_paths([
        external.clone(),
        managed.clone(),
        Path::new("/usr/bin").to_path_buf(),
        Path::new("/bin").to_path_buf(),
    ])
    .unwrap();
    env::set_var("PATH", &path);
    env::set_var("CARGO_HOME", &cargo);
    env::set_var(
        "CABOODLE_QUECHUA_FILE",
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/vocabulary/quechua-ns-v0.1.0.ttl"),
    );
    let mut plan = Plan::for_profile(Profile::Kg);
    assert!(!plan.external_quipu);
    assert!(!toml::to_string(&plan).unwrap().contains("external_quipu"));
    assert!(matches!(
        path_resolution("quipu", Some(&path)),
        PathResolution::Shadowed { .. }
    ));
    plan.external_quipu = true;
    let adapter = adapter_for_plan(ToolName::Quipu, &plan).unwrap();
    let identity = adapter.version().unwrap();
    assert!(identity.contains("quipu 0.11.1"));
    assert!(identity.contains(external.join("quipu").to_str().unwrap()));
    assert!(identity.contains(external.join("quipu-server").to_str().unwrap()));
    assert!(adapter
        .install()
        .unwrap_err()
        .to_string()
        .contains("installation refused"));
    assert!(caboodle::release_update::update(
        &plan,
        ToolName::Quipu,
        &root.path().join("state.json"),
        false
    )
    .unwrap_err()
    .to_string()
    .contains("release update refused"));
    // PATH moves after resolution: identity and all functional probes remain pinned.
    env::set_var("PATH", "/usr/bin:/bin");
    fs::remove_file(managed.join("quipu")).unwrap();
    adapter.verify().unwrap();
    tool(
        &external,
        "quipu",
        "case \"$1\" in --version) echo 'quipu 0.11.1';; read) echo false;; esac",
    );
    assert!(adapter
        .verify()
        .unwrap_err()
        .to_string()
        .contains("read-back did not find"));
    env::set_var("PATH", "/usr/bin:/bin");
    assert!(adapter_for_plan(ToolName::Quipu, &plan).is_err());
    env::set_var("PATH", original_path);
    env::remove_var("CARGO_HOME");
    env::remove_var("CABOODLE_QUECHUA_FILE");
}

#[test]
#[ignore = "requires externally installed Quipu and the reviewed vocabulary cache"]
fn live_external_quipu_roundtrip() {
    let mut plan = Plan::for_profile(Profile::Kg);
    plan.external_quipu = true;
    let adapter = adapter_for_plan(ToolName::Quipu, &plan).unwrap();
    println!("{}", adapter.version().unwrap());
    adapter
        .verify()
        .expect("external Quipu must pass isolated round trips");
}
