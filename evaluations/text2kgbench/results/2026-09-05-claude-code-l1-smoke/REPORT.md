# L1 smoke panel via Claude Code — 2026-09-05

145-case committed smoke panel, run through the `claude_code` provider (headless
`claude -p`) on the Claude subscription rather than an Anthropic API key.
Bead: aegis-tangtq. Directive: Stiwi 2026-09-05, *"for Text2KGBench can we just use
claude code?"*

Re-score this result with no model call:

```bash
scripts/evaluate-text2kgbench.py score \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json \
  --responses evaluations/text2kgbench/responses/claude-code-v1 \
  --lever L1 --output /tmp/rescore
```

Verified: re-scoring from the committed `responses/claude-code-v1/` reproduces
`tp/fp/fn = 140/232/247`, strict micro F1 `0.368906`, identical to the live run.

## Configuration

| | |
|---|---|
| provider | `claude_code` (`claude -p --output-format json`) |
| CLI | `claude-code==2.1.261` |
| model reported by the CLI | `claude-haiku-4-5-20251001` (single value across all 145 cases) |
| decoding | temperature/top_p/max_output_tokens **null** (the CLI exposes no such control); `thinking_budget_tokens: 0` via `MAX_THINKING_TOKENS` |
| isolation | `--tools "" --system-prompt "" --setting-sources "" --strict-mcp-config --disable-slash-commands --no-session-persistence`, parent `CLAUDE_CODE_*` / `ANTHROPIC_*` scrubbed |
| containment | `systemd-run --user --scope -p MemoryMax=4G -p CPUQuota=200%`, serial (concurrency 1) |
| window | 2026-09-05T02:49:53Z → 02:56:49Z |

## Extraction quality (145 cases, model predictions — not a replay)

| metric | this panel | frozen L0 Vicuna replay floor |
|---|---|---|
| strict micro F1 | **0.368906** (p 0.376344 / r 0.361757) | 0.172973 |
| relation-filtered micro F1 | **0.412979** (p 0.481100 / r 0.361757) | 0.351648 |

The floors are the v2 25-case film config's regression floors and are quoted for
orientation only: different corpus, different case count. **This is not a parity
claim** — `text2kgbench-parity-requires-scorer-equivalence` applies.

Stage decomposition: generated 402, accepted 372, `syntax_repaired` 125,
`malformed_output` 20, `object_not_grounded` 20, `evidence_not_grounded` 5,
`subject_not_grounded` 3, `relation_outside_ontology` 2. Refusals 0.

### The 20 malformed outputs are a parse POLICY cost, not truncation

Every one of the 20 finished with `stop_reason: end_turn` and there is no length
ceiling (malformed responses run 463–1293 chars; well-formed ones reach 1560).
19 of 20 are a valid fenced JSON block **followed by an explanatory paragraph**;
`parse_response` requires the whole response to be exactly a fence, which is
deliberate and asserted by an existing test.

Measured headroom, computed offline from the same bytes with a balanced-brace
extractor (**this is a lever estimate, not a harness result — nothing in the
harness changed**): strict F1 0.368906 → 0.380697, filtered 0.412979 → 0.430976.
So the strict parse policy costs about **0.012 strict F1**. Worth a prompt or
`--json-schema` lever later; not a blocker, and not a reason to loosen the parser.

## Cost

Per case, billed (`modelUsage` session totals, which include the CLI's own
per-invocation bookkeeping call — 451,930 billed input vs 176,542 in the scored
requests alone, so a per-request figure understates the run by ~2.6x on input):

| | 145 cases | projected 6,076 cases (x41.9) |
|---|---|---|
| billed input tokens | 451,930 | 18,937,425 |
| billed output tokens | 33,182 | 1,390,440 |
| thinking tokens | 0 | 0 |
| list-price equivalent | $0.6178 | **$25.89** |
| serial wall time | 415.3 s (p50 2.75 s, p95 4.07 s, max 4.46 s) | **4.83 h** |

The dollar column is the CLI's own `costUSD` at list basis. **No dollars were
spent** — this ran on the subscription. It is included as a size check, not a bill.

### Governor points — measured, and honestly below resolution

The endpoint behind `st_governor_used_percent`
(`api.anthropic.com/api/oauth/usage`) was sampled every 60 s from before the run
to after it; samples are in `governor-usage-samples.jsonl`.

| window | before the run (fleet-only control) | during the run |
|---|---|---|
| `seven_day` (base weekly, `weekly_all`) | 13% → 13% over 7.02 min | **13% → 13% over 6.02 min** |
| `five_hour` | 25% → 27% over 7.02 min (0.285 pt/min) | 28% → 30% over 6.02 min (0.332 pt/min) |

**The base window did not move at all.** Both windows quantize to whole
percentage points, and ~8 other crew agents were burning the same account
throughout, so:

* **MEASURED:** the 145-case panel cost **< 1 base-window point**. Scaling by
  41.9 gives a hard upper bound of **< 42 base-window points** for all 6,076
  cases. That bound is true and it is loose.
* **NOT a measurement:** the five-hour excess over the control rate is
  ~0.047 pt/min x 6.02 min ~ 0.3 points. That is below the instrument's 1-point
  resolution and must not be quoted as this run's cost. The `during` and
  `before` segments both moved exactly 2 points; the fleet alone explains them.

**To resolve the projection, run a contiguous ~1,000-case block and read the
base-window delta.** At ~7x this panel it is the smallest run that can move a
1-point instrument, costs ~$4 list-equivalent, and turns the bound into a number.
A 145-case panel cannot do it, and no amount of care with these samples will make
it able to.

## Honesty requirements for any published scoreboard

The extractor was **Claude (`claude-haiku-4-5-20251001`) via Claude Code
`2.1.261`, on 2026-09-05**, with the thinking budget pinned to 0 and no
temperature/top_p/max-token pins available. Reproducing it needs a Claude
subscription and that CLI build. `anthropic_messages` remains the
API-reproducible path for outsiders. Nothing in the manifest, scoring, corpora or
the 29-ontology / 6,076-case universe changed.
