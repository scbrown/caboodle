# caboodle — the whole kit
# Stubs refuse loudly until Phase 0 lands; nothing here pretends.

default:
    @just --list

# Install the stack per a plan file (Phase 0 — not implemented)
install:
    @echo "caboodle install: not implemented yet (Phase 0). See docs/design/vision.md" >&2
    @exit 1

# Verify every installed tool: version read-back, functional round-trip, negative control
verify:
    @echo "caboodle verify: not implemented yet (Phase 0). See docs/design/vision.md" >&2
    @exit 1
