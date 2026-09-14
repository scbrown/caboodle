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

Two plan-level choices extend what the box installs, both documented with
their proofs in [Profiles](profiles.md): `--quipu-flavor lancedb` builds the
reviewed Quipu revision with the lancedb feature compiled in (proven by the
server's `GET /version` per-feature compile map), and
`--embedding-model <spec.toml>` provisions checksum-pinned embedding-model
artifacts (a mismatched download is deleted and fails the step; verify
re-hashes the artifacts on disk).

## Claude Code cloud environments

`scripts/setup-environment.sh` is the version-controlled copy of the setup
script for a Claude Code cloud environment. Paste its body into the Setup
script field (claude.ai/code → the cloud icon above the message box →
Add/edit environment) — the field takes a script, not a path, and it runs
before the repo is available, which is why the file exists to be pasted from
rather than executed.

It bootstraps quipu (checksummed prebuilt `quipu` and `quipu-server` release
binaries), caboodle itself (prebuilt release binary via `scripts/install.sh`),
the shared tooling the stack's quality gates need (`just`, `bd`,
`pre-commit`, `mdbook`, `mdbook-mermaid`, `cffi`), and stages the stack
knowledge packs into `~/.caboodle/packs/`, verified with
`quipu pack --verify` — a pack that fails verification is deleted rather
than trusted. It deliberately does **not** build the rest of the corpus:
that is caboodle's own job, and a session that needs the full stack runs
`caboodle install` against a reviewed plan so every tool is proved rather
than assumed.

Set `CABOODLE_ALLOW_SOURCE_FALLBACK=1` only when the Quipu release artifact is
unavailable and the environment has enough time and memory for a Rust build.
The normal setup path never compiles Quipu from source.

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

### Shipping a pack from a repository

Each repository can publish its graph beside its release binaries as a Quipu
share. A `.qpack.tar.gz` release asset is a deterministic archive of the same
text bundle, not a SQLite database:

```bash
quipu share --graph https://example.org/knowledge/repository/example \
  --db .bobbin/quipu/quipu.db --out repository-share
tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
  -czf repository.qpack.tar.gz -C repository-share .
```

Publish the archive as an immutable release asset. A clone points Quipu at the
asset; Quipu fetches bounded bytes, verifies the manifest and payload, and opens
a fresh in-memory store without a local download step:

```bash
quipu import \
  https://github.com/scbrown/quipu/releases/download/quipu-ai-v0.3.29/quipu-quipu-ai-v0.3.29-repository.qpack.tar.gz
```

For a modified graph, `quipu share --since <parent-share> --out <delta-dir>`
writes a parent-bound SPARQL 1.1 Update delta. Import rejects a wrong parent,
changed update bytes, unrestricted operations, or a mismatched result digest.
Bobbin's `.bbpack` index remains a separate binary release artifact.

## Uninstall

Remove only the Caboodle binary and optional local Caboodle state. Installed
corpus tools are intentionally left in place because they may be shared with
other workflows:

```bash
rm "${CARGO_HOME:-$HOME/.cargo}/bin/caboodle"
# Optional, from the directory where Caboodle was run:
rm -r .caboodle
```

## Opt-in published binary updates

After the initial install and plan review, track published stable releases for a
selected binary without waiting for this Caboodle build's reviewed pins to change:

```bash
caboodle update-release --tool bobbin --check
caboodle update-release --tool bobbin
caboodle update-release --tool yupana
caboodle update-release --tool desire-path
```

This path currently supports Linux x86_64. The tool must already be installed on
PATH and selected in the retained plan. `--plan` and `--state` select the existing
files; the command never starts a new interview or changes the selected profile.
Each invocation changes one tool, not the whole stack. Quipu server/CLI source
convergence, Camayoc source bundles, and crew runtime updates retain their own
contracts and are not silently included.

The updater resolves GitHub's latest published stable release, requires the named
binary archive and checksum asset, checks the downloaded SHA256 and executable's
version, then atomically replaces the PATH entry. Symlink targets are not written
through. It retains the previous bytes, runs the existing adapter's functional
verification, and only then records the new version as verified. Failed proof
restores the backup. An interrupted update leaves a journal; the next invocation
restores its previous artifact before doing anything else, including during a hold.

A numerically newer installed version is preserved. Equal versions with different
binary bytes are ambiguous (for example a source build ahead of its release) and
are refused. Unreadable versions, missing assets and wrong checksums are errors,
never evidence of a current install. The older reviewed-pin `update` path also
refuses to replace a newer installed binary or an ambiguous equal-version build.
For initial or missing installations, use `apply` first.

Create `~/.caboodle/hold` to defer new release lookup and installation, or set
`CABOODLE_HOLD_FILE` to a shared host hold path. The updater uses an OS-held lock
beside the state file to serialize release updates; backups are retained under
`release-backups/<binary>/<sha256>` in the same directory. A host scheduler must
bound total runtime and retain stdout/stderr. This command does not itself install
a timer or claim that another host or a long-running process has updated.

`caboodle update-self` applies the same release, checksum, downgrade, backup and
interruption-recovery controls to Caboodle itself. Its post-install contract
requires the new binary's version and `update-release --help` to answer correctly.
Use `update-self --check` to inspect metadata. Bootstrap from a published release
before scheduling this command; a same-version source build with different bytes
is deliberately refused rather than silently overwritten.
