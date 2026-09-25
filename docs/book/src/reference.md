# Reference

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
[installed files](installing.md#installed-files)). Progress is written atomically to
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
the [crew capability contracts](crew-contracts.md).

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

## Command and option index

Run `caboodle <command> --help` for that command’s complete option syntax.
The table covers the current source tree; published binaries may lag new commands.

| Command | Purpose and options |
|---|---|
| `doctor` | Read-only preflight; `--plan`. |
| `init` | Interview; `--guided`, `--output`, `--session`. |
| `plan` | Write selection; `--profile`, `--crew`, `--intent`, `--output`, `--share`, `--quipu-db`, `--quipu-flavor`, `--embedding-model`. |
| `apply` | Install; `--plan`, `--state`, `--skip-install`. |
| `verify` | Functional proof; `--plan`, `--state`, `--creel-doctor`, `--creel-admission`. |
| `install` | Apply then verify; all apply and verify options. |
| `check-updates` | Compare reviewed pins; `--plan`. |
| `update` | Converge reviewed pins; `--plan`, `--state`, `--creel-doctor`, `--creel-admission`. |
| `update-release` | Unix published release update; `--tool`, `--check`, `--plan`, `--state`. |
| `update-self` | Unix self-update; `--check`, `--state`. |
| `project-settings` | Rig registration; `--root`, `--agent`, `--registry`; or `--policy-only` with `--plan`, `--output`. |
| `verify-questions` | Execute intent questions; `--plan`, `--db`. |
| `render-observability` | Generate reviewable artifacts; `--plan`, `--targets`, `--output`. |
| `validate-observability` | Check artifact contracts; `--output`. |
| `queue-br` | Queue lifecycle records; input JSONL argument, `--queue`. |
| `flush-episodes` | Deliver queued records; `--queue`, `--endpoint`. |

## Files and configuration

The [configuration reference](configuration.md) documents plan, state, intent,
environment variables and observability targets.

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
