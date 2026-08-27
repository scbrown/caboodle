---
name: caboodle
description: Install, verify, diagnose, resume, or update the Quipu-stack tool corpus with CABOODLE. Use when an agent or human wants a review-first guided installation of Quipu and Bobbin, needs to resume an interrupted CABOODLE interview or apply, or needs functional proof that the installed stack works.
---

# CABOODLE

Drive one reviewable sequence: interview → plan review → apply → verify. Use the
same CLI path in every harness; require only a shell for the current profiles.

## Bootstrap and preflight

1. If `caboodle` is absent and the request authorizes installation, install the public source with:
   `cargo install --git https://github.com/scbrown/caboodle --locked`.
   Otherwise stop and request authorization; bootstrapping CABOODLE is itself an
   installation.
2. Prove the binary answers: `caboodle --version`.
3. Run `skills/caboodle/scripts/check-surface.sh "$(command -v caboodle)"`
   when this skill directory is available. A failed surface check is a bootstrap
   failure; do not continue with guessed commands.

Do not print credentials or introduce private endpoints. Do not delete
`.caboodle/` to recover from an error: it contains the resumable interview and
last-proven state.

## Interview and plan review

For a guided install, run:

```sh
caboodle init --guided
```

Run from the same working directory throughout: default session, plan, and
state paths are relative. Before resuming, inspect `.caboodle/interview.toml` to
confirm the accepted answers. An older `caboodle-plan.toml` or state file is not
evidence that this interview completed. If custom paths or the original working
directory are unknown, stop and ask; do not search broadly or start a new
session that could fork the plan.

If input ends, rerun the exact command. Accepted answers resume from
`.caboodle/interview.toml`; no install has happened. For automation with choices
already supplied, use `caboodle plan --profile kg|retrieval`. Crew plans may be
generated with `--profile crew --crew shantytown|creel|both|standalone`, but the
current release does not install the crew harnesses yet.

Read `caboodle-plan.toml` back to the operator. Confirm the selected profile and
tools. Never apply a conversational answer directly: the plan file is the
review boundary. If the request did not already authorize installation, stop
after presenting the plan. Do not run `apply`, `install`, `verify`, or
`project-settings` at that boundary: verify performs functional writes and
updates state, while settings projection writes configuration artifacts.

## Apply and verify

After plan approval:

```sh
caboodle apply
caboodle verify
```

`apply` proves each installed binary by version read-back. `verify` then runs an
isolated negative control, functional write/index, and reader-path proof for
each selected adapter. Both commands update `.caboodle/state.json` atomically,
so rerunning converges.

Use `caboodle install` only as shorthand for apply + verify after the plan has
already been reviewed. Use `--skip-install` only when an external package
manager owns installation; it still requires version and functional proof.

Report the selected profile, each applied version, each verified adapter, and
the retained plan/state paths. Exit status or an “installed” banner alone is
not proof.

## Diagnose and resume

- Interview paused at EOF: rerun `caboodle init --guided` with the same
  `--session` and `--output` paths if custom paths were used.
- Apply failed before version read-back: keep the state file, fix the named
  installer or PATH problem, and rerun `caboodle apply`.
- Apply succeeds with `--skip-install` but normal apply fails: the external
  package manager boundary is the likely cause; state that as inferred until
  its own logs prove it.
- Verify names an adapter: run `caboodle verify` again only after addressing
  that adapter. Do not mark the stack healthy because another adapter passed.
- Plan parsing fails: repair or regenerate the plan; never bypass validation by
  editing state.

CABOODLE 0.1 has no `upgrade` subcommand. Do not invent one. Update the
`caboodle` binary through its documented install path, prove `--version`, then
rerun apply and verify against the existing reviewed plan. A changed tool
version clears its prior verified state until functional verification passes.
