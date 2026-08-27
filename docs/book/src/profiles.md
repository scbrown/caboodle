# Profiles

- `kg` — quipu + camayoc
- `retrieval` — + bobbin
- `code-intel` — + yupana
- `crew` — a **choice**: shantytown, creel, both, or **standalone** (no crew
  layer — the tools serve a single agent or a human directly). A tmux-resident
  crew on a host, parallel agent bursts in the browser, the pair, or neither.

Both harnesses are first-class: **Claude Code and codex** can each drive the
install, and when a crew is chosen the wizard asks which harness each role runs
on, emitting the right per-harness configuration.
- `everything`

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
`retrieval`. Camayoc currently distributes its bootstrap ontology, shapes,
queries, and gate proof as a repository bundle; CABOODLE pins and checksums
that bundle until the designed `core.qpack` artifact is published.

Crew plans
already carry the reviewed runtime choice; the Shantytown and Creel install
adapters remain Phase 1 work.
