# The stack

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
this workflow is the [`caboodle` skill](https://github.com/scbrown/caboodle/blob/main/skills/caboodle/SKILL.md), and the full
design is in the [book](introduction.md).

## Related projects

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

See [the corpus](corpus.md) for share import and [profiles](profiles.md) for
which tools each selection installs.
