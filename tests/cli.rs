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
    command.current_dir(root).env("PATH", path_with(bin));
    command
}

fn install_fakes(bin: &Path) {
    fake_tool(
        bin,
        "quipu",
        r#"
if [ "${1:-}" = "--version" ]; then echo 'quipu 0.test'; exit 0; fi
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
}

#[test]
fn plan_install_verify_is_resumable() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(&bin);

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
fn verify_names_the_failing_adapter() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    install_fakes(&bin);
    fake_tool(
        &bin,
        "quipu",
        "if [ \"${1:-}\" = --version ]; then echo 'quipu broken'; exit 0; fi\nexit 9",
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
