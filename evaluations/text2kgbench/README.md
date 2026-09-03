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

L1 compiles each ontology into a canonical JSON prompt and uses the sole pinned provider adapter
in `providers/anthropic_messages.py`. Install the Python 3.12 lock, set `ANTHROPIC_API_KEY`, and
run the committed 145-case panel before any full run:

```bash
uv pip sync evaluations/text2kgbench/requirements-inference.txt
scripts/evaluate-text2kgbench.py run \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json \
  --responses evaluations/text2kgbench/responses/v3 \
  --output /tmp/text2kg-v3-smoke --smoke
```

The adapter refuses absent credentials, SDK drift, custom endpoints and proxies before reading a
case. Full execution is an operator cost decision and requires `--confirm-cost <amount>`; neither
CI nor offline scoring calls the provider. Response rows bind the request, prompt, exact model,
decoding settings, provider request ID, usage, latency, finish reason and response digest.

`reconcilers.json` declares the old film plugin as domain-specific. It is excluded from v3 and
cannot contribute to either general corpus aggregate.
