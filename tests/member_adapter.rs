//! The data-declared member adapter, driven against a fake release (aegis-1i5h1j C1).
//!
//! These arms mutate PATH and CARGO_HOME, which are process-global, so they run
//! in ONE test function, in order, and never beside each other.
// The fixture release records a digest for x86_64 Linux only.
#![cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    feature = "fixture-members"
))]

use std::{env, fs, os::unix::fs::PermissionsExt, path::Path};

use caboodle::adapter::adapter;
use caboodle::model::{QuipuFlavor, ToolName};

const RELEASE: &str = "tests/fixtures/releases/fixture-demo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz";

fn script(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// PATH = the given dirs, then the system PATH (for tar and sha256sum).
fn set_path(system: &str, dirs: &[&Path]) {
    let joined = env::join_paths(
        dirs.iter()
            .map(|d| d.to_path_buf())
            .chain(env::split_paths(system)),
    )
    .unwrap();
    env::set_var("PATH", joined);
}

fn err(result: anyhow::Result<()>) -> String {
    format!("{:#}", result.expect_err("expected a refusal"))
}

#[test]
fn a_member_installs_from_its_reviewed_release_and_refuses_everything_else() {
    let system = env::var("PATH").unwrap();
    let root = tempfile::tempdir().unwrap();
    let fakes = root.path().join("fakes");
    let cargo_home = root.path().join("cargo-home");
    let installed = cargo_home.join("bin/fixture-demo");
    let log = root.path().join("curl.log");
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join(RELEASE);
    env::set_var("CARGO_HOME", &cargo_home);
    env::set_var("HOME", root.path());
    env::set_var("FAKE_CURL_LOG", &log);
    env::set_var("FAKE_RELEASE", &release);
    // Serves $FAKE_RELEASE for any URL and records the URL it was asked for.
    script(
        &fakes,
        "curl",
        r#"out=''
for a in "$@"; do last=$a; done
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output" ]; then shift; out=$1; fi
  shift
done
printf '%s\n' "$last" >> "$FAKE_CURL_LOG"
cp "$FAKE_RELEASE" "$out"
printf 200"#,
    );
    let member = ToolName::parse("fixture-demo").unwrap();
    let demo = adapter(member, QuipuFlavor::Release);

    // 1. Absent -> install from the reviewed release, then version and verify.
    set_path(&system, &[&fakes, &cargo_home.join("bin")]);
    assert!(
        demo.version().is_err(),
        "control: not installed before install"
    );
    demo.install().expect("install from the fake release");
    assert!(installed.is_file());
    assert_eq!(
        fs::read_to_string(&log).unwrap().trim(),
        "https://github.com/caboodle-fixtures/fixture-demo/releases/download/v0.1.0/\
         fixture-demo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    );
    let version = demo.version().unwrap();
    assert_eq!(version, "fixture-demo 0.1.0");
    assert!(demo.is_current(&version));
    assert!(!demo.is_current("fixture-demo 0.0.9"));
    demo.verify().expect("verify the installed member");
    assert!(
        !root.path().join(".fixture-demo").exists(),
        "verify must run in a hermetic HOME, never the user's"
    );

    // 2. A tampered asset is refused on the RECORDED digest and installs nothing.
    fs::remove_file(&installed).unwrap();
    let tampered = root.path().join("tampered.tar.gz");
    let mut bytes = fs::read(&release).unwrap();
    bytes.push(0);
    fs::write(&tampered, bytes).unwrap();
    env::set_var("FAKE_RELEASE", &tampered);
    assert!(err(demo.install()).contains("release checksum mismatch"));
    assert!(!installed.exists());
    env::set_var("FAKE_RELEASE", &release);

    // 3. A different program of the same name on PATH is not replaced (A2).
    let foreign = root.path().join("foreign");
    script(&foreign, "fixture-demo", "echo 'sd 1.0.0'");
    set_path(&system, &[&fakes, &foreign, &cargo_home.join("bin")]);
    assert!(err(demo.install()).contains("a different `fixture-demo` is already on PATH"));
    assert!(!installed.exists());

    // 4. Verify is red when the created marker never appears. (Its absent-first
    //    control cannot be tripped end to end, since the marker is fresh per run;
    //    the schema requiring it is unit-tested in members.rs.)
    let lossy = root.path().join("lossy");
    script(
        &lossy,
        "fixture-demo",
        r#"case "$1" in --version) echo 'fixture-demo 0.1.0' ;; *) exit 0 ;; esac"#,
    );
    set_path(&system, &[&lossy]);
    assert!(err(demo.verify()).contains("not present after it was created"));

    // 5. The reviewed-update guard refuses an installed member at or ahead of
    //    the pin (malcolm C1), and passes one behind it (control).
    let ahead = root.path().join("ahead");
    script(
        &ahead,
        "fixture-demo",
        r#"[ "$1" = --version ] && echo 'fixture-demo 0.2.0'"#,
    );
    set_path(&system, &[&ahead]);
    let refused = err(caboodle::release_update::guard_reviewed_update(
        member,
        &demo.desired_version(),
    ));
    assert!(
        refused.contains("refusing reviewed-pin downgrade"),
        "{refused}"
    );
    let behind = root.path().join("behind");
    script(
        &behind,
        "fixture-demo",
        r#"[ "$1" = --version ] && echo 'fixture-demo 0.0.9'"#,
    );
    set_path(&system, &[&behind]);
    caboodle::release_update::guard_reviewed_update(member, &demo.desired_version())
        .expect("a member behind the pin may converge");

    env::set_var("PATH", system);
}
