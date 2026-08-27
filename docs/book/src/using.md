# Using caboodle

Phase 0 ships the plan/apply/verify engine with adapters for Quipu and Bobbin.

```bash
cargo install --git https://github.com/scbrown/caboodle --locked

# Review first: this only writes caboodle-plan.toml.
caboodle plan --profile retrieval

# Converge the reviewed plan, then prove both tools with isolated round trips.
caboodle apply
caboodle verify

# Or run both phases together after review.
caboodle install
```

`apply` installs a missing released tool and reads its version back — a
successful installer exit alone is never accepted. `verify` uses temporary
isolated stores: it proves a marker absent, writes it, and requires the reader
path to return it. Progress is written atomically to `.caboodle/state.json`, so
rerunning converges.

Use `--skip-install` when package installation belongs to another system; the
version and functional checks still run.
