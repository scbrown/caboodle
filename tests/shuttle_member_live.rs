//! shuttle, the first python-wheel stack member, installed from its PUBLISHED
//! release and proven with a real run round trip (aegis-1i5h1j.2). Needs the
//! network (the wheel, and pip resolving cryptography), so it is ignored by
//! default:
//!
//!     cargo test --test shuttle_member_live -- --ignored
//!
//! Run it after every `bump-member members/shuttle.toml`.
#![cfg(unix)]

use std::{env, fs, os::unix::fs::PermissionsExt};

use caboodle::adapter::adapter;
use caboodle::model::{QuipuFlavor, ToolName};

#[test]
#[ignore = "downloads the published shuttle release and its dependency"]
fn shuttle_installs_from_its_published_release_and_runs_a_workflow() {
    let root = tempfile::tempdir().unwrap();
    let cargo_home = root.path().join("cargo-home");
    env::set_var("CARGO_HOME", &cargo_home);
    env::set_var("HOME", root.path());
    let system = env::var("PATH").unwrap();
    env::set_var(
        "PATH",
        format!("{}:{}", cargo_home.join("bin").display(), system),
    );

    let member = ToolName::parse("shuttle").expect("shuttle is an embedded member");
    let shuttle = adapter(member, QuipuFlavor::Release);
    shuttle
        .install()
        .expect("install from the reviewed release");
    let installed = cargo_home.join("bin/shuttle");
    assert!(installed.is_file(), "shuttle was not installed");
    let version = shuttle.version().expect("version read-back");
    assert!(
        shuttle.is_current(&version),
        "installed {version:?} is not the pinned version"
    );
    shuttle
        .verify()
        .expect("define -> start -> advance -> status round trip");

    // MUTANT: the same install, except `advance` signs and records nothing.
    // The run is then still `open`, so the read-back must refuse it. This is
    // what makes the green above mean "a transition happened".
    let real = root.path().join("real-shuttle");
    fs::rename(&installed, &real).unwrap();
    let mutant = root.path().join("mutant");
    fs::create_dir_all(&mutant).unwrap();
    let script = mutant.join("shuttle");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\n[ \"$1\" = advance ] && exit 0\nexec {} \"$@\"\n",
            real.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    env::set_var("PATH", format!("{}:{}", mutant.display(), system));
    let refused = format!("{:#}", shuttle.verify().expect_err("mutant must fail"));
    assert!(
        refused.contains("not present after it was created"),
        "{refused}"
    );
    env::set_var("PATH", system);
}
