use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Serialize)]
struct Episode<'a> {
    name: String,
    episode_body: &'a str,
    source: &'a str,
    group_id: &'static str,
    nodes: Vec<Value>,
    edges: Vec<Value>,
}

pub fn queue_transition(
    state_path: &Path,
    tool: &str,
    transition: &str,
    version: &str,
) -> Result<PathBuf> {
    let queue = state_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("episodes");
    queue_event(
        &queue,
        transition,
        tool,
        &format!("CABOODLE {transition}: {tool} at {version}"),
    )
}

pub fn queue_br_jsonl(input: &Path, queue: &Path) -> Result<usize> {
    let body = fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;
    let mut count = 0;
    for (line_number, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let issue: Value = serde_json::from_str(line)
            .with_context(|| format!("parse br JSONL line {}", line_number + 1))?;
        let id = issue["id"].as_str().context("br row omitted string id")?;
        let title = issue["title"].as_str().unwrap_or("");
        queue_event(queue, "bead-created", id, &format!("{id}: {title}"))?;
        count += 1;
        if let Some(comments) = issue["comments"].as_array() {
            for (index, comment) in comments.iter().enumerate() {
                let comment_id = comment["id"]
                    .as_str()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| (index + 1).to_string());
                let text = comment["text"]
                    .as_str()
                    .or_else(|| comment["body"].as_str())
                    .unwrap_or("");
                queue_event(
                    queue,
                    "bead-commented",
                    &format!("{id}-{comment_id}"),
                    &format!("{id} comment {comment_id}: {text}"),
                )?;
                count += 1;
            }
        }
        if issue["status"].as_str() == Some("closed") {
            queue_event(queue, "bead-closed", id, &format!("{id}: closed"))?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn flush(queue: &Path, endpoint: &str) -> Result<usize> {
    flush_with_program(queue, endpoint, Path::new("curl"))
}

fn flush_with_program(queue: &Path, endpoint: &str, curl: &Path) -> Result<usize> {
    if !(endpoint.starts_with("https://") || endpoint.starts_with("http://localhost")) {
        bail!("Quipu endpoint must use https:// or explicit localhost http://");
    }
    let token = env::var("QUIPU_AUTH_TOKEN").context("QUIPU_AUTH_TOKEN is required to flush")?;
    let mut auth = tempfile::NamedTempFile::new()?;
    writeln!(auth, "Authorization: Bearer {token}")?;
    let auth_header = format!("@{}", auth.path().display());
    let control = Command::new(curl)
        .args([
            "-fsS",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            "X-Quipu-Client: agent-adhoc",
            "-d",
            r#"{"query":"SELECT ?marker WHERE { VALUES ?marker { <urn:caboodle:control> } }"}"#,
            &format!("{}/query", endpoint.trim_end_matches('/')),
        ])
        .arg("-H")
        .arg(&auth_header)
        .output()
        .context("run Quipu readiness control")?;
    if !control.status.success()
        || !String::from_utf8_lossy(&control.stdout).contains("caboodle:control")
    {
        bail!("Quipu readiness control failed; queue remains pending");
    }

    let sent = queue.join("sent");
    fs::create_dir_all(&sent)?;
    let mut paths: Vec<_> = fs::read_dir(queue)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut delivered = 0;
    for path in paths {
        let result = Command::new(curl)
            .args([
                "-fsS",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "-H",
                "X-Quipu-Client: agent-adhoc",
                "--data-binary",
            ])
            .arg(format!("@{}", path.display()))
            .arg("-H")
            .arg(&auth_header)
            .arg(format!("{}/episode", endpoint.trim_end_matches('/')))
            .output()
            .with_context(|| format!("post queued episode {}", path.display()))?;
        if !result.status.success() {
            bail!("queued episode delivery failed; identical file remains pending");
        }
        let response: Value =
            serde_json::from_slice(&result.stdout).context("parse Quipu episode response")?;
        if !matches!(
            response["outcome"].as_str(),
            Some("created" | "updated" | "unchanged")
        ) {
            bail!("Quipu response omitted a successful outcome; identical file remains pending");
        }
        fs::rename(&path, sent.join(path.file_name().unwrap()))?;
        delivered += 1;
    }
    Ok(delivered)
}

fn queue_event(queue: &Path, kind: &str, subject: &str, body: &str) -> Result<PathBuf> {
    let redacted = redact(body);
    let identity = format!("{kind}\n{subject}\n{redacted}");
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    let name = format!("caboodle-{kind}-{}", &digest[..16]);
    let episode = Episode {
        name: name.clone(),
        episode_body: &redacted,
        source: "caboodle-emission-queue-v1",
        group_id: "caboodle",
        nodes: vec![json!({
            "name": name,
            "type": "Feature",
            "description": redacted,
        })],
        edges: vec![],
    };
    fs::create_dir_all(queue)?;
    let path = queue.join(format!("{digest}.json"));
    if !path.exists() {
        let mut tmp = tempfile::NamedTempFile::new_in(queue)?;
        serde_json::to_writer_pretty(&mut tmp, &episode)?;
        writeln!(tmp)?;
        tmp.persist(&path).map_err(|error| error.error)?;
    }
    Ok(path)
}

fn redact(input: &str) -> String {
    let mut output = Vec::new();
    let mut hide_next = false;
    for word in input.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        if hide_next {
            output.push("[REDACTED]");
            hide_next = false;
        } else if lower == "bearer" {
            output.push(word);
            hide_next = true;
        } else if [
            "token=",
            "token:",
            "password=",
            "password:",
            "secret=",
            "secret:",
            "authorization:",
        ]
        .iter()
        .any(|needle| lower.starts_with(needle))
        {
            output.push("[REDACTED]");
            hide_next = lower.ends_with(':');
        } else {
            output.push(word);
        }
    }
    output.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_and_secret_events_queue_once() {
        let root = tempfile::tempdir().unwrap();
        let queue = root.path().join("queue");
        let first = queue_event(&queue, "verified", "quipu", "token=abc Bearer xyz ready").unwrap();
        let second =
            queue_event(&queue, "verified", "quipu", "token=abc Bearer xyz ready").unwrap();
        assert_eq!(first, second);
        let body = fs::read_to_string(first).unwrap();
        assert!(!body.contains("abc"));
        assert!(!body.contains("xyz"));
        assert!(body.contains("[REDACTED]"));
        assert_eq!(fs::read_dir(queue).unwrap().count(), 1);
    }

    #[test]
    fn br_snapshots_map_created_comments_and_closed_idempotently() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("issues.jsonl");
        let queue = root.path().join("queue");
        fs::write(&input, r#"{"id":"demo-1","title":"Demo","status":"closed","comments":[{"id":"c1","text":"done"}]}"#).unwrap();
        assert_eq!(queue_br_jsonl(&input, &queue).unwrap(), 3);
        assert_eq!(queue_br_jsonl(&input, &queue).unwrap(), 3);
        assert_eq!(fs::read_dir(queue).unwrap().count(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn offline_and_retry_keep_identical_queue_until_success() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let queue = root.path().join("queue");
        queue_event(&queue, "verified", "quipu", "ready").unwrap();
        let curl = root.path().join("curl");
        let marker = root.path().join("attempted");
        fs::write(
            &curl,
            format!(
                "#!/bin/sh\ncase \"$*\" in\n  */query*) echo '{{\"marker\":\"urn:caboodle:control\"}}';;\n  */episode*) if [ ! -e '{}' ]; then touch '{}'; exit 1; else echo '{{\"outcome\":\"unchanged\"}}'; fi;;\nesac\n",
                marker.display(),
                marker.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&curl).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&curl, permissions).unwrap();
        std::env::set_var("QUIPU_AUTH_TOKEN", "fixture-token");
        assert!(flush_with_program(&queue, "https://example.invalid", &curl).is_err());
        assert_eq!(
            fs::read_dir(&queue)
                .unwrap()
                .filter_map(Result::ok)
                .filter(
                    |entry| entry.path().extension().and_then(|value| value.to_str())
                        == Some("json")
                )
                .count(),
            1
        );
        assert_eq!(
            flush_with_program(&queue, "https://example.invalid", &curl).unwrap(),
            1
        );
        assert_eq!(
            flush_with_program(&queue, "https://example.invalid", &curl).unwrap(),
            0
        );
        assert_eq!(fs::read_dir(queue.join("sent")).unwrap().count(), 1);
        std::env::remove_var("QUIPU_AUTH_TOKEN");
    }
}
