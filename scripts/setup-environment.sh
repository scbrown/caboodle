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
#      without the cache this runs on EVERY session instead of once. Quipu
#      alone is 4m19s measured, so the lanes below run in PARALLEL and the
#      total is the slowest lane, not the sum.
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

# ── Lane 1: quipu — the long pole, so start it first ────────────────────────
#
# There is no prebuilt binary: quipu's releases are release-plz source tags
# with empty asset lists. Measured 4m19s for a release build on 4 vCPUs.
#
# --features shacl,onnx is mandatory, not a preference:
#   shacl → enforces governance at WRITE time; without it ungoverned facts
#           store silently, which is the exact failure the store exists to
#           prevent.
#   onnx  → the embedding runtime. Without it quipu_context and
#           quipu_hybrid_search degrade to SPARQL CONTAINS.
#
# Both bins are named explicitly because quipu-server declares
# `required-features` and a plain build SILENTLY SKIPS it — exit 0, no
# warning.
(
  have quipu || cargo install --locked --git https://github.com/scbrown/quipu \
      --features shacl,onnx --bin quipu --bin quipu-server >/dev/null 2>&1 || true
) &
QUIPU_PID=$!

# ── Lane 2: caboodle + stack knowledge packs ───────────────────────────────
(
  # Prebuilt, sha256-verified release binary — seconds, not a cargo build.
  have caboodle || curl -fsSL \
      https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh \
      | sh >/dev/null 2>&1 || true

  # Stage the stack knowledge packs where any session can `quipu unpack`
  # them without waiting for a clone. These are quipu .qpack.db artifacts
  # carrying the stack map and per-tool operational knowledge; verification
  # happens below, AFTER the quipu lane finishes, because only quipu can
  # prove a pack rather than merely download it.
  mkdir -p "$HOME/.caboodle/packs"
  for pack in stack-map stack-operations; do
      [ -f "$HOME/.caboodle/packs/$pack.qpack.db" ] || curl -fsSL \
          "https://raw.githubusercontent.com/scbrown/caboodle/main/packs/$pack.qpack.db" \
          -o "$HOME/.caboodle/packs/$pack.qpack.db" 2>/dev/null || true
  done
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

# ── Post-install: verify the staged packs now that quipu can exist ─────────
# A pack that fails verification is deleted rather than left in place: a
# half-downloaded or tampered pack sitting where sessions trust it is worse
# than an absent one, and absence is visible in the report below.
if have quipu; then
    for p in "$HOME/.caboodle/packs/"*.qpack.db; do
        [ -f "$p" ] || continue
        quipu pack --verify "$p" >/dev/null 2>&1 || rm -f "$p"
    done
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
log "Stack packs"
for p in "$HOME/.caboodle/packs/"*.qpack.db; do
    [ -f "$p" ] && printf '  %s (verified)\n' "$p" || printf '  none staged\n'
done

# Never fail the session: a missing optional tool degrades a lane, it does
# not stop Claude from working on everything else.
exit 0
