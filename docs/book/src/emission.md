# Emitting to quipu

Caboodle is itself a quipu emitter, twice over:

**The box emits.** Engine state transitions — installed, verified, upgraded,
failed — become episodes through camayoc's ingress discipline, so the graph the
box just installed immediately knows what the box contains. Emissions queue
locally until quipu passes its own verify, then flush once.

**Beads emits.** Bead lifecycle — created, closed, commented — flows to quipu
as episodes through the same discipline, so the tracker's history becomes graph
knowledge as it happens instead of by after-the-fact extraction. Because `br`
is upstream, the emitter ships in the integration layer (a post-write hook or
JSONL-watcher installed by caboodle), not as a fork patch.
