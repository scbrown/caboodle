# Profiles

- `kg` — quipu + camayoc
- `retrieval` — + bobbin
- `code-intel` — retrieval + Yupana; isolated `analyze`/`callers` proof
- `crew` — a **choice**: shantytown, creel, both, or **standalone** (no crew
  layer — the tools serve a single agent or a human directly). A tmux-resident
  crew on a host, parallel agent bursts in the browser, the pair, or neither.

Both harnesses are first-class: **Claude Code and codex** can each drive the
install, and when a crew is chosen the wizard asks which harness each role runs
on, emitting the right per-harness configuration.
- `everything` — code-intel + Desire Path; isolated failure-ingest/read proof

Any profile can extend its corpus with repeatable `--share` selections because
all profiles include Quipu. Shares require an explicit `--quipu-db`, are staged
through Quipu's canonical import command during `apply`, and are never promoted
to ROOT automatically. The share choice is part of the reviewable plan rather
than a hidden installer input.

The crew choice is emitted today:

```console
caboodle plan --profile crew --crew shantytown
caboodle plan --profile crew --crew creel
caboodle plan --profile crew --crew both
caboodle plan --profile crew --crew standalone
```

`both` is not fan-out. Its plan names Shantytown as `durable_owner`, Creel as
`burst_owner`, and `explicit-handoff` as the only routing mode. This keeps one
owner per task until the cross-harness handoff contract lands.

The reviewed plan is also the one source for identity, model selection and tool
policy. Project it through harness-owned adapters:

```console
caboodle project-settings
```

In `both` mode this writes `shantytown.settings.json` and
`creel.settings.json`. Their `shared` object is byte-equivalent. Their security
surfaces are intentionally not: Shantytown owns hooks and host-workspace access;
Creel owns browser-BYO credentials (write-only to agents) and operator-granted
browser permissions. Neither adapter is allowed to emit the other's fields.

The engine applies and verifies Quipu + Camayoc for `kg`, adding Bobbin for
`retrieval`, Yupana for `code-intel`, and Desire Path for `everything`. A crew
plan then applies only the selected runtime: the released
Shantytown wheel, the static Creel browser bundle, both in their declared owner
roles, or neither for standalone. Both distributions are checksum-pinned.

Shantytown verification reads back `st 0.4.0`. Creel's browser security and
admission facts cannot honestly be inferred by the host-side installer, so its
adapter consumes the versioned [crew capability contracts](crew-contracts.md).
It refuses when either contract is absent, when a required doctor result is not
`pass`, or when the governor does not return a measured `admit`.

Camayoc currently distributes its bootstrap ontology, shapes, queries, and gate
proof as a repository bundle; CABOODLE pins and checksums that bundle until the
designed `core.qpack` artifact is published.

## Quipu install flavor

Every profile installs Quipu, and the plan's `quipu_flavor` field decides how.
The default, `release`, installs the reviewed release feature set, which
deliberately excludes the `lancedb` cargo feature — so
`vector.backend = "lancedb"` refuses at startup on a default box. The
`lancedb` flavor builds the **same reviewed revision** with
`--features full,lancedb`; only the feature list changes, so choosing the
flavor can never pull in an unreviewed Quipu:

```console
caboodle plan --profile retrieval --quipu-flavor lancedb
```

The flavor is proven, never assumed. `quipu --version` cannot say which
features were compiled in, so verification asks the running Quipu server's
`GET /version` per-feature compile map and goes red when the plan asked for
`lancedb` and the map lacks it — or when the server reports no map at all,
because an unprovable claim is a refusal, not a pass. An installer exit code
is never accepted as the proof. Because the flavor is invisible to plain
version read-back, `check-updates` cannot see flavor drift; `verify` is the
command that catches it, and its failure names the exact reinstall command
that converges.
