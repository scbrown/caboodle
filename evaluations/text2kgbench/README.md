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
