# The corpus

| tool | role in the box |
|---|---|
| quipu | governed knowledge-graph store |
| bobbin | semantic index · retrieval · RAG |
| camayoc | ontology bootstrap, ingress discipline, knowledge packs |
| yupana | code-structure graph — impact, callers, verify |
| desire-path | hallucinations → feature requests |
| shantytown | crew harness (multi-agent, tmux-resident) |
| shuttle | workflow engine — signed runs, windowed export, frozen history |
| creel | browser-native crew — tabs as workers |
| shanty | the tmux the crew lives in |
| skein | portable agentic skills |
| beads (`br`) | issue tracking as agent memory — SQLite + JSONL |

## Extending the corpus with Quipu shares

Every profile includes Quipu and can stage git-native knowledge shares into an
explicit local database:

```console
caboodle plan --profile retrieval \
  --share ./team-knowledge \
  --share ./project-knowledge \
  --quipu-db ./knowledge.db
caboodle apply --skip-install
```

A share directory is the canonical output of `quipu share`: `manifest.json`,
`export.nt`, and `shapes.ttl`. Caboodle does not parse, rewrite, unpack, or wrap
those files. It invokes `quipu import <directory> --db <database>`, so Quipu
owns payload-hash verification, identity candidates, local SHACL validation,
and the staging/quarantine graph.

The returned `share_id`, graph, outcome, promotion eligibility, and blockers
are retained under `shares` in `.caboodle/state.json`. A quarantined import is
a successful safe consumption outcome that still needs review; Caboodle never
runs `quipu import promote`. Promotion into ROOT remains a separate explicit
operator decision using the share ID.
