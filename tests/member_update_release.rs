//! `update-release --tool <member>` tracks the member's REVIEWED PIN
//! (aegis-2ezq5j). Driven through the CLI against the committed fixture-demo
//! release, with a fake curl serving it.
#![cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    feature = "fixture-members"
))]

use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;

const RELEASE: &str = "tests/fixtures/releases/fixture-demo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz";

fn script(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A fixture-demo at `version` that is otherwise a working member.
fn demo(dir: &Path, version: &str) {
    script(
        dir,
        "fixture-demo",
        &format!(
            r#"store="$HOME/.fixture-demo"
case "$1" in
  --version) echo "fixture-demo {version}" ;;
  --help) echo "fixture-demo is the caboodle fixture stack member" ;;
  list) cat "$store" 2>/dev/null; exit 0 ;;
  add) echo "$2" >> "$store" ;;
esac"#
        ),
    );
}

struct Box_ {
    root: tempfile::TempDir,
}

impl Box_ {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let fakes = root.path().join("fakes");
        script(
            &fakes,
            "curl",
            r#"out=''
for a in "$@"; do last=$a; done
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output" ]; then shift; out=$1; fi
  shift
done
printf '%s\n' "$last" >> "$HOME/curl.log"
cp "$FAKE_RELEASE" "$out"
printf 200"#,
        );
        fs::write(
            root.path().join("plan.toml"),
            "schema_version = 1\nprofile = \"everything\"\n\
             tools = [\"quipu\", \"camayoc\", \"bobbin\", \"yupana\", \"desire-path\", \"fixture-demo\"]\n",
        )
        .unwrap();
        Self { root }
    }
    fn path(&self) -> &Path {
        self.root.path()
    }
    fn bin(&self) -> std::path::PathBuf {
        self.path().join("bin")
    }
    fn installed(&self) -> std::path::PathBuf {
        self.bin().join("fixture-demo")
    }
    fn run(&self, release: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
        let path = format!(
            "{}:{}:{}",
            self.path().join("fakes").display(),
            self.bin().display(),
            std::env::var("PATH").unwrap()
        );
        Command::cargo_bin("caboodle")
            .unwrap()
            .current_dir(self.path())
            .env("HOME", self.path())
            .env("CARGO_HOME", self.path().join("cargo-home"))
            .env("PATH", path)
            .env("FAKE_RELEASE", release)
            .args([
                "update-release",
                "--tool",
                "fixture-demo",
                "--plan",
                "plan.toml",
            ])
            .args(extra)
            .assert()
    }
}

#[test]
fn a_member_tracks_its_reviewed_pin_in_place() {
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join(RELEASE);
    let b = Box_::new();
    demo(&b.bin(), "0.0.9");
    let old = fs::read(b.installed()).unwrap();

    // --check reports and touches nothing.
    b.run(&release, &["--check"])
        .success()
        .stdout(predicate::str::contains(
            "reviewed pin 0.1.0, installed fixture-demo 0.0.9; install/functional proof not run",
        ));
    assert_eq!(fs::read(b.installed()).unwrap(), old);

    // Behind the pin: the PATH copy is replaced by the pinned release, verified,
    // backed up, and recorded.
    b.run(&release, &[])
        .success()
        .stdout(predicate::str::contains(
            "fixture-demo: installed and verified reviewed pin 0.1.0 (was fixture-demo 0.0.9)",
        ));
    let pinned = fs::read(b.installed()).unwrap();
    assert!(String::from_utf8_lossy(&pinned).contains("fixture-demo 0.1.0"));
    assert!(
        !b.path().join("cargo-home/bin/fixture-demo").exists(),
        "the copy PATH runs is updated in place, not shadowed by a second copy"
    );
    let backups = b.path().join(".caboodle/release-backups/fixture-demo");
    assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);
    let state = fs::read_to_string(b.path().join(".caboodle/state.json")).unwrap();
    assert!(state.contains("fixture-demo 0.1.0"), "{state}");

    // At the pin: a fresh functional proof every run, no download.
    fs::remove_file(b.path().join("curl.log")).unwrap();
    b.run(&release, &[])
        .success()
        .stdout(predicate::str::contains(
            "fixture-demo: current and verified (reviewed pin 0.1.0",
        ));
    assert!(
        !b.path().join("curl.log").exists(),
        "a current member downloads nothing"
    );

    // Ahead of the pin: refused as a downgrade, untouched, NOT reported verified.
    demo(&b.bin(), "0.2.0");
    let ahead = fs::read(b.installed()).unwrap();
    b.run(&release, &[]).success().stdout(
        predicate::str::contains("ahead of reviewed pin 0.1.0")
            .and(predicate::str::contains("verified").not()),
    );
    assert_eq!(fs::read(b.installed()).unwrap(), ahead);
}

#[test]
fn a_foreign_program_or_a_tampered_asset_is_refused_untouched() {
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join(RELEASE);
    let b = Box_::new();

    // A same-named program that is not the member is never replaced (A2).
    script(
        &b.bin(),
        "fixture-demo",
        r#"case "$1" in --version) echo 'fixture-demo 0.0.1' ;; *) echo 'find and replace' ;; esac"#,
    );
    let foreign = fs::read(b.installed()).unwrap();
    b.run(&release, &[])
        .failure()
        .stderr(predicate::str::contains(
            "refusing to update it as fixture-demo",
        ));
    assert_eq!(fs::read(b.installed()).unwrap(), foreign);

    // An asset that does not hash to the RECORDED digest installs nothing.
    demo(&b.bin(), "0.0.9");
    let old = fs::read(b.installed()).unwrap();
    let tampered = b.path().join("tampered.tar.gz");
    let mut bytes = fs::read(&release).unwrap();
    bytes.push(0);
    fs::write(&tampered, bytes).unwrap();
    b.run(&tampered, &[])
        .failure()
        .stderr(predicate::str::contains("release checksum mismatch"));
    assert_eq!(fs::read(b.installed()).unwrap(), old);
}

#[test]
fn a_failed_verify_after_the_swap_restores_the_previous_copy() {
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join(RELEASE);
    let b = Box_::new();
    demo(&b.bin(), "0.0.9");
    let old = fs::read(b.installed()).unwrap();
    // The REVIEWED asset swaps in, then its verify fails: the pinned program
    // reads its store with `cat`, and a `cat` that always fails is first on the
    // PATH the hermetic verify passes through. Version read-back still passes.
    script(&b.path().join("fakes"), "cat", "exit 1");
    b.run(&release, &[])
        .failure()
        .stderr(predicate::str::contains("previous artifact restored"));
    assert_eq!(
        fs::read(b.installed()).unwrap(),
        old,
        "the pre-update copy must be back in place"
    );
    let state = b.path().join(".caboodle/state.json");
    assert!(
        !state.exists()
            || !fs::read_to_string(&state)
                .unwrap()
                .contains("fixture-demo 0.1.0"),
        "a failed update must not be recorded as verified"
    );
}
