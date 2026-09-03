#!/usr/bin/env bash
#
# Cloud-environment setup script for the caboodle stack.
#
# Paste the BODY of this file into the Setup script field of a Claude Code
# cloud environment (claude.ai/code → the cloud icon above the message box →
# Add/edit environment). The field takes a script, not a path, and it runs
# before the repo is available — so it cannot `bash scripts/setup-environment.sh`
# from the clone. This file is the version-controlled copy to paste from.
# https://code.claude.com/docs/en/claude-code-on-the-web
#
# THREE HARD CONSTRAINTS, from the docs and from measuring this:
#
#   1. Must exit 0. A non-zero exit means the session FAILS TO START. Hence
#      `set -u` without `-e`, `|| true` on everything optional, and an
#      unconditional `exit 0`.
#   2. Must finish in ~5 minutes or the environment cache never builds — and
#      without the cache this runs on EVERY session instead of once. The lanes
#      below run in PARALLEL so total time is the slowest lane, not the sum.
#   3. Needs network. GitHub (git, release assets, raw files), crates.io,
#      npmjs and PyPI are all on the default Trusted allowlist.
#
# The cache is a filesystem snapshot: installed files persist to later
# sessions, running processes do not. It rebuilds when this script changes or
# after ~7 days.
#
# WHAT THIS DELIBERATELY DOES NOT DO: build bobbin, yupana, or the rest of
# the corpus. Installing the whole stack is caboodle's own job — this script
# bootstraps caboodle and its prerequisites, and a session that needs the
# full stack runs `caboodle install --plan <reviewed plan>` and lets the
# wizard prove each tool rather than trusting this script's exit codes.
#
# NOT installed here — already in the base image: Rust, Python+uv+ruff+mypy+
# pytest, Node, ninja, git, jq, ripgrep.
#
set -u

log() { printf '\n\033[1m== %s\033[0m\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export DEBIAN_FRONTEND=noninteractive
mkdir -p "$HOME/.local/bin"

# ── Lane 1: quipu — checksummed release binaries ───────────────────────────
#
# Quipu publishes the reviewed full-feature build (including SHACL and ONNX)
# as a checksummed archive. Source compilation is deliberately opt-in: a
# transient release outage must not turn the five-minute setup lane into an
# unbounded 10–12 GB Rust build.
install_quipu_release() {
  local version=0.3.27 target=x86_64-unknown-linux-gnu
  local archive="quipu-v${version}-${target}.tar.gz"
  local base="https://github.com/scbrown/quipu/releases/download/v${version}"
  local tmp status
  [ "$(uname -s)-$(uname -m)" = "Linux-x86_64" ] || return 1
  tmp=$(mktemp -d) || return 1
  curl -fsSL "$base/$archive" -o "$tmp/$archive" &&
    curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" &&
    (cd "$tmp" && sha256sum -c "$archive.sha256") >/dev/null 2>&1 &&
    tar -xzf "$tmp/$archive" -C "$tmp" &&
    install -m 0755 "$tmp/quipu-v${version}-${target}/quipu" "$HOME/.local/bin/quipu" &&
    install -m 0755 "$tmp/quipu-v${version}-${target}/quipu-server" "$HOME/.local/bin/quipu-server"
  status=$?
  rm -rf "$tmp"
  return "$status"
}

install_quipu_source_fallback() {
  cargo install --locked --git https://github.com/scbrown/quipu \
    --features shacl,onnx --bin quipu --bin quipu-server >/dev/null 2>&1
}

(
  if ! have quipu && ! install_quipu_release; then
    [ "${CABOODLE_ALLOW_SOURCE_FALLBACK:-0}" = 1 ] && install_quipu_source_fallback || true
  fi
) &
QUIPU_PID=$!

# ── Lane 2: caboodle ───────────────────────────────────────────────────────
(
  # Prebuilt, sha256-verified release binary — seconds, not a cargo build.
  have caboodle || curl -fsSL \
      https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh \
      | sh >/dev/null 2>&1 || true

) &
CABOODLE_PID=$!

# ── Lane 3: npm + pip + prebuilt tools — small, fast ───────────────────────
(
  # `just` is the single entry point for every command across the stack's
  # repos. From npm rather than `cargo install just`: seconds, not minutes.
  have just || npm install -g rust-just >/dev/null 2>&1 || true

  # Task tracking. npm, NOT `go install`: the Go route builds with
  # CGO_ENABLED=0 and embedded Dolt needs CGO, so `bd init` then fails with
  # "embedded Dolt requires a CGO build".
  have bd || npm install -g @beads/bd >/dev/null 2>&1 || true

  have pre-commit || pip install --quiet --break-system-packages pre-commit >/dev/null 2>&1 || true

  # cffi: camayoc's test suite imports `cryptography`, whose _cffi_backend
  # is not wired in the base image — without this, test_certify_pack fails
  # at import with ModuleNotFoundError, measured 2026-08-31.
  python3 -c 'import _cffi_backend' 2>/dev/null || \
      pip install --quiet --break-system-packages cffi >/dev/null 2>&1 || true

  # mdbook + mdbook-mermaid: quipu's and caboodle's `just check` run an
  # mdbook-build hook, which hard-fails when either is missing. Prebuilt
  # binaries — a `cargo install mdbook` from source is minutes against the
  # 5-minute cap.
  have mdbook || curl -fsSL \
      https://github.com/rust-lang/mdBook/releases/download/v0.4.40/mdbook-v0.4.40-x86_64-unknown-linux-gnu.tar.gz \
      | tar xz -C "$HOME/.local/bin" 2>/dev/null || true
  have mdbook-mermaid || curl -fsSL \
      https://github.com/badboy/mdbook-mermaid/releases/download/v0.14.0/mdbook-mermaid-v0.14.0-x86_64-unknown-linux-gnu.tar.gz \
      | tar xz -C "$HOME/.local/bin" 2>/dev/null || true
) &
TOOLS_PID=$!

wait "$QUIPU_PID" "$CABOODLE_PID" "$TOOLS_PID" 2>/dev/null || true

# ── Post-install: consume a repository share directly by reference ─────────
# A qpack is a text share, not a SQLite file. The caller pins an immutable
# release URL; Quipu fetches it into bounded memory and verifies it before the
# transient store is opened. No user-visible download is staged here.
SHARE_STATUS="not requested"
if have quipu && [ -n "${CABOODLE_QUIPU_SHARE_URL:-}" ]; then
    if quipu import "$CABOODLE_QUIPU_SHARE_URL" >/dev/null 2>&1; then
        SHARE_STATUS="verified and imported from $CABOODLE_QUIPU_SHARE_URL"
    else
        SHARE_STATUS="FAILED verification/import from $CABOODLE_QUIPU_SHARE_URL"
    fi
fi

# ── Verify: report what is actually present ────────────────────────────────
# A setup script that half-worked and exited 0 is worse than one that failed,
# so print the truth rather than assuming the installs above landed.
log "Installed"
for tool in just bd pre-commit mdbook mdbook-mermaid quipu quipu-server caboodle; do
    if have "$tool"; then
        printf '  %-24s %s\n' "$tool" "$("$tool" --version 2>&1 | head -1)"
    else
        printf '  %-24s MISSING\n' "$tool"
    fi
done
log "Repository share"
printf '  %s\n' "$SHARE_STATUS"

# Never fail the session: a missing optional tool degrades a lane, it does
# not stop Claude from working on everything else.
exit 0
