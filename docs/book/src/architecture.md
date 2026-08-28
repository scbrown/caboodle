# Architecture

Caboodle's engine is a **plan → apply → verify** loop with a state file
recording what is installed *and last proven working*. Idempotent and
resumable: re-running converges, and a failed step names itself.

The wizard is an interview, not a form over install switches. `caboodle init
--guided` first elicits intended use, themed crew-member identities/domains/roles,
and the questions the installed graph must answer. Those answers become typed
plan data and executable reader-path contracts. The engine therefore builds
backward from expected retrieval rather than forward from whatever data happens
to be available.

In agent hands the same flow is a skill; non-interactive callers provide the
same schema in `caboodle-intent.toml`. Output is always the plan file —
two-phase, so nothing installs on the strength of a conversation alone.
