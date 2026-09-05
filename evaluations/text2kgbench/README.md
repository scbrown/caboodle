# Text2KGBench evaluation

This harness separates extraction quality from graph-store conformance. It replays a hash-pinned
published Vicuna-13B response artifact from Text2KGBench, scores triples deterministically, then
passes the predictions through Camayoc's governed RML executor into a disposable Quipu server.
No live graph is modified and no model call is made.

`graph-extract` is an agent extraction procedure rather than a callable model endpoint. This
harness therefore measures its two independently testable boundaries: a frozen, hash-pinned
text-to-triples artifact for extraction quality, and the production Camayoc governed-write path
for store conformance. It does not claim that the upstream Vicuna artifact was produced by the
`graph-extract` skill.

The Quipu binary must provide `/graph/create`, `/graph/label`, and `/graphs` (Quipu 0.3.27 is the
recorded evaluation baseline). The harness provisions Camayoc's low-trust inferred plane before
writing; it never falls back to ROOT.

Create an environment containing the pinned dependency, clone the pinned dataset commit, and run:

```bash
python -m venv /tmp/text2kg-venv
/tmp/text2kg-venv/bin/pip install -r evaluations/text2kgbench/requirements.txt
git clone https://github.com/cenguix/Text2KGBench.git /tmp/Text2KGBench
git -C /tmp/Text2KGBench checkout 50a3d255371b8817cdff70fd88459ac82b339cfe
scripts/evaluate-text2kgbench.py \
  --dataset-root /tmp/Text2KGBench \
  --camayoc-root /path/to/camayoc \
  --camayoc-python /tmp/text2kg-venv/bin/python \
  --quipu-server /path/to/quipu-server-0.3.27 \
  --output /tmp/text2kg-result
```

`config.json` freezes the dataset, selection, model-output artifact, prompt, vocabulary, and scorer.
The report contains strict extraction metrics, the benchmark's relation-filtered metrics, a
per-stage error decomposition, RML materialization counts, Quipu's write verdict, and graph
read-back metadata. `temperature` and `seed` are explicitly null because upstream did not publish
them; claiming otherwise would manufacture provenance. Replay of the response bytes is the frozen
inference configuration.

For fast extraction-quality iteration, pass `--extraction-only`. This still verifies every dataset
hash, writes the reconciled predictions and complete extraction report, and enforces the configured
strict/filtered/recall regression floors; it records `ingress: null` rather than making a store
conformance claim. The default remains the full fail-closed Camayoc/Quipu run.

## Full ontology-guided evaluation (v3)

The v3 path is separate from the film-specific v2 reconciler. It covers all 29 ontologies in the
two pinned test corpora (6,076 cases / 13,491 gold triples), consumes ontology JSON as data, and
has no ontology-name branches. Validate the checked-in byte manifest first:

```bash
scripts/evaluate-text2kgbench.py inventory \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json
```

Replay and score the frozen upstream L0 responses without a model call:

```bash
scripts/evaluate-text2kgbench.py score \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json \
  --responses /tmp/unused --lever L0 --output /tmp/text2kg-l0
```

This writes all 6,076 case rows, 29 ontology rows, two corpus micro rows, Wikidata selected/unseen
strata, stage errors, and a report containing every artifact hash. Missing responses remain false
negatives in the denominator. Re-running against the same bytes is deterministic.

L1 compiles each ontology into a canonical JSON prompt and calls one of the provider adapters
registered in `providers.json`. **The provider is configuration**: `run --provider <id>` selects a
registry row, and no path in `scripts/` names a provider module. The default is the manifest's
`inference.provider`, so an unqualified `run` behaves exactly as before.

| provider | runtime | billing | reproducible by |
|---|---|---|---|
| `anthropic_messages` (default) | `anthropic` SDK, pinned | Anthropic API key | anyone with an API key |
| `claude_code` | headless `claude -p --output-format json` | Claude subscription (governor points) | anyone with a Claude subscription and the pinned CLI build |

Install the Python 3.12 lock and run the committed 145-case panel before any full run:

```bash
uv pip sync evaluations/text2kgbench/requirements-inference.txt

# API-key path (set ANTHROPIC_API_KEY)
scripts/evaluate-text2kgbench.py run \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json \
  --responses evaluations/text2kgbench/responses/v3 \
  --output /tmp/text2kg-v3-smoke --smoke

# subscription path (no API key; must NOT have one set)
scripts/evaluate-text2kgbench.py run ... --provider claude_code
```

Each adapter refuses, before reading a case, anything that would move the run off its declared
billing path: `anthropic_messages` refuses an absent key, SDK drift, custom endpoints and proxies;
`claude_code` refuses a **present** `ANTHROPIC_API_KEY` (which would silently spend money instead of
subscription budget), CLI version drift, custom endpoints and proxies. Two guards in the driver
refuse a registry row that disagrees with the manifest's pinned inference block, and a provider
module that disagrees with its own registry row — either would silently change every `request_hash`.

Full execution is an operator cost decision and requires `--confirm-cost <amount>`; neither CI nor
offline scoring calls a provider. Response rows bind the request, prompt, exact model, decoding
settings, provider request ID, usage, latency, finish reason, provider id and response digest.

Two honest divergences on the `claude_code` path, recorded in `providers.json` rather than
smoothed over:

* The CLI exposes no sampling controls, so its decoding block is `temperature: null`,
  `top_p: null`, `max_output_tokens: null`. Copying the Anthropic path's `temperature: 0` would
  claim a setting the run never made. Because `request_hash` binds decoding, the two providers'
  response ledgers cannot be confused for one another.
* One `claude -p` invocation bills more than the scored request. `input_tokens` / `output_tokens`
  therefore carry the session totals from `modelUsage` — the figure actually billed — while
  `api_input_tokens` / `api_output_tokens` carry the scored request alone.

A published scoreboard from the `claude_code` path must say the extractor was Claude via Claude
Code, on the recorded date and model string, and that reproducing it needs a Claude subscription.
`anthropic_messages` remains the API-reproducible path for outsiders.

`reconcilers.json` declares the old film plugin as domain-specific. It is excluded from v3 and
cannot contribute to either general corpus aggregate.
