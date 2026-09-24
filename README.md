<p align="center">
  <img src="assets/header.svg" width="100%" alt="Animated banner — a teal kit box with a tipping lid and three flowing, knotted cords"/>
</p>

<p align="center">
  <img src="assets/logo.svg" width="300" alt="Caboodle logo — a light-teal tackle box open with pink fold-out trays, each compartment holding a tool of the stack"/>
</p>

<h1 align="center">caboodle</h1>

<p align="center">
  <em>🧰 The whole kit — one wizard that installs the stack, proves it works, and watches it run</em>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"/></a>
  <a href="https://github.com/scbrown/quipu"><img src="https://img.shields.io/badge/stack-quipu-8B5E3C.svg" alt="Part of the quipu stack"/></a>
</p>

> *"Kit and caboodle" — from Dutch* boedel*, one's whole estate and effects. Everything,
> together, nothing left on the dock.*

**Caboodle installs a set of open-source tools that give AI coding agents
(Claude Code, Codex, Cursor) a memory and a map of your code, then proves each
tool works on your machine.** A short interview writes a plan, one command
installs and checks everything in it, and the result is a list of passed proofs
rather than a list of exit codes.

## Why you would want it

Coding agents forget everything between sessions and see your repository one
file at a time. The tools caboodle installs fix both:

- **[quipu](https://github.com/scbrown/quipu)** stores what your agents learn as
  a knowledge graph that refuses facts that break its rules.
- **[camayoc](https://github.com/scbrown/camayoc)** loads the starter vocabulary
  into quipu and decides how new knowledge earns its way in.
- **[bobbin](https://github.com/scbrown/bobbin)** indexes your repositories and
  serves search and context to agents over MCP.
- **[yupana](https://github.com/scbrown/yupana)** knows which functions call
  which, so an agent can check the blast radius before it edits.
- **[desire-path](https://github.com/scbrown/desire-path)** records the tool
  calls your agents get wrong, so you can see what to fix.

Each tool installs on its own. Caboodle is the part that picks the right
versions, installs them together, and runs a real round trip through each one
(write something, read it back, and first prove it was not already there).

## Quickstart

```bash
# 1. Install the caboodle binary (checksummed release; read the script first if you like)
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh
sh install.sh && export PATH="$HOME/.cargo/bin:$PATH"

# 2. Answer five short questions; this writes caboodle-plan.toml and changes nothing else
mkdir my-stack && cd my-stack
caboodle init --guided

# 3. Install and prove every tool in the plan
caboodle install
```

Before step 3, run `caboodle doctor`. It changes nothing and lists every
missing prerequisite, unsupported platform, and stale or shadowed binary that
would stop the install. Fix each `FAIL` line and run it again.

A finished install ends with one `verified` line per tool:

```text
quipu: verified
camayoc: verified
bobbin: verified
```

Run everything from the same directory. Caboodle keeps its plan and progress
there, and an interrupted `init` or `install` resumes when you rerun it.

### Answering the interview

| prompt | what to type |
|---|---|
| `profile` | `retrieval` for a first install. See [profiles](#pick-a-profile). |
| `what should this installation help you do?` | One plain sentence. It is recorded in the plan. |
| `how many themed crew members` | `0` unless you run a named team of agents. |
| `how many ontology questions` | Press **enter** on a first install: caboodle uses a built-in self-test question that checks the store answers a query, and asks nothing else. Or type a number to write your own; each question is a check the finished graph must pass. |
| the question's five fields | Only if you typed a number. A question in words, its answer shape, the seed fact that answers it, a SPARQL `SELECT` or `ASK` query, and a word the answer must contain. |

To skip the interview, write the same answers to a file and pass it in. A
starter file is in [`examples/caboodle-intent.toml`](examples/caboodle-intent.toml):

```bash
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/examples/caboodle-intent.toml
caboodle plan --profile retrieval --intent caboodle-intent.toml
```

## Before you start

**Platforms.** Linux x86_64 needs no Rust toolchain: each tool comes from a
checksummed release or source archive, and Go builds desire-path. On other
platforms quipu and yupana have no release yet, so you build them once with
Rust; `caboodle doctor` prints the exact command.

| | Linux x86_64 | macOS arm64 | macOS x86_64 | Linux arm64 |
|---|---|---|---|---|
| caboodle | release | release | release | build with `cargo install --git` |
| quipu | release | build with cargo | build with cargo | build with cargo |
| camayoc | source archive | source archive | source archive | source archive |
| bobbin | release | release | release | release |
| yupana | release | build with cargo | build with cargo | build with cargo |
| desire-path | built with Go | built with Go | built with Go | built with Go |

**Commands on PATH.** `curl`, `tar`, `git`, `bash`, `python3`, and `sha256sum`
(recent macOS ships it; otherwise `brew install coreutils`). The `everything`
profile also needs Go 1.22+. Rust is only needed for the source builds above.

**Accounts and servers.** None. Nothing talks to a private network, and no
token is needed to install. A token (`QUIPU_AUTH_TOKEN`) matters only when you
send install records to a remote quipu with `flush-episodes`.

## Pick a profile

| profile | installs | pick it when |
|---|---|---|
| `kg` | quipu, camayoc | you want the knowledge graph only |
| `retrieval` | kg + bobbin | you want agents to search your code (start here) |
| `code-intel` | retrieval + yupana | you also want call graphs and impact checks |
| `everything` | code-intel + desire-path | you want the whole kit; needs Go |
| `crew` | kg + bobbin + a crew runner | you run several agents at once with [shantytown](https://github.com/scbrown/shantytown) or [creel](https://github.com/scbrown/creel) |

## What install leaves on your machine

- Binaries in `~/.cargo/bin` (or `$CARGO_HOME/bin`). Keep that directory on your
  PATH, **ahead of** any other directory that holds an older copy of the same tool
  (`~/.local/bin` is the usual one). The shell runs the first copy it finds.
- Unpacked releases under `~/.local/share/caboodle/`.
- One setting, `[quipu.owl] reactive_materialize = true`, merged into
  `~/.config/bobbin/config.toml`. Other settings in that file are kept.
- In the directory you ran it from: `caboodle-plan.toml` and `.caboodle/` (interview,
  state, queued install records).

Every verification check uses a throwaway directory, camayoc's included. The camayoc
check starts its own `quipu-server` on a free localhost port with a temporary store,
loads camayoc's vocabulary and one marker node into it, and stops it afterwards. It
never touches the server at `QUIPU_SERVER`. To set camayoc up against your real quipu
server, run the bundle's bootstrap yourself as a separate step:
`QUIPU_SERVER=<your server> bash ~/.local/share/caboodle/camayoc/<revision>/scripts/bootstrap.sh`.

To remove caboodle, delete `~/.cargo/bin/caboodle` and the `.caboodle/`
directory. Installed tools stay, because other workflows may use them. The
[install guide](docs/book/src/installing.md) covers release pins, source
installs, and updates.

## Connect the tools to your agent

`caboodle install` installs and proves the binaries. It does not register MCP
servers with your agent yet. Register them yourself after a green install. For
Claude Code:

```bash
claude mcp add bobbin -- bobbin serve
claude mcp add yupana -- yupana serve   # code-intel and everything profiles
claude mcp list                         # each should show as connected
```

Bobbin's MCP server also carries quipu's knowledge-graph tools. For Cursor and
other MCP clients, use the same command and arguments in the client's MCP
config. Index a repository before searching it: `cd your-repo && bobbin init &&
bobbin index`.

## When something fails

- **`caboodle doctor`** first. It names the blocker and the fix.
- **`<tool> install step ... no checksummed CABOODLE release for <platform>`**:
  build that tool with the `cargo install` command doctor prints, then rerun
  `caboodle install`. Caboodle adopts an installed tool that passes its checks.
- **`<tool> functional verification`**: the tool installed but its round trip
  failed. The error includes the tool's own output. Fix it and rerun
  `caboodle install`; tools that already passed are not redone.
- **macOS: bobbin panics with `Failed to load ONNX Runtime dylib`**: bobbin
  looks for its bundled runtime next to the `~/.cargo/bin/bobbin` symlink rather
  than next to the real binary. Until bobbin resolves the symlink, run
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

## Words you will see

| word | meaning |
|---|---|
| plan | `caboodle-plan.toml`, the reviewed list of what will be installed. Nothing installs without one. |
| state | `.caboodle/state.json`, what is installed and when it last passed its check. |
| negative control | The half of a check that proves the thing is absent first, so a pass could have been a fail. |
| episode | A batch of facts written to quipu in one go. |
| knot | One write to quipu over HTTP (`/knot`). |
| ontology, shapes | The vocabulary quipu accepts and the rules (SHACL) that reject bad facts. |
| share, qpack | A portable export of a quipu graph that another store can import. |
| crew | Several named coding agents that work together, run by shantytown (terminal) or creel (browser). |
| bead, `br` | An issue in [beads](https://github.com/Dicklesworthstone/beads_rust), a git-friendly issue tracker agents use as memory. |
| MCP | Model Context Protocol, how an agent calls a tool's server. |

## How it works

```text
    caboodle                      (interview → plan → apply → verify → observe)
        │  installs, proves, and watches ↓
        ▼
  quipu · bobbin · camayoc · yupana · desire-path · shantytown / creel
        │
        ▼
    prometheus                    (every tool's metrics, consolidated)
```

Caboodle never re-implements a tool's job. It installs the tool and then makes
the tool demonstrate its own job, through three proofs:

1. **Installed**: proven by reading the version back, never by an exit code.
2. **Working**: a per-tool round trip. quipu accepts an episode and a query
   finds it; bobbin indexes a fixture and search returns it; yupana finds a
   caller in a fixture repository. Every check first proves its marker is
   absent, so a pass is capable of failing.
3. **Observable**: caboodle generates Prometheus scrape config, starter alerts,
   and one dashboard for the selected tools, for you to review and deploy.

Install and verification outcomes are also queued as quipu episodes, so a fresh
box's first knowledge is a record of the box itself. The agent-facing version of
this workflow is the [`caboodle` skill](skills/caboodle/SKILL.md), and the full
design is in the [book](docs/book/src/introduction.md).

## Reference: every command

The quickstart covers `init`, `plan`, `doctor`, and `install`. The rest of the
command surface, for reviewed updates, shared graphs, observability, and install
records:

```bash
# Build caboodle from source instead of the release installer.
cargo install --git https://github.com/scbrown/caboodle --locked

# Report what would block an install here. Changes nothing; exits nonzero on a blocker.
caboodle doctor

# Answer the use-shaped interview. It asks who the themed crew members are and
# which questions the finished graph must answer, then only writes a plan.
caboodle init --guided

# Or supply the same intended-use contract non-interactively.
caboodle plan --profile retrieval --intent caboodle-intent.toml

# Optionally add canonical Quipu share directories to the reviewed corpus.
# The target database is explicit; apply stages imports but never promotes them.
caboodle plan --profile retrieval --share ./team-share --quipu-db ./knowledge.db

# Converge the reviewed plan, then prove each tool with a round trip.
caboodle apply
caboodle verify
caboodle verify-questions

# Or run both phases together after review.
caboodle install

# Ask whether the running stack matches this Caboodle build's reviewed pins.
# The command exits nonzero when anything is missing, unreadable, or drifted.
caboodle check-updates

# Converge only drifted tools, read every version back, and functionally verify
# each changed tool before recording it as current.
caboodle update

# A full tool + crew plan is explicit and reviewable.
caboodle plan --profile everything --crew both
```

`apply` installs a missing released tool and reads its version back; Bobbin comes
from its checksummed release bundle (including its runtime), not a source build.
A successful installer exit alone is not accepted. `verify` proves a marker is
absent first, writes/indexes it, and then requires the reader path to return it.
Every check uses a temporary isolated store; camayoc's starts and stops its own
scratch quipu server (see
[what install leaves on your machine](#what-install-leaves-on-your-machine)). Progress is written atomically to
`.caboodle/state.json`, so rerunning converges and preserves a still-current
verified result.

`check-updates` compares the running identities with the release versions and
source revisions pinned in the selected plan's Caboodle build. It does not use
an unreviewed “latest” endpoint. `update` is the mutation half: it installs only
drifted selections through their existing checksummed/source-pinned adapters,
requires version read-back to match the reviewed identity, runs the adapter's
functional proof, and only then records an `updated` transition. A second run is
current and performs no installation. This keeps release selection in review
while making deployed-versus-released drift executable rather than anecdotal.

If the interview loses input or its caller stops, rerun `caboodle init --guided`:
accepted answers resume from `.caboodle/interview.toml`. The session disappears
only after the complete plan has been written successfully.

The interview works backward from expected use. Each crew member has a free-form
theme, domain, and role rather than a closed role enum. Each anticipated ontology
question records its expected answer shape, the seed facts needed to exercise it,
an executable `SELECT`/`ASK` query, and a result marker. `caboodle
verify-questions` runs those reader-path checks after installation. Empty,
duplicate, credential-bearing, or non-executable entries are refused before a
plan is written.

Installable profiles today are `kg` (Quipu + Camayoc), `retrieval` (plus
Bobbin), `code-intel` (plus Yupana), `everything` (plus Desire Path and,
when selected with `--crew`, Shantytown/Creel), and `crew` (Shantytown, Creel,
both, or standalone). CABOODLE installs
checksum-pinned Shantytown and Creel distributions. Creel verification remains
browser-owned: it requires explicit machine-readable doctor and admission
documents and refuses missing, unknown, unredacted, or non-admit evidence. See
the [crew capability contracts](docs/book/src/crew-contracts.md).

Until Camayoc publishes `core.qpack`, CABOODLE installs its
checksum-pinned bootstrap distribution: ontology, shapes, queries, and the
same fail-closed gate proof. Verification then proves a separate first ingest
with an absent control, reader-path retrieval, and an idempotent replay.
Use `--skip-install` when package installation belongs to another system; the
version and functional checks still run.

Yupana installs from its checksum-pinned v0.7.0 release and proves `callers` on
an isolated fixture repository. Desire Path publishes no release archive, so
CABOODLE builds it from the revision its `v0.2.1` tag names (`6c5840f`), stamps
that identity into `dp version`, and proves an isolated ingest/list round trip. Neither
verification can write into the user's normal Yupana state or Desire Path DB.

`caboodle render-observability` converts the reviewed selection plus explicit
generic targets into Prometheus scrape config, starter alerts, a dashboard, and
versioned contracts. `validate-observability` fails closed on missing targets or
uncovered metrics; rendering is review-only and never claims a live scrape.

Applied/verified transitions and `br` JSONL lifecycle snapshots queue as
redacted, content-addressed Quipu episodes. `flush-episodes` requires an HTTPS
endpoint plus an environment-only token, proves a query marker first, and keeps
the identical bytes pending on ambiguous delivery.

Profiles can also consume one or more directories produced by `quipu share`.
Caboodle passes each directory unchanged to `quipu import`, records the returned
share ID, staging graph, eligibility, and blockers in `.caboodle/state.json`,
and leaves ROOT promotion to the explicit `quipu import promote` review step.
Non-conforming or off-vocabulary shares remain quarantined and visible rather
than being reshaped into a Caboodle-specific bundle.

## 🧺 The stack

| repo | what it is |
|---|---|
| [quipu](https://github.com/scbrown/quipu) | AI-native knowledge graph with strict ontology enforcement |
| [bobbin](https://github.com/scbrown/bobbin) | repo indexing and context injection for AI agents |
| [camayoc](https://github.com/scbrown/camayoc) | bootstrap ontology, knowledge ingress, knowledge packs |
| [yupana](https://github.com/scbrown/yupana) | structural code intelligence — impact before you touch |
| [desire-path](https://github.com/scbrown/desire-path) | turn AI hallucinations into feature requests |
| [shantytown](https://github.com/scbrown/shantytown) | a small harness for running a crew of coding agents |
| [creel](https://github.com/scbrown/creel) | parallel agent bursts, entirely in the browser |
| [shanty](https://github.com/scbrown/shanty) | a terminal multiplexer wrapper that makes tmux feel like home |
| [skein](https://github.com/scbrown/skein) | portable agentic skills — shell + HTTP only |
| [beads](https://github.com/Dicklesworthstone/beads_rust) | issue tracking as agent memory — `br`, SQLite + JSONL |
| [shuttle](https://github.com/scbrown/shuttle) | workflow engine — signed runs, windowed export, frozen history |

## 📜 License

[MIT](LICENSE)

<p align="center">
  <img src="assets/footer.svg" width="100%" alt="Animated footer — a woven band of teal, pink, and ochre cords with sliding beads"/>
</p>
