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

## Uninstall

Remove only the Caboodle binary and optional local Caboodle state. Installed
corpus tools are intentionally left in place because they may be shared with
other workflows:

```bash
rm "${CARGO_HOME:-$HOME/.cargo}/bin/caboodle"
# Optional, from the directory where Caboodle was run:
rm -r .caboodle
```
