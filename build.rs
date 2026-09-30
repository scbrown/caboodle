//! Embed the git commit in `caboodle --version`. Two builds of one release
//! version are otherwise indistinguishable, which hid a stale copy shadowing a
//! current one on PATH (aegis-nvw6ye.1).
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn main() {
    println!("cargo:rerun-if-env-changed=CABOODLE_GIT_SHA");
    // Rebuild when HEAD moves: HEAD itself, the branch ref it names, and
    // packed refs. Paths come from git so linked worktrees work too.
    for path in ["HEAD", "packed-refs"] {
        if let Some(p) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    let sha = std::env::var("CABOODLE_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git(&["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=CABOODLE_GIT_SHA={sha}");
}
