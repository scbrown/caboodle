//! Explicit release acceptance in a separate process with an isolated home.
#![cfg(unix)]

use caboodle::adapter::adapter;
use caboodle::model::{QuipuFlavor, ToolName};
use std::{env, os::unix::fs::PermissionsExt};

#[test]
#[ignore = "downloads and executes the published native Yupana release"]
fn yupana_installs_and_proves_a_real_caller() {
    let root = tempfile::tempdir().unwrap();
    let cargo_home = root.path().join("cargo");
    env::set_var("HOME", root.path());
    env::set_var("CARGO_HOME", &cargo_home);
    env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
    env::set_var("XDG_STATE_HOME", root.path().join("state"));
    env::set_var("XDG_CACHE_HOME", root.path().join("cache"));
    env::set_var(
        "PATH",
        format!(
            "{}:{}",
            cargo_home.join("bin").display(),
            env::var("PATH").unwrap()
        ),
    );
    let yupana = adapter(ToolName::Yupana, QuipuFlavor::Release);
    yupana.install().unwrap();
    let binary = cargo_home.join("bin/yupana");
    assert_ne!(binary.metadata().unwrap().permissions().mode() & 0o111, 0);
    let version = yupana.version().unwrap();
    assert!(yupana.is_current(&version), "unexpected version: {version}");
    yupana.verify().unwrap();
}
