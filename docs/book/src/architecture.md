# Architecture

Caboodle's engine is a **plan → apply → verify** loop with a state file
recording what is installed *and last proven working*. Idempotent and
resumable: re-running converges, and a failed step names itself.

The wizard is an interview, not a UI: `caboodle init --guided` runs the
question flow; in agent hands the same flow is a skill. Output is always the
plan file — two-phase, so nothing installs on the strength of a conversation
alone.
