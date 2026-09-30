//! seeds, the first real stack member, installed from its PUBLISHED release and
//! proven (aegis-1i5h1j .10). Needs the network, so it is ignored by default:
//!
//!     cargo test --test seeds_member_live -- --ignored
//!
//! Run it after every `bump-member members/seeds.toml`: it is the same path
//! `caboodle apply` + `verify` take for this member, against the real asset.
#![cfg(target_os = "linux")]

use std::env;

use caboodle::adapter::adapter;
use caboodle::model::{QuipuFlavor, ToolName};

#[test]
#[ignore = "downloads the published seeds release"]
fn seeds_installs_from_its_published_release_and_verifies() {
    let root = tempfile::tempdir().unwrap();
    let cargo_home = root.path().join("cargo-home");
    env::set_var("CARGO_HOME", &cargo_home);
    env::set_var("HOME", root.path());
    // The managed bin dir first, so version() reads the member just installed
    // rather than any `sd` already on this machine.
    let system = env::var("PATH").unwrap();
    env::set_var(
        "PATH",
        format!("{}:{}", cargo_home.join("bin").display(), system),
    );

    let member = ToolName::parse("seeds").expect("seeds is an embedded member");
    let seeds = adapter(member, QuipuFlavor::Release);
    seeds.install().expect("install from the reviewed release");
    assert!(cargo_home.join("bin/sd").is_file(), "sd was not installed");
    let version = seeds.version().expect("version read-back");
    assert!(
        seeds.is_current(&version),
        "installed {version:?} is not the pinned version"
    );
    seeds.verify().expect("hermetic create/list round trip");
}
