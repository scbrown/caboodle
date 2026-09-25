# Getting started

Install Caboodle with the [release installer](installing.md), then try the
smallest profile: Quipu and Camayoc. Start in an empty directory.

```bash
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/examples/caboodle-intent.toml
caboodle plan --profile kg --intent caboodle-intent.toml
caboodle install
```

The plan command prints `plan: caboodle-plan.toml`. Read that file before
installing: it selects Quipu and Camayoc, and retains the example intent.
Installation prints progress on stderr and these lines on stdout:

```text
quipu: applied
camayoc: applied
stack configuration: applied
quipu: verified
camayoc: verified
```

Each verified line means an isolated functional round trip passed, including
a control proving the marker was absent before the write. The example intent
is a starting contract, not a seeded production graph; see [question verification](reference.md).

## Guided interview

Use `caboodle init --guided` instead of the first two commands to describe
your own use interactively. Run `caboodle doctor` before installing to see
platform or prerequisite blockers.

| prompt | what to type |
|---|---|
| `profile` | `retrieval` for a first install. See [profiles](profiles.md). |
| `what should this installation help you do?` | One plain sentence. It is recorded in the plan. |
| `how many themed crew members` | `0` unless you run a named team of agents. |
| `how many ontology questions` | Press **enter** on a first install: caboodle uses a built-in self-test question that checks the store answers a query, and asks nothing else. Or type a number to write your own; each question is a check the finished graph must pass. |
| the question's five fields | Only if you typed a number. A question in words, its answer shape, the seed fact that answers it, a SPARQL `SELECT` or `ASK` query, and a word the answer must contain. |

## Recovery

- **`caboodle doctor`** first. It names the blocker and the fix.
- **`<tool> install step ... no checksummed CABOODLE release for <platform>`**:
  build that tool with the `cargo install` command doctor prints, then rerun
  `caboodle install`. Caboodle adopts an installed tool that passes its checks.
- **`<tool> functional verification`**: the tool installed but its round trip
  failed. The error includes the tool's own output. Fix it and rerun
  `caboodle install`; tools that already passed are not redone.
- **macOS with Bobbin v0.16.2: bobbin panics with `Failed to load ONNX Runtime dylib`**: bobbin
  looks for its bundled runtime next to the `~/.cargo/bin/bobbin` symlink rather
  than next to the real binary. For that release, run
  `export ORT_DYLIB_PATH="$HOME/.local/share/caboodle/bobbin/v0.16.2/lib/libonnxruntime.dylib"`
  (add it to your shell profile) and rerun `caboodle install`.
- **A new build still behaves like the old one**: another copy earlier on PATH
  wins. Doctor lists every copy in the order your shell finds them, and
  `caboodle verify` fails with `SHADOWED` when the copy PATH runs reports a
  different version from the one caboodle installed. Remove the stale copy or
  move `~/.cargo/bin` earlier on PATH.
- **`no plan at caboodle-plan.toml`**: you are in a different directory from the
  one where you ran `caboodle init --guided`.

Do not delete `.caboodle/` to recover. It holds your interview answers and the
last proven state.

## Reproduce the README proof

On Linux x86_64, from a checkout, run `scripts/test-readme.sh`. It extracts the
README's shell blocks, runs the checksummed release installer and the three
first-success commands under `env -i` with a fresh home, then diffs stdout
against the README's own expected-output block. It does not reuse host binaries
for the stack. The Documentation workflow runs this proof on every push and PR.

For local link checks, install mdBook 0.5.4 and lychee 0.24.2, then run
`just docs-check`. `pre-commit install` enables the same check before commits.
The check builds the book under its Pages base path and checks files, anchors
and public URLs. The source-build route additionally requires Rust; agent
registration additionally requires the selected client and profile.
