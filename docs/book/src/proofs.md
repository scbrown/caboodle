# Three proofs, not one

Three claims separate caboodle from an install script, and each is a proof:

1. **Installed** — proven by version read-back, never by exit code.
2. **Working** — a per-tool functional round-trip: quipu accepts an episode and
   a control-gated query finds it; bobbin indexes a fixture and search returns
   it. Every check pairs with a **negative control** — the selftest that
   corrupts a byte and goes red — so a pass is capable of failing.
3. **Observable** — Prometheus actually scrapes each tool's declared exporter;
   caboodle generates the scrape config, a starter alert pack, and one
   consolidated dashboard.

This encodes the operating culture the stack was built under: verify by
mechanism, read back what you wrote, control every zero.
