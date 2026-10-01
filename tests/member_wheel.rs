//! The python-wheel member kind, driven against a committed fixture wheel
//! (aegis-1i5h1j.2). PATH, HOME and CARGO_HOME are process-global, so every
//! arm runs in ONE test function, in order.
#![cfg(all(unix, feature = "fixture-members"))]

use std::{env, fs, os::unix::fs::PermissionsExt, path::Path};

use caboodle::adapter::adapter;
use caboodle::model::{QuipuFlavor, ToolName};

const WHEEL: &str = "tests/fixtures/releases/fixture_wheel-0.1.0-py3-none-any.whl";

fn script(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

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
fn a_wheel_member_installs_into_its_own_venv_and_verifies_through_stdin() {
    let system = env::var("PATH").unwrap();
    let root = tempfile::tempdir().unwrap();
    let fakes = root.path().join("fakes");
    let cargo_home = root.path().join("cargo-home");
    let installed = cargo_home.join("bin/fixture-wheel");
    let venv_parent = root
        .path()
        .join(".local/share/caboodle/members/fixture-wheel");
    let log = root.path().join("curl.log");
    let wheel = Path::new(env!("CARGO_MANIFEST_DIR")).join(WHEEL);
    env::set_var("CARGO_HOME", &cargo_home);
    env::set_var("HOME", root.path());
    env::set_var("FAKE_CURL_LOG", &log);
    env::set_var("FAKE_RELEASE", &wheel);
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
    let member = ToolName::parse("fixture-wheel").unwrap();
    let demo = adapter(member, QuipuFlavor::Release);
    set_path(&system, &[&fakes, &cargo_home.join("bin")]);

    // 1. Absent -> install the ONE host-independent wheel, then version + verify.
    assert!(demo.version().is_err(), "control: not installed yet");
    demo.install().expect("install the fixture wheel");
    assert_eq!(
        fs::read_to_string(&log).unwrap().trim(),
        "https://github.com/caboodle-fixtures/fixture-wheel/releases/download/v0.1.0/\
         fixture_wheel-0.1.0-py3-none-any.whl"
    );
    assert!(installed.is_file());
    let venvs: Vec<_> = fs::read_dir(&venv_parent).unwrap().collect();
    assert_eq!(venvs.len(), 1);
    assert!(
        venvs[0]
            .as_ref()
            .unwrap()
            .path()
            .join("bin/python")
            .exists(),
        "the venv lives under the caboodle-owned members dir"
    );
    let version = demo.version().unwrap();
    assert_eq!(version, "fixture-wheel 0.1.0");
    assert!(demo.is_current(&version));
    demo.verify()
        .expect("verify, with the marker delivered on stdin");
    assert!(
        !root.path().join(".fixture-wheel").exists(),
        "verify must run in a hermetic HOME, never the user's"
    );

    // 2. Re-install builds a NEW venv beside the live one and swaps the script.
    let members = root
        .path()
        .join(".local/share/caboodle/members/fixture-wheel");
    demo.install().expect("reinstall beside the existing venv");
    assert_eq!(demo.version().unwrap(), "fixture-wheel 0.1.0");
    assert_eq!(
        fs::read_dir(&members).unwrap().count(),
        2,
        "old venv kept for rollback"
    );

    // 2b. dearing #59: a reinstall whose venv build FAILS leaves the live venv
    //     and the installed script untouched, and leaves no half-built dir.
    let live_script = fs::read(&installed).unwrap();
    let broken = root.path().join("broken-python");
    let real = String::from_utf8(
        std::process::Command::new("sh")
            .args(["-c", "command -v python3"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    script(
        &broken,
        "python3",
        &format!(
            r#"if [ "$1" = -m ] && [ "$2" = venv ]; then mkdir -p "$3/bin"; exit 1; fi
exec {} "$@""#,
            real.trim()
        ),
    );
    set_path(&system, &[&fakes, &broken, &cargo_home.join("bin")]);
    let failed = err(demo.install());
    assert!(failed.contains("venv"), "{failed}");
    assert_eq!(
        fs::read(&installed).unwrap(),
        live_script,
        "live script replaced"
    );
    assert_eq!(
        fs::read_dir(&members).unwrap().count(),
        2,
        "a half-built venv was left"
    );
    assert_eq!(demo.version().unwrap(), "fixture-wheel 0.1.0");
    demo.verify().expect("the live install still works");
    set_path(&system, &[&fakes, &cargo_home.join("bin")]);

    // 3. A tampered wheel is refused on the RECORDED digest; nothing installs.
    fs::remove_file(&installed).unwrap();
    let tampered = root.path().join("tampered.whl");
    let mut bytes = fs::read(&wheel).unwrap();
    bytes.push(0);
    fs::write(&tampered, bytes).unwrap();
    env::set_var("FAKE_RELEASE", &tampered);
    assert!(err(demo.install()).contains("release checksum mismatch"));
    assert!(!installed.exists());
    env::set_var("FAKE_RELEASE", &wheel);

    // 4. MUTANT: a member that ignores stdin stores nothing, and verify is red.
    //    This is the arm that proves stdin reached the program in arm 1.
    let deaf = root.path().join("deaf");
    script(
        &deaf,
        "fixture-wheel",
        r#"case "$1" in
  --help) echo 'fixture-wheel is the caboodle python-wheel fixture stack member' ;;
  list) cat "$HOME/.fixture-wheel" 2>/dev/null; true ;;
  add) echo "$2" >> "$HOME/.fixture-wheel" ;;
esac"#,
    );
    set_path(&system, &[&deaf]);
    let deaf_err = err(demo.verify());
    assert!(
        deaf_err.contains("not present after it was created"),
        "{deaf_err}"
    );

    env::set_var("PATH", system);
}
