---
name: caboodle
description: Install, verify, diagnose, and upgrade the quipu-stack tool corpus through caboodle's plan → apply → verify engine. The guided interview lands in Phase 1; Phase 0 can already execute a reviewed plan.
---

The full agent interview arrives in Phase 1. Until then, drive the Phase 0 engine
through its explicit two-phase contract:

1. Run `caboodle plan --profile kg|retrieval` and present the generated
   `caboodle-plan.toml` for review. This step changes no installed tools.
2. After review, run `caboodle apply`. Use `--skip-install` only when an external
   package manager owns installation; version read-back still must pass.
3. Run `caboodle verify`. Treat any named adapter failure as a failed install;
   the command uses an isolated negative-control + write/index + read-back proof.

`caboodle install` combines steps 2 and 3 for an already-reviewed plan. Do not
invent per-tool manifests or environment-specific defaults; adapters are the
in-code convention boundary.
