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

The stack has a store that governs what a fact may claim
([quipu](https://github.com/scbrown/quipu)), a keeper that decides how knowledge
earns its way in ([camayoc](https://github.com/scbrown/camayoc)), an index that
serves it back into agent context ([bobbin](https://github.com/scbrown/bobbin)),
and a graph that knows what your code touches
([yupana](https://github.com/scbrown/yupana)). What it did not have is the first
hour: a fresh machine, a person or an agent, and a working stack at the end of it.

That is this repo's job. Caboodle is an **AI-installable install wizard**: an LLM
agent — or a human answering the same interview — drives the install end to end,
and every claim along the way is a proof, not a banner.

## 🧵 Role in the stack

```text
    caboodle                      (interview → plan → apply → verify → observe)
        │  installs, proves, and watches ↓
        ▼
  quipu · bobbin · camayoc · yupana · desire-path · shantytown / creel · shanty
        │
        ▼
    prometheus                    (every tool's metrics, consolidated)
```

- Caboodle sits **above** the corpus: it never re-implements a tool's job, it
  installs the tool and then makes the tool demonstrate its own job.
- Camayoc ends at knowledge-graph bootstrapping; caboodle begins at
  "I have a fresh machine and I want this stack."

## 🔍 Three proofs, not one

1. **Installed** — proven by version read-back, never by exit code.
2. **Working** — a per-tool functional round-trip: quipu accepts an episode and a
   control-gated query finds it; bobbin indexes a fixture and search returns it;
   yupana answers `impact` on a fixture repo. Every check pairs with a **negative
   control** — the selftest that corrupts a byte and goes red — so a pass is
   capable of failing.
3. **Observable** — Prometheus actually scrapes each tool's declared exporter.
   Caboodle generates the scrape config, a starter alert pack, and one
   consolidated dashboard.

## 🪢 What this repo owns

1. **The interview** — `caboodle init --guided`: what do you want (knowledge
   graph only? retrieval? a full crew?), where does it run, do you have
   Prometheus or should I bring one? Answers become a **plan file** the user or
   agent reads before anything executes. Two-phase, always: nothing installs on
   the strength of a conversation alone.
2. **The engine** — plan → apply → verify over the corpus, driving each tool
   through the conventions the repos already follow: CLI verbs
   (`<tool> selftest`, `<tool> health`, `<tool> init`) and `just` recipes.
   **Convention over manifest** — there is no per-repo config file to parse or
   rot. A state file records what is installed *and last proven working*;
   re-running converges; a failed step names itself.
3. **Profiles** — `kg` (quipu + camayoc), `retrieval` (+ bobbin), `code-intel`
   (+ yupana), `crew`, `everything`. The `crew` profile is a choice:
   **shantytown, creel, both, or standalone** — a tmux-resident crew on a host,
   parallel agent bursts in the browser, the pair, or no crew layer at all.
   Both harnesses are first-class: **Claude Code and codex** can each drive the
   install, and crews can run roles on either.
4. **The observability weave** — each tool's metrics contract is catalogued in
   camayoc (meaning and ownership live in the graph the box just installed),
   and wired into Prometheus by generated config.
   And caboodle is itself a **quipu emitter**: every install and verification
   outcome becomes an episode through camayoc's ingress discipline — what is
   installed, at what version, last proven working when — so a fresh box's
   first knowledge domain is the box itself.
5. **`skills/caboodle`** — the agent-facing skill: install, verify, diagnose,
   upgrade. Its sibling `skills/camayoc` (the ontology-bootstrap interview)
   ships with camayoc; caboodle installs both.

## 🧰 What runs today

The plan/apply/verify engine, fixed convention adapters for Quipu and Bobbin,
and the resumable guided interview run without a tool manifest or private
network assumptions:

```bash
cargo install --git https://github.com/scbrown/caboodle --locked

# Answer the use-shaped interview. It asks who the themed crew members are and
# which questions the finished graph must answer, then only writes a plan.
caboodle init --guided

# Or supply the same intended-use contract non-interactively.
caboodle plan --profile retrieval --intent caboodle-intent.toml

# Converge the reviewed plan, then prove both tools with isolated round trips.
caboodle apply
caboodle verify
caboodle verify-questions

# Or run both phases together after review.
caboodle install
```

`apply` installs a missing released tool and reads its version back; Bobbin comes
from its checksummed release bundle (including its runtime), not a source build.
A successful installer exit alone is not accepted. `verify` uses temporary
isolated stores: it proves a marker is absent first, writes/indexes it, and then
requires the reader path to return it. Progress is written atomically to
`.caboodle/state.json`, so rerunning converges and preserves a still-current
verified result.

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
Bobbin), `code-intel` (plus Yupana), `everything` (plus Desire Path), and
`crew` (Shantytown, Creel, both, or standalone). CABOODLE installs
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

Yupana installs from its checksum-pinned v0.6.4 release and proves `callers` on
an isolated fixture repository. Desire Path currently has no tags or release
assets, so CABOODLE pins public source revision `1ca7b36`, stamps that identity
into `dp version`, and proves an isolated ingest/list round trip. Neither
verification can write into the user's normal Yupana state or Desire Path DB.

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
