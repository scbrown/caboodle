//! The pinned Quechua vocabulary (aegis-1i5h1j.3): verify must prove the term
//! resolves AND that its controls can fail. Env is process-global, so the
//! fake-quipu arms run in one test function.
#![cfg(unix)]

use std::{env, fs, os::unix::fs::PermissionsExt, path::Path};

const FIXTURE: &str = "tests/fixtures/vocabulary/quechua-ns-v0.1.0.ttl";

fn script(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn err(result: anyhow::Result<()>) -> String {
    format!("{:#}", result.expect_err("expected a refusal"))
}

#[test]
fn verify_refuses_a_tampered_file_and_a_quipu_that_cannot_tell() {
    let system = env::var("PATH").unwrap();
    let root = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);

    // 1. A cached file that does not hash to the pin is refused before any load.
    let tampered = root.path().join("tampered.ttl");
    let mut bytes = fs::read(&fixture).unwrap();
    bytes.extend_from_slice(b"\n# changed\n");
    fs::write(&tampered, bytes).unwrap();
    env::set_var("CABOODLE_QUECHUA_FILE", &tampered);
    assert!(err(caboodle::vocabulary::verify()).contains("does not hash to the reviewed"));
    env::set_var("CABOODLE_QUECHUA_FILE", &fixture);

    // 2. MUTANT: a quipu whose knot stores nothing. The term never resolves.
    let lossy = root.path().join("lossy");
    script(
        &lossy,
        "quipu",
        r#"case "$1" in knot) exit 0 ;; read) echo false ;; esac"#,
    );
    env::set_var("PATH", format!("{}:{system}", lossy.display()));
    assert!(err(caboodle::vocabulary::verify()).contains("owl:versionInfo"));

    // 3. MUTANT: a quipu that answers true to every ASK. Control 1 (an empty
    //    store must not declare the term) catches it before anything loads.
    let yes = root.path().join("yes");
    script(
        &yes,
        "quipu",
        r#"case "$1" in knot) exit 0 ;; read) echo true ;; esac"#,
    );
    env::set_var("PATH", format!("{}:{system}", yes.display()));
    assert!(err(caboodle::vocabulary::verify()).contains("control failed"));

    env::set_var("PATH", system);
    env::remove_var("CABOODLE_QUECHUA_FILE");
}

/// Against the REAL quipu, when one is on PATH. CI's clean-machine job runs
/// the same proof through `caboodle install`.
#[test]
#[ignore = "needs a real quipu on PATH"]
fn the_pinned_release_resolves_a_term_in_a_real_quipu() {
    env::set_var(
        "CABOODLE_QUECHUA_FILE",
        Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE),
    );
    caboodle::vocabulary::verify().expect("real quipu round trip");
}
