# Crew capability contracts

CABOODLE installs the selected crew runtime, but it does not take ownership of
the runtime's security or admission policy. Shantytown is a host executable, so
its adapter reads back the installed `st` version. Creel's decisive facts exist
inside the browser; verification therefore consumes two browser-produced JSON
documents:

```console
caboodle verify \
  --creel-doctor creel-doctor.json \
  --creel-admission creel-admission.json
```

Both schemas are version 1 and reject unknown fields. A new producer field
cannot silently become evidence an older CABOODLE claims to understand.

## Doctor

```json
{
  "schema_version": 1,
  "overall": "pass",
  "checks": [{
    "id": "secure-context",
    "status": "pass",
    "severity": "required",
    "evidence": "browser reported a secure context",
    "remediation": "serve Creel over HTTPS",
    "redacted": true
  }]
}
```

Check IDs must be non-empty and unique. Status is `pass`, `fail`, or
`unknown`; severity is `required` or `advisory`. Every check carries non-empty
redacted evidence and remediation. Every required check and the aggregate must
be `pass`.

The Creel doctor workstream owns which checks it emits, including secure
context, persistence, popup/bridge permissions, provider credential presence,
Quipu WASM, state-repository health, service-worker freshness, and leases.
CABOODLE validates the contract; it does not reproduce those probes.

## Admission

```json
{
  "schema_version": 1,
  "verdict": "admit",
  "provider_window": {"status":"pass", "evidence":"below ceiling"},
  "device_tab_cap": {"status":"pass", "evidence":"one slot available"},
  "signal_freshness": {"status":"pass", "evidence":"observed now"},
  "reason": "launch is within the redacted policy limits",
  "redacted": true
}
```

The verdict is `admit`, `refuse`, or `unknown`. CABOODLE proceeds only when all
three signals are measured passes and the verdict is `admit`. It never prints
credentials, accepts an unredacted reason, rounds unknown to zero, or converts
a policy refusal into an installer warning.

Until Creel publishes both producer interfaces, its bundle can be applied but
cannot be marked verified. That refusal is the intended boundary, not a
CABOODLE installation failure to bypass.
