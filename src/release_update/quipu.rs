//! Paired Quipu artifacts, pinned at review time, with one durable transaction.
use super::*;
use crate::model::QuipuFlavor;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    version: String,
    sha256: BTreeMap<String, String>,
}
impl Pin {
    fn digest(&self, target: &str) -> Result<&str> {
        stable_version(&self.version)?;
        let digest = self
            .sha256
            .get(target)
            .context("Quipu pin lacks this target")?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("Quipu pin requires lowercase SHA256");
        }
        Ok(digest)
    }
}

pub(super) fn update(plan: &Plan, state: &Path, check: bool) -> Result<()> {
    let pin: Pin = toml::from_str(include_str!("../../release-pins/quipu.toml"))?;
    update_at(plan, state, check, &pin)
}

fn owned_pair() -> Result<[PathBuf; 2]> {
    let home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
        .context("Quipu requires its Caboodle-owned cargo bin directory")?;
    let directory = home.join("bin").canonicalize()?;
    let mut paths = Vec::new();
    for binary in ["quipu", "quipu-server"] {
        let path = selected_path(binary)?;
        if path.parent().context("binary directory")?.canonicalize()? != directory {
            bail!("Quipu release update requires owned PATH commands in CARGO_HOME/bin");
        }
        if binary == "quipu" && !fs::symlink_metadata(&path)?.file_type().is_file() {
            bail!("owned Quipu primary must be a regular non-symlink executable");
        }
        crate::quipu_layout::alias(&path)?;
        paths.push(directory.join(binary));
    }
    Ok(paths.try_into().unwrap())
}

type VersionIdentity = (String, (u64, u64, u64));

fn versions(paths: &[PathBuf; 2]) -> Result<[VersionIdentity; 2]> {
    let mut values = Vec::new();
    for (path, name) in paths.iter().zip(["quipu", "quipu-server"]) {
        let output = text(path.to_str().context("binary encoding")?, &["--version"])?;
        if output.split_whitespace().next() != Some(name) {
            bail!("Quipu executable identity differs from {name}");
        }
        let version = installed_version(&output)?;
        values.push((output, version));
    }
    if values[0].1 != values[1].1 {
        bail!("Quipu CLI/server versions disagree; refusing mixed pair");
    }
    Ok(values.try_into().unwrap())
}

fn extract_pair(archive: &Path, directory: &Path) -> Result<[PathBuf; 2]> {
    let list = text(
        "tar",
        &["-tzf", archive.to_str().context("archive encoding")?],
    )?;
    let mut paths = Vec::new();
    for binary in ["quipu", "quipu-server"] {
        let members: Vec<_> = list
            .lines()
            .filter(|p| Path::new(p).file_name().is_some_and(|n| n == binary))
            .collect();
        if members.len() != 1 {
            bail!("Quipu archive must contain exactly one {binary}");
        }
        let member = members[0];
        if Path::new(member).is_absolute()
            || Path::new(member)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            bail!("unsafe Quipu archive member");
        }
        // Refuse tar links/directories, even when -xO would dereference a link.
        let detail = text("tar", &["-tvzf", archive.to_str().unwrap(), "--", member])?;
        if binary == "quipu-server"
            && detail.starts_with('l')
            && detail.lines().count() == 1
            && detail.trim_end().ends_with(&format!("{member} -> quipu"))
            && Path::new(member).parent()
                == list
                    .lines()
                    .find(|p| Path::new(p).file_name().is_some_and(|n| n == "quipu"))
                    .and_then(|p| Path::new(p).parent())
        {
            let path = directory.join(binary);
            crate::quipu_layout::replace_alias(&path)?;
            paths.push(path);
            continue;
        }
        if !detail.starts_with('-') || detail.lines().count() != 1 {
            bail!("Quipu archive executable must be regular or the exact relative server alias");
        }
        let bytes = checked(
            "tar",
            ["-xOzf", archive.to_str().unwrap(), "--", member],
            None,
        )?;
        let path = directory.join(binary);
        fs::write(&path, bytes.stdout)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        paths.push(path);
    }
    Ok(paths.try_into().unwrap())
}

fn update_at(plan: &Plan, state_path: &Path, check: bool, pin: &Pin) -> Result<()> {
    plan.validate()?;
    if plan.external_quipu
        || plan.quipu_flavor != QuipuFlavor::Release
        || !plan.tools.contains(&ToolName::Quipu)
    {
        bail!("Quipu release update requires a selected owned release-flavor pair");
    }
    let target = crate::adapter::quipu_release_target()?;
    let digest = pin.digest(target)?;
    let _lock = lock_state(state_path)?;
    if held() {
        println!("quipu: held (no release lookup or install)");
        return Ok(());
    }
    let paths = owned_pair()?;
    crate::adapter::preflight_owned_quipu_alias()?;
    let before = versions(&paths)?;
    let wanted = stable_version(&pin.version)?;
    if before[0].1 > wanted {
        println!(
            "quipu: ahead of reviewed pin {} — refusing downgrade",
            pin.version
        );
        return Ok(());
    }
    let tag = format!("quipu-ai-v{}", pin.version);
    let archive_name = format!("quipu-{tag}-{target}.tar.gz");
    let sums_name = format!("{archive_name}.sha256");
    let raw = text(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "30",
            "--proto",
            "=https",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/scbrown/quipu/releases/tags/{tag}"),
        ],
    )?;
    let release: Release = serde_json::from_str(&raw)?;
    if release.tag_name != tag || release.draft || release.prerelease {
        bail!("Quipu publication is not the reviewed stable release");
    }
    for name in [&archive_name, &sums_name] {
        if release.assets.iter().filter(|a| &a.name == name).count() != 1 {
            bail!("Quipu publication lacks unique asset {name}");
        }
    }
    if check {
        println!("quipu: reviewed pin {}, installed {}; paired assets available, checksum/functional proof not run", pin.version, before[0].0);
        return Ok(());
    }
    let scratch = tempfile::tempdir()?;
    let archive = scratch.path().join(&archive_name);
    let sums = scratch.path().join(&sums_name);
    let base = format!("https://github.com/scbrown/quipu/releases/download/{tag}");
    download_https(&format!("{base}/{archive_name}"), &archive)?;
    download_https(&format!("{base}/{sums_name}"), &sums)?;
    if hash(&archive)? != digest || checksum(&fs::read_to_string(sums)?, &archive_name)? != digest {
        bail!("Quipu release SHA256 differs from reviewed pin");
    }
    let candidates = extract_pair(&archive, scratch.path())?;
    let after = versions(&candidates)?;
    if after[0].1 != wanted {
        bail!("Quipu candidate pair differs from reviewed version");
    }
    let prior_aliases = [
        crate::quipu_layout::alias(&paths[0])?,
        crate::quipu_layout::alias(&paths[1])?,
    ];
    let prior_hashes = [hash(&paths[0])?, hash(&paths[1])?];
    let new_hashes = [hash(&candidates[0])?, hash(&candidates[1])?];
    if before[0].1 == wanted && prior_hashes != new_hashes {
        bail!("Quipu version equals pin but paired bytes differ; refusing ambiguous replacement");
    }
    // The existing verifier uses only a throwaway DB; server help/version is a
    // CLI contract probe, not a running-server or service-deployment assertion.
    crate::adapter::verify_quipu_pair(&candidates[0], &candidates[1])?;
    let mut state = State::read(state_path)?;
    if prior_hashes == new_hashes {
        crate::adapter::verify_quipu_pair(&paths[0], &paths[1])?;
        crate::adapter::ensure_owned_quipu_alias()?;
        println!("quipu: current paired release and freshly verified");
        return Ok(());
    }
    let previous_state = match fs::read(state_path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let mut artifacts = Vec::new();
    for (i, binary) in ["quipu", "quipu-server"].iter().enumerate() {
        let backup = state_path
            .parent()
            .context("state directory")?
            .join("release-backups")
            .join(binary)
            .join(&prior_hashes[i]);
        atomic_copy(&paths[i], &backup)?;
        artifacts.push(transaction::Pending {
            restore_alias: prior_aliases[i].clone(),
            tool: "quipu".into(),
            destination: paths[i].clone(),
            backup,
            sha256: prior_hashes[i].clone(),
        });
    }
    if held() {
        println!("quipu: held before paired swap");
        return Ok(());
    }
    if owned_pair()? != paths
        || versions(&paths)? != before
        || [hash(&paths[0])?, hash(&paths[1])?] != prior_hashes
        || [
            crate::quipu_layout::alias(&paths[0])?,
            crate::quipu_layout::alias(&paths[1])?,
        ] != prior_aliases
    {
        bail!("Quipu pair changed concurrently; refusing swap");
    }
    transaction::begin_pair(
        state_path,
        &transaction::PairPending {
            runtime_aliases: candidates[1].is_symlink()
                || prior_aliases.iter().any(Option::is_some),
            artifacts,
            previous_state,
        },
    )?;
    let result = (|| -> Result<()> {
        for i in 0..2 {
            if crate::quipu_layout::alias(&candidates[i])?.is_some() {
                crate::quipu_layout::replace_alias(&paths[i])?;
            } else {
                atomic_copy(&candidates[i], &paths[i])?;
            }
            durable::sync_dir(durable::parent(&paths[i]))?;
        }
        if [hash(&paths[0])?, hash(&paths[1])?] != new_hashes || versions(&paths)? != after {
            bail!("installed Quipu pair read-back mismatch");
        }
        crate::adapter::verify_quipu_pair(&paths[0], &paths[1])?;
        // The relative link remains valid if a later error restores the prior
        // pair; the supported command follows the owned canonical executable.
        crate::adapter::ensure_owned_quipu_alias()?;
        state.tools.insert(
            "quipu".into(),
            ToolState {
                version: format!("{}; {}", after[0].0, after[1].0),
                applied: true,
                verified: true,
            },
        );
        state.write(state_path)?;
        fs::File::open(state_path)?.sync_all()?;
        durable::sync_dir(durable::parent(state_path))?;
        durable::create_dirs(&durable::parent(state_path).join("episodes"))?;
        let event = emission::queue_transition(
            state_path,
            "quipu",
            "release-updated",
            &format!("{}; {}", after[0].0, after[1].0),
        )?;
        fs::File::open(&event)?.sync_all()?;
        durable::sync_dir(durable::parent(&event))?;
        Ok(())
    })();
    if let Err(error) = result {
        transaction::recover(state_path)
            .context("paired update failed AND recovery failed; retain journal")?;
        return Err(error)
            .context("Quipu release verification failed; previous pair/state restored");
    }
    transaction::finish(state_path).context("verified Quipu pair, but durable journal completion is uncertain; no success or restoration claimed")?;
    println!("quipu: installed and verified reviewed paired pin {} (CLI roundtrip and server command contract; no service restart)", pin.version);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, process::Command};

    #[test]
    fn unified_archive_keeps_the_server_as_an_exact_relative_alias() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("release");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("quipu"), b"private executable fixture").unwrap();
        std::os::unix::fs::symlink("quipu", source.join("quipu-server")).unwrap();
        let archive = root.path().join("release.tar.gz");
        assert!(Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(root.path())
            .arg("release")
            .status()
            .unwrap()
            .success());
        let destination = root.path().join("out");
        fs::create_dir(&destination).unwrap();
        let paths = extract_pair(&archive, &destination).unwrap();
        assert_eq!(fs::read_link(&paths[1]).unwrap(), Path::new("quipu"));
        assert_eq!(
            paths[0].canonicalize().unwrap(),
            paths[1].canonicalize().unwrap()
        );
    }

    #[test]
    fn unified_archive_refuses_foreign_dangling_absolute_and_cross_directory_aliases() {
        for (target, separate) in [
            ("other", false),
            ("../quipu", false),
            ("/quipu", false),
            ("quipu", true),
        ] {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("release");
            fs::create_dir(&source).unwrap();
            fs::write(source.join("quipu"), b"fixture").unwrap();
            let server_dir = if separate {
                let nested = source.join("nested");
                fs::create_dir(&nested).unwrap();
                nested
            } else {
                source.clone()
            };
            std::os::unix::fs::symlink(target, server_dir.join("quipu-server")).unwrap();
            let archive = root.path().join("release.tar.gz");
            assert!(Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(root.path())
                .arg("release")
                .status()
                .unwrap()
                .success());
            let output = root.path().join("output");
            fs::create_dir(&output).unwrap();
            assert!(
                extract_pair(&archive, &output).is_err(),
                "accepted {target}/{separate}"
            );
        }
    }

    #[test]
    fn reviewed_pin_covers_exact_native_targets() {
        let pin: Pin = toml::from_str(include_str!("../../release-pins/quipu.toml")).unwrap();
        assert_eq!(pin.sha256.len(), 4);
        for target in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ] {
            pin.digest(target).unwrap();
        }
        assert!(pin.digest("invented-target").is_err());
    }

    // Each scenario gets its OWN process and HOME; never mutate the suite's environment.
    #[test]
    fn isolated_pair_update_and_refusal_controls() {
        for mode in [
            "success",
            "unified-success",
            "bad-digest",
            "missing-checksum",
            "missing-server",
            "duplicate-server",
            "mixed-candidate",
            "mixed-installed",
            "failed-candidate",
            "failed-installed",
            "hold",
            "dangling-hold",
            "ahead",
            "same-version",
            "external",
            "lancedb",
            "interrupted",
            "interrupted-state",
            "failed-state",
            "hold-arrives",
            "bad-backup",
        ] {
            let child = Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "release_update::quipu::tests::fixture_process",
                    "--ignored",
                    "--nocapture",
                ])
                .env("CABOODLE_PAIR_SCENARIO", mode)
                .output()
                .unwrap();
            assert!(
                child.status.success(),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&child.stdout),
                String::from_utf8_lossy(&child.stderr)
            );
        }
    }

    fn script(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[ignore = "private child fixture invoked by isolated_pair_update_and_refusal_controls"]
    fn fixture_process() {
        let mode = env::var("CABOODLE_PAIR_SCENARIO").expect("fixture scenario");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let cargo = root.join("cargo");
        let bin = cargo.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let downloads = root.join("downloads");
        fs::create_dir(&downloads).unwrap();
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        env::set_var("HOME", root);
        env::set_var("CARGO_HOME", &cargo);
        env::set_var("XDG_CONFIG_HOME", root.join("config"));
        env::set_var("XDG_CACHE_HOME", root.join("cache"));
        env::set_var("XDG_STATE_HOME", root.join("state"));
        env::set_var("CABOODLE_HOLD_FILE", root.join("hold"));
        env::remove_var("QUIPU_AUTH_TOKEN");
        env::remove_var("QUIPU_AUTH_TOKEN_FILE");
        env::set_var(
            "CABOODLE_QUECHUA_FILE",
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/vocabulary/quechua-ns-v0.1.0.ttl"),
        );
        env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
        let client = |version: &str, broken: bool| {
            format!(
                r#"
if [ "$1" = --version ]; then echo 'quipu {version}'; exit 0; fi
if [ '{broken}' = true ]; then exit 7; fi
if [ "$1" = read ]; then
  query=$2; db=$4
  case "$query" in
    ASK*)
      if [ ! -f "$db.knot" ]; then echo false
      else case "$query" in *CaboodleVerifyAbsentTerm*) echo false;; *) echo true;; esac; fi;;
    *) [ ! -f "$db" ] || echo caboodle-verify-roundtrip;;
  esac
elif [ "$1" = episode ]; then touch "$4"
elif [ "$1" = knot ]; then cp "$2" "$4.knot"
else exit 8; fi
"#
            )
        };
        let old = if mode == "ahead" {
            "0.13.0"
        } else if mode == "same-version" {
            "0.12.0"
        } else {
            "0.11.1"
        };
        script(&bin.join("quipu"), &client(old, false));
        std::os::unix::fs::symlink("quipu", bin.join("qp")).unwrap();
        let old_server = if mode == "mixed-installed" {
            "0.11.0"
        } else {
            old
        };
        script(&bin.join("quipu-server"), &format!("case \"$1\" in --version) echo 'quipu-server {old_server}';; --help) echo '--db --bind';; *) exit 8;; esac"));
        let mut candidate = client("0.12.0", mode == "failed-candidate");
        if mode == "same-version" {
            candidate.push_str("\n# distinct source build bytes\n");
        }
        if mode == "failed-installed" {
            // A candidate that succeeds staged but fails at the installed PATH.
            candidate = candidate.replace(
                "if [ 'false' = true ];",
                "case \"$0\" in */cargo/bin/quipu) exit 19;; esac\nif [ 'false' = true ];",
            );
        }
        if mode == "unified-success" {
            candidate = format!("case \"$0\" in *quipu-server) case \"$1\" in --version) echo 'quipu-server 0.12.0';; --help) echo '--db --bind';; *) exit 8;; esac; exit 0;; esac\n{candidate}");
        }
        script(&stage.join("quipu"), &candidate);
        let server_version = if mode == "mixed-candidate" {
            "0.12.1"
        } else {
            "0.12.0"
        };
        script(&stage.join("quipu-server"), &format!("case \"$1\" in --version) echo 'quipu-server {server_version}';; --help) echo '--db --bind';; *) exit 8;; esac"));
        if mode == "unified-success" {
            fs::remove_file(stage.join("quipu-server")).unwrap();
            std::os::unix::fs::symlink("quipu", stage.join("quipu-server")).unwrap();
        }
        let target = crate::adapter::quipu_release_target().unwrap();
        let name = format!("quipu-quipu-ai-v0.12.0-{target}.tar.gz");
        let archive = downloads.join(&name);
        let mut tar = Command::new("tar");
        tar.args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(&stage)
            .arg("quipu");
        if mode != "missing-server" {
            tar.arg("quipu-server");
        }
        if mode == "duplicate-server" {
            fs::create_dir(stage.join("nested")).unwrap();
            fs::copy(
                stage.join("quipu-server"),
                stage.join("nested/quipu-server"),
            )
            .unwrap();
            tar.arg("nested/quipu-server");
        }
        assert!(tar.status().unwrap().success());
        let digest = hash(&archive).unwrap();
        fs::write(
            downloads.join(format!("{name}.sha256")),
            format!("{digest}  {name}\n"),
        )
        .unwrap();
        let mut assets = vec![serde_json::json!({"name":name})];
        if mode != "missing-checksum" {
            assets.push(serde_json::json!({"name":format!("{name}.sha256")}));
        }
        fs::write(downloads.join("metadata"),serde_json::json!({"tag_name":"quipu-ai-v0.12.0","draft":false,"prerelease":false,"assets":assets}).to_string()).unwrap();
        script(
            &bin.join("curl"),
            r#"
out=''; url=''
while [ "$#" -gt 0 ]; do case "$1" in --output|-o) shift; out=$1;; https://*) url=$1;; esac; shift; done
printf '%s\n' "$url" >> "$HOME/requests"
case "$url" in
 https://api.github.com/repos/scbrown/quipu/releases/tags/quipu-ai-v0.12.0) cat "$HOME/downloads/metadata";;
 https://github.com/scbrown/quipu/releases/download/quipu-ai-v0.12.0/*) cp "$HOME/downloads/${url##*/}" "$out";;
 *) echo 'unapproved fixture network' >&2; exit 99;; esac
"#,
        );
        let state_path = root.join("state.json");
        let mut state = State::read(&state_path).unwrap();
        state.tools.insert(
            "bobbin".into(),
            ToolState {
                version: "untouched".into(),
                applied: true,
                verified: true,
            },
        );
        state.tools.insert(
            "quipu".into(),
            ToolState {
                version: "prior-pair".into(),
                applied: true,
                verified: true,
            },
        );
        state.write(&state_path).unwrap();
        let state_before = fs::read(&state_path).unwrap();
        let paths = [bin.join("quipu"), bin.join("quipu-server")];
        let prior_bytes = [fs::read(&paths[0]).unwrap(), fs::read(&paths[1]).unwrap()];
        let mut plan = Plan::for_profile(crate::model::Profile::Kg);
        if mode == "external" {
            plan.external_quipu = true;
        }
        if mode == "lancedb" {
            plan.quipu_flavor = QuipuFlavor::Lancedb;
        }
        if mode == "hold" {
            fs::write(root.join("hold"), "operator").unwrap();
        }
        if mode == "dangling-hold" {
            std::os::unix::fs::symlink(root.join("missing"), root.join("hold")).unwrap();
        }
        if ["interrupted", "interrupted-state", "bad-backup"].contains(&mode.as_str()) {
            let mut artifacts = Vec::new();
            for (i, path) in paths.iter().enumerate() {
                let backup = root.join(format!("backup{i}"));
                atomic_copy(path, &backup).unwrap();
                artifacts.push(transaction::Pending {
                    restore_alias: None,
                    tool: "quipu".into(),
                    destination: path.clone(),
                    backup,
                    sha256: hash(path).unwrap(),
                });
            }
            transaction::begin_pair(
                &state_path,
                &transaction::PairPending {
                    runtime_aliases: false,
                    artifacts,
                    previous_state: Some(state_before.clone()),
                },
            )
            .unwrap();
            fs::copy(stage.join("quipu"), &paths[0]).unwrap();
            if mode == "interrupted-state" {
                fs::copy(stage.join("quipu-server"), &paths[1]).unwrap();
                state.tools.get_mut("quipu").unwrap().version = "unconfirmed".into();
                state.write(&state_path).unwrap();
            }
            if mode == "bad-backup" {
                fs::write(root.join("backup1"), "damaged").unwrap();
            }
            let result = transaction::recover(&state_path);
            if mode == "bad-backup" {
                assert!(result.is_err());
                assert!(state_path.with_extension("release-pending.json").exists());
                assert_eq!(
                    fs::read(&paths[0]).unwrap(),
                    fs::read(stage.join("quipu")).unwrap()
                );
            } else {
                result.unwrap();
                assert_eq!(fs::read(&paths[0]).unwrap(), prior_bytes[0]);
                assert_eq!(fs::read(&paths[1]).unwrap(), prior_bytes[1]);
                assert_eq!(fs::read(&state_path).unwrap(), state_before);
            }
            assert_eq!(fs::read_link(bin.join("qp")).unwrap(), Path::new("quipu"));
            assert_eq!(
                fs::read(bin.join("qp")).unwrap(),
                fs::read(&paths[0]).unwrap()
            );
            return;
        }
        if mode == "failed-state" {
            fs::write(root.join("episodes"), "not a directory").unwrap();
        }
        if mode == "hold-arrives" {
            let curl = fs::read_to_string(bin.join("curl")).unwrap().replace(
                r#"cp "$HOME/downloads/${url##*/}" "$out""#,
                r#"cp "$HOME/downloads/${url##*/}" "$out"; touch "$HOME/hold""#,
            );
            fs::write(bin.join("curl"), curl).unwrap();
        }
        let pin = Pin {
            version: "0.12.0".into(),
            sha256: BTreeMap::from([(
                target.into(),
                if mode == "bad-digest" {
                    "0".repeat(64)
                } else {
                    digest
                },
            )]),
        };
        let result = update_at(&plan, &state_path, false, &pin);
        assert_eq!(fs::read_link(bin.join("qp")).unwrap(), Path::new("quipu"));
        use std::os::unix::fs::MetadataExt;
        let alias_meta = fs::metadata(bin.join("qp")).unwrap();
        let client_meta = fs::metadata(&paths[0]).unwrap();
        assert_eq!(
            (alias_meta.dev(), alias_meta.ino()),
            (client_meta.dev(), client_meta.ino())
        );
        assert_eq!(
            text(bin.join("qp").to_str().unwrap(), &["--version"]).unwrap(),
            text(paths[0].to_str().unwrap(), &["--version"]).unwrap()
        );
        if mode == "success" || mode == "unified-success" {
            result.unwrap();
            if mode == "unified-success" {
                assert_eq!(fs::read_link(&paths[1]).unwrap(), Path::new("quipu"));
                assert_eq!(
                    paths[0].canonicalize().unwrap(),
                    paths[1].canonicalize().unwrap()
                );
            }
            assert_eq!(versions(&paths).unwrap()[0].1, (0, 12, 0));
            let state = State::read(&state_path).unwrap();
            assert!(state.tools["quipu"].verified);
            assert_eq!(state.tools["bobbin"].version, "untouched");
            for binary in ["quipu", "quipu-server"] {
                assert_eq!(
                    fs::read_dir(root.join("release-backups").join(binary))
                        .unwrap()
                        .count(),
                    1
                );
            }
            assert!(!state_path.with_extension("release-pending.json").exists());
        } else {
            if ["hold", "dangling-hold", "ahead"].contains(&mode.as_str()) {
                result.unwrap();
                assert!(!root.join("requests").exists());
            } else if mode == "hold-arrives" {
                result.unwrap();
            } else {
                assert!(result.is_err(), "{mode} unexpectedly succeeded");
            }
            assert_eq!(
                fs::read(&paths[0]).unwrap(),
                prior_bytes[0],
                "{mode} changed CLI"
            );
            assert_eq!(
                fs::read(&paths[1]).unwrap(),
                prior_bytes[1],
                "{mode} changed server"
            );
            assert_eq!(
                fs::read(&state_path).unwrap(),
                state_before,
                "{mode} changed state"
            );
            assert!(!state_path.with_extension("release-pending.json").exists());
        }
    }
}
