# Emitting to quipu

Caboodle is itself a quipu emitter, twice over:

**The box emits.** Every applied and functionally verified tool transition is
written as a redacted episode under `.caboodle/episodes/`. The filename is the
SHA-256 identity of the event content, so rerunning the same transition creates
no duplicate. A failed delivery leaves the exact bytes pending; a later replay
can safely receive Quipu's `unchanged` outcome and then move the file to
`sent/`.

**Beads emits.** Bead lifecycle — created, closed, commented — flows to quipu
as episodes through the same discipline. `caboodle queue-br export.jsonl`
projects each issue row into created, comment, and (when closed) closure events;
re-reading a snapshot is idempotent. Because `br` is upstream, this integration
lives in CABOODLE and requires no fork patch.

Flush only after reviewing the queue. The endpoint must be HTTPS (or explicit
localhost HTTP), and the bearer comes only from `QUIPU_AUTH_TOKEN`; it is never
written into the plan, queue, or command line. CABOODLE first runs a SPARQL
control whose marker must return, then posts files byte-for-byte:

```bash
caboodle queue-br br-export.jsonl
QUIPU_AUTH_TOKEN=... caboodle flush-episodes --endpoint https://graph.example
```

Created, updated, and unchanged are successful delivery outcomes. A transport
error, malformed response, or missing outcome preserves the identical pending
file instead of guessing whether the write committed.
