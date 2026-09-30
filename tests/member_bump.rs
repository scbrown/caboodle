//! `caboodle bump-member` against a fake release (wu M2, aegis-1i5h1j).
// Bump proves the host target, and the fixture pins only x86_64 Linux.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command as StdCommand};

use assert_cmd::Command;
use predicates::prelude::*;

const ASSET: &str = "fixture-demo-v0.2.0-x86_64-unknown-linux-gnu.tar.gz";

struct Fake {
    root: tempfile::TempDir,
}

impl Fake {
    /// A manifest pinned at 0.1.0 and a release `tag` whose assets are served
    /// from `served/`. The fake curl answers the API with `release.json` and
    /// every download from `served/<name>`, and logs each URL.
    fn new(tag: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.path().join("served")).unwrap();
        let curl = bin.join("curl");
        fs::write(
            &curl,
            r#"#!/bin/sh
out=''; url=''
while [ "$#" -gt 0 ]; do
  case "$1" in --output) shift; out=$1 ;; https://*) url=$1 ;; esac
  shift
done
printf '%s\n' "$url" >> "$FAKE_ROOT/curl.log"
case "$url" in
  */releases/latest|*/releases/tags/*) cat "$FAKE_ROOT/release.json" ;;
  *) cp "$FAKE_ROOT/served/${url##*/}" "$out" 2>/dev/null || { printf 404; exit 22; }; printf 200 ;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
        let fake = Fake { root };
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/members/fixture-demo.toml"),
            fake.manifest(),
        )
        .unwrap();
        fake.publish(tag, &good_program(tag));
        fake
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(name)
    }

    fn manifest(&self) -> std::path::PathBuf {
        self.path("fixture-demo.toml")
    }

    /// Publish `tag`: a release tarball whose `fixture-demo` runs `program`,
    /// and a sums file that is correct for it.
    fn publish(&self, tag: &str, program: &str) {
        let asset = format!("fixture-demo-{tag}-x86_64-unknown-linux-gnu.tar.gz");
        let stage = self.path("stage");
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(stage.join("fixture-demo")).unwrap();
        let exe = stage.join("fixture-demo/fixture-demo");
        fs::write(&exe, format!("#!/bin/sh\n{program}\n")).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let status = StdCommand::new("tar")
            .arg("-czf")
            .arg(self.path("served").join(&asset))
            .arg("-C")
            .arg(&stage)
            .arg("fixture-demo")
            .status()
            .unwrap();
        assert!(status.success());
        fs::write(
            self.path("served/SHA256SUMS.txt"),
            format!("{}  {asset}\n", sha256(&self.path("served").join(&asset))),
        )
        .unwrap();
        fs::write(
            self.path("release.json"),
            format!(
                r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{{"name":"SHA256SUMS.txt"}},{{"name":"{asset}"}}]}}"#
            ),
        )
        .unwrap();
    }

    fn bump(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let path = format!(
            "{}:{}",
            self.path("bin").display(),
            std::env::var("PATH").unwrap()
        );
        Command::cargo_bin("caboodle")
            .unwrap()
            .current_dir(self.root.path())
            .env("PATH", path)
            .env("FAKE_ROOT", self.root.path())
            .env("HOME", self.path("home"))
            .arg("bump-member")
            .arg(self.manifest())
            .args(args)
            .assert()
    }

    fn log(&self) -> String {
        fs::read_to_string(self.path("curl.log")).unwrap_or_default()
    }
}

/// A release program that answers the fixture's identity and `tag`'s version.
fn good_program(tag: &str) -> String {
    format!(
        "case \"$1\" in --help) echo 'fixture-demo is the caboodle fixture stack member' ;; \
         --version) echo 'fixture-demo {}' ;; esac",
        tag.trim_start_matches('v')
    )
}

fn sha256(path: &Path) -> String {
    let out = StdCommand::new("sha256sum").arg(path).output().unwrap();
    String::from_utf8(out.stdout).unwrap()[..64].to_owned()
}

#[test]
fn a_bump_records_the_digests_the_release_publishes() {
    let fake = Fake::new("v0.2.0");
    let published = sha256(&fake.path("served").join(ASSET));
    fake.bump(&[])
        .success()
        .stdout(predicate::str::contains("0.1.0 -> 0.2.0"))
        .stdout(predicate::str::contains(&published));
    let written = fs::read_to_string(fake.manifest()).unwrap();
    assert!(written.contains("version = \"0.2.0\""), "{written}");
    assert!(
        written.contains(&format!("x86_64-unknown-linux-gnu = \"{published}\"")),
        "{written}"
    );
    // Only version and digests change: the review comment survives.
    assert!(written.starts_with("# Test-only stack member"), "{written}");
    // The asset itself was downloaded and hashed, not only the sums file.
    assert!(
        fake.log().contains(&format!("/v0.2.0/{ASSET}")),
        "{}",
        fake.log()
    );
    assert!(fake.log().contains("/releases/latest"));

    // Control: a second bump finds it current and writes nothing.
    fake.bump(&[])
        .success()
        .stdout(predicate::str::contains("nothing written"));
    assert_eq!(fs::read_to_string(fake.manifest()).unwrap(), written);
}

#[test]
fn a_sums_file_that_disagrees_with_the_asset_is_refused_and_nothing_is_written() {
    let fake = Fake::new("v0.2.0");
    let before = fs::read_to_string(fake.manifest()).unwrap();
    // The served asset is replaced after its sums line was published.
    fs::write(fake.path("served").join(ASSET), b"different bytes").unwrap();
    fake.bump(&[])
        .failure()
        .stderr(predicate::str::contains("refusing to record either"));
    assert_eq!(fs::read_to_string(fake.manifest()).unwrap(), before);
}

#[test]
fn an_older_release_or_a_missing_target_line_is_refused() {
    let fake = Fake::new("v0.0.9");
    let before = fs::read_to_string(fake.manifest()).unwrap();
    fake.bump(&[])
        .failure()
        .stderr(predicate::str::contains("refusing to downgrade"));
    assert_eq!(fs::read_to_string(fake.manifest()).unwrap(), before);

    let fake = Fake::new("v0.2.0");
    fs::write(fake.path("served/SHA256SUMS.txt"), "").unwrap();
    fake.bump(&[]).failure().stderr(predicate::str::contains(
        "SHA256SUMS.txt for x86_64-unknown-linux-gnu",
    ));
    assert_eq!(fs::read_to_string(fake.manifest()).unwrap(), before);
}

#[test]
fn a_release_that_is_not_the_member_is_refused_before_anything_is_recorded() {
    // malcolm S1: a release that dropped its identity text must not bump green.
    let fake = Fake::new("v0.2.0");
    let before = fs::read_to_string(fake.manifest()).unwrap();
    fake.publish(
        "v0.2.0",
        "case \"$1\" in --help) echo 'a find and replace tool' ;; --version) echo 'fixture-demo 0.2.0' ;; esac",
    );
    fake.bump(&[]).failure().stderr(predicate::str::contains(
        "does not answer its manifest identity",
    ));
    assert_eq!(fs::read_to_string(fake.manifest()).unwrap(), before);

    // A release tagged 0.2.0 whose program reports another version is refused.
    fake.publish(
        "v0.2.0",
        "case \"$1\" in --help) echo 'fixture-demo is the caboodle fixture stack member' ;; --version) echo 'fixture-demo 0.1.0' ;; esac",
    );
    fake.bump(&[])
        .failure()
        .stderr(predicate::str::contains("reports version"));
    assert_eq!(fs::read_to_string(fake.manifest()).unwrap(), before);

    // Control: the same release with its identity and version intact bumps.
    fake.publish("v0.2.0", &good_program("v0.2.0"));
    fake.bump(&[]).success();
}

#[test]
fn a_bump_keeps_the_file_mode_and_runs_the_release_in_a_throwaway_home() {
    let fake = Fake::new("v0.2.0");
    fs::create_dir_all(fake.path("home")).unwrap();
    fs::set_permissions(fake.manifest(), fs::Permissions::from_mode(0o644)).unwrap();
    // A release program that writes under $HOME whenever it runs.
    fake.publish(
        "v0.2.0",
        &format!("touch \"$HOME/touched\"\n{}", good_program("v0.2.0")),
    );
    fake.bump(&[]).success();
    // malcolm N1: NamedTempFile's 0600 must not replace the reviewed file's mode.
    let mode = fs::metadata(fake.manifest()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
    // wu: the proof ran the release, but never in the maintainer's HOME.
    assert!(
        !fake.path("home/touched").exists(),
        "the release wrote into the real HOME"
    );
}

#[test]
fn an_explicit_tag_is_fetched_by_tag() {
    let fake = Fake::new("v0.2.0");
    fake.bump(&["--tag", "v0.2.0"])
        .success()
        .stdout(predicate::str::contains("0.1.0 -> 0.2.0"));
    assert!(
        fake.log().contains("/releases/tags/v0.2.0"),
        "{}",
        fake.log()
    );
    fake.bump(&["--tag", "v0.2.0/../x"])
        .failure()
        .stderr(predicate::str::contains("unsafe release tag"));
}
