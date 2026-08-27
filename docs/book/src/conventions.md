# Convention over manifest

There is no per-repo manifest file to parse or rot. Each tool is
caboodle-ready by exposing the pattern the repos already follow:

- conventional CLI verbs: `<tool> selftest`, `<tool> health`, `<tool> init`
- and/or `just` recipes: `just install`, `just verify`

Caboodle discovers and drives these directly. Metrics contracts live as
camayoc catalogue entries, not per-repo config. The readiness contract every
corpus repo adopts:

1. The conventional CLI surface above.
2. A `selftest` with a negative control.
3. A metrics story, catalogued in camayoc.
4. Versioned release artifacts — install never means build-from-source.
5. `init` writes a starter config; file + env with documented precedence.
6. A health probe distinct from the selftest.
7. No environment-specific names in defaults or docs.
