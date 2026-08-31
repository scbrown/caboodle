# Installing and removing Caboodle

Caboodle publishes a release only when a `vVERSION` tag exactly matches the
version in `Cargo.toml`. Each supported archive has a sibling `.sha256` file;
the installer verifies that checksum before replacing the binary.

## Release install

Review the installer first, then run the same file you reviewed:

```bash
curl -fsSLo /tmp/caboodle-install.sh \
  https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh
less /tmp/caboodle-install.sh
sh /tmp/caboodle-install.sh
caboodle --version
```

Supported release targets are Linux x86_64, macOS x86_64, and macOS arm64.
The default destination is `${CARGO_HOME:-$HOME/.cargo}/bin/caboodle`; override
it with `CABOODLE_INSTALL_DIR` when that directory is managed elsewhere.

## Source install

A Rust toolchain can install the reviewed revision directly:

```bash
cargo install --git https://github.com/scbrown/caboodle \
  --rev <reviewed-commit-sha> --locked
caboodle --version
```

## First proof and resume

The interview writes a plan and changes nothing else. Read the complete file
before applying it:

```bash
caboodle init --guided
less caboodle-plan.toml
caboodle install
caboodle verify-questions
```

If input or an install is interrupted, rerun the same command. Interview
answers resume from `.caboodle/interview.toml`; install state resumes from
`.caboodle/state.json`. A green result means version read-back and functional
reader-path checks passed, not merely that an installer exited zero.

## Claude Code cloud environments

`scripts/setup-environment.sh` is the version-controlled copy of the setup
script for a Claude Code cloud environment. Paste its body into the Setup
script field (claude.ai/code → the cloud icon above the message box →
Add/edit environment) — the field takes a script, not a path, and it runs
before the repo is available, which is why the file exists to be pasted from
rather than executed.

It bootstraps quipu (built from source — the long pole, so its lane starts
first), caboodle itself (prebuilt release binary via `scripts/install.sh`),
the shared tooling the stack's quality gates need (`just`, `bd`,
`pre-commit`, `mdbook`, `mdbook-mermaid`, `cffi`), and stages the stack
knowledge packs into `~/.caboodle/packs/`, verified with
`quipu pack --verify` — a pack that fails verification is deleted rather
than trusted. It deliberately does **not** build the rest of the corpus:
that is caboodle's own job, and a session that needs the full stack runs
`caboodle install` against a reviewed plan so every tool is proved rather
than assumed.

## Stack knowledge packs

`packs/` carries quipu knowledge packs for working with the stack:

- `stack-map.qpack.db` — what each tool is, where it lives, and how the
  pieces relate.
- `stack-operations.qpack.db` — how each repo builds, tests, and proves
  itself, and the git discipline that binds them.

Each pack is an ordinary quipu SQLite store with a one-row manifest. Attach
one to any quipu database with `quipu unpack`, or prove one with
`quipu pack --verify`. The Turtle sources live in `packs/src/` and are the
review surface; `scripts/build-stack-packs.sh` rebuilds the artifacts from
them (run by hand when the sources change — packs stamp their creation time,
so a rebuild without a source change is hash churn, not content).

## Uninstall

Remove only the Caboodle binary and optional local Caboodle state. Installed
corpus tools are intentionally left in place because they may be shared with
other workflows:

```bash
rm "${CARGO_HOME:-$HOME/.cargo}/bin/caboodle"
# Optional, from the directory where Caboodle was run:
rm -r .caboodle
```
