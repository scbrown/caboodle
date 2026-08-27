# Using caboodle

Caboodle ships a resumable interview and the plan/apply/verify engine with
adapters for Quipu and Bobbin.

```bash
cargo install --git https://github.com/scbrown/caboodle --locked

# Guided: answers are checkpointed, and completion only writes a plan.
caboodle init --guided

# Non-interactive: produces the same reviewable plan bytes.
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

If input ends partway through an interview, Caboodle stops without a plan and
keeps accepted answers in `.caboodle/interview.toml`. Run `caboodle init
--guided` again to resume at the next unanswered question. Invalid answers are
rejected and never produce a plan.

Use `--skip-install` when package installation belongs to another system; the
version and functional checks still run.
