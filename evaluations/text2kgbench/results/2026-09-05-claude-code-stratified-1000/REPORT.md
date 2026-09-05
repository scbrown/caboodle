# Stratified Text2KGBench L1 measurement — 2026-09-05

Complete: 1,000 selected cases generated and scored; direct pre/post usage observations and exporter history preserved.

## Measured result

The serial run completed **1,000/1,000** selected requests and responses, covering **29/29 ontologies and both corpora**, from **17:16:11Z to 18:09:53Z** (53m42s), exit 0. Selected IDs, per-ontology counts, response hashes, request hashes, model name and pinned manifest hash all verified. A second offline scoring pass reproduced five full-universe scorer artifacts byte-for-byte.

| Selected-sample metric | Strict | Relation-filtered |
|---|---:|---:|
| True positives | 742 | 742 |
| False positives | 1,646 | 898 |
| False negatives | 1,468 | 1,468 |
| Precision | 0.310720 | 0.452439 |
| Recall | 0.335747 | 0.335747 |
| Micro F1 | **0.322749** | **0.385455** |

| Recorded successful-invocation usage | Value |
|---|---:|
| Input tokens, including cache/bookkeeping | 2,776,374 |
| Output tokens | 222,831 |
| Recorded thinking tokens | 0 |
| List-price equivalent | **$3.890529** |
| Latency p50 / p95 | 2.798s / 4.449s |
| Maximum latency | 127.295s |
| Empty-output flags | 0 |

The provider's `refusal` flag means empty raw output, not a semantic review of refusals. Three cases exceeded 10 seconds (13.35s, 124.31s and 127.30s); exact retry counts and usage lost with timed-out attempts are unavailable. The stage ledger records 141 malformed outputs and 2,388 accepted triples. No parser or extractor change was made during this run.

Scaling each ontology's recorded usage by its population/sample ratio gives a **$23.580882 list-equivalent estimate** for 6,076 cases under the observed usage mix, with about 16.81M input and 1.35M output tokens. This is a point estimate from one deterministic stratified sample, with no uncertainty interval, excluding unobserved retry usage. It is not a subscription-point estimate, a guaranteed cost, or an executed full-corpus run.

## Scope and provenance

Exactly 1,000 cases selected proportionally by ontology size, with a floor of one per ontology and largest-remainder allocation. The manifest's existing seeded hash ranking determines selection within each ontology. Selection covers all 29 ontologies and both corpora. `selection.json` records every selected ID and each stratum's population and sample size; `provenance.json` records code, dataset, manifest and selection hashes.

Code: Caboodle `9c242482ec35d827d4237efd9741f453dbd3762a`, including merged PR #4. Dataset: Text2KGBench `50a3d255371b8817cdff70fd88459ac82b339cfe`. Provider: Claude Code 2.1.261, model `claude-haiku-4-5-20251001`, thinking budget zero; temperature, top-p and maximum output length are not configurable through this provider and remain unpinned. Requests use the same ontology compiler, response parser and scorer as the earlier runs.

The run uses a fresh response directory, serial execution, a 4-GiB memory ceiling and two-CPU quota. Other crew activity and local Bobbin check/test commands ran concurrently on the host; observed latency is not isolated performance evidence. This run authorizes only the selected 1,000 cases. No full-corpus inference was run.

## Governor interpretation

Samples are taken every 60 seconds from the shared subscription usage endpoint. Pre-control began at 17:11:51Z with five-hour usage 15 and weekly usage 47; the last pre-run sample at 17:15:52Z was 15/48. Weekly usage therefore moved before any benchmark inference began. The run began at 17:16:11Z.

| Direct observations | Five-hour window | Weekly window |
|---|---:|---:|
| Pre-control, 17:11:51–17:15:52Z | 15 → 15 | 47 → 48 |
| Last pre-run → first post-run, 18:10:56Z | 15 → 26 | 48 → 50 |
| Post-control, 18:10:56–18:14:57Z | 26 → 27 | 50 → 50 |

The observed last-pre to first-post weekly increase is **2 displayed points**, including fleet traffic. Direct samples were unavailable between 17:28:56Z and 18:10:56Z because the endpoint returned HTTP429. Independently stored exporter history supplies coarser observations across that gap, with source timestamps and probe success/status/cache-age fields. The exporter later received HTTP429 too; stale values are explicitly marked and not used to claim a flat control. The temporary samplers were stopped.

The preregistered post-control target was at least five minutes. Collection continued beyond that time, but the successful direct control readings span **4m01s**; the next request was rate-limited. The pre-control also spans 4m01s. This is a collection limitation, not a reason to infer values in the missing interval. `governor-summary.json`, `governor-usage-samples.jsonl`, `exporter-history.json`, and `sampler-errors.md` preserve the distinction.

Displayed percentage-point changes include concurrent fleet traffic and endpoint quantization. They are reported separately from provider token totals and cannot establish an exact extraction-only governor cost. A displayed delta of d is not a strict upper bound of d on true consumption: under a stable monotone window and a width-one quantizer, true aggregate movement can approach d+1. Flat pre/post controls do not constrain bursts during the intervening run. These qualifications correct the earlier prefix report's strict two-point-bound interpretation.

## Comparison limits

The earlier 1,000-case prefix covered only 10 ontologies and one corpus. The 145-case smoke panel used equal per-ontology counts. Neither raw mean has the same population weighting as this proportional sample. Scores across these different selections do not measure a change in extractor quality. Any full-corpus token/list-price estimate from this run must use population/sample weights per ontology and remains an estimate from one deterministic sample, not an executed full run. Subscription usage percentages cannot be projected from dollar equivalents.

## Token-ledger scope

The provider records each successful Claude Code invocation's `modelUsage` totals, including that invocation's bookkeeping calls. These are recorded session totals, not a guarantee of complete account billing: `create_message` can time out and retry, but failed/timed-out attempts do not return token usage into the ledger. A response latency near 124 seconds is consistent with the 120-second timeout plus retry and requires this caveat. Attempt counts and unreturned usage are not observed in the current adapter; no exact retry count or exact total subscription spend can be inferred from latency alone.

## Scoring denominator

The unchanged scorer emits rows for all 6,076 manifest cases, marking cases without responses as `missing_output`. Final sample metrics are therefore aggregated only over the 1,000 IDs in `selection.json`, with an assertion that every selected case has a response. The other 5,076 unrun cases are outside this experiment and are not counted as extraction failures. Full-universe scorer artifacts and selected-sample summaries are kept distinct.

## Reproduction

From this repository, score the saved responses without model calls (substitute a fresh output directory):

```sh
python3 scripts/evaluate-text2kgbench.py score \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json \
  --responses evaluations/text2kgbench/results/2026-09-05-claude-code-stratified-1000/responses \
  --lever L1 --output /tmp/stratified-rescore
```

Read `selection.json`, retain only those IDs from the resulting `cases.json`, and sum `tp`, `fp`, and `fn` separately for `strict` and `relation_filtered`. Precision is `tp/(tp+fp)`, recall is `tp/(tp+fn)`, and micro F1 is `2*tp/(2*tp+fp+fn)`. `score/summary.json` records the counts, and `score/cases.json` contains exactly those 1,000 rows. `score/ontology-summary.json` records each population/sample denominator and its token totals, allowing the weighted estimate to be reproduced.
