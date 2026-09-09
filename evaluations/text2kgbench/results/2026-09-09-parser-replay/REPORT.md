# Offline parser replay: aggregate gain, regression gate FAIL

The opt-in `fenced-json-v2` parser recovers model JSON followed by commentary. It improves both aggregate F1 scores on the **same frozen 1,000 L1 responses**, but regresses three ontologies. **Do not promote it as an unqualified improvement.** The default `strict-v1` parser and published headline remain unchanged.

Original inference: Claude Haiku 4.5 (`claude-haiku-4-5-20251001`), Claude Code provider, 2026-09-05. Selected population: **1,000/6,076 cases, all 29 ontologies, both corpora**. New model calls: **zero**. Original pipeline: `caboodle-text2kg-v3-ontology-guided`; candidate: `caboodle-text2kg-v3-ontology-guided+fenced-json-v2`. This is response-parser recovery on a development sample, not a new model run or held-out generalisation result.

| Metric | Published/default replay | Candidate replay |
|---|---:|---:|
| strict tp | 742 | 787 |
| relation_filtered tp | 742 | 787 |
| strict fp | 1646 | 1749 |
| relation_filtered fp | 898 | 950 |
| strict fn | 1468 | 1423 |
| relation_filtered fn | 1468 | 1423 |
| strict precision | 0.310720268 | 0.310331230 |
| relation_filtered precision | 0.452439024 | 0.453080023 |
| strict recall | 0.335746606 | 0.356108597 |
| relation_filtered recall | 0.335746606 | 0.356108597 |
| strict f1 | 0.322749021 | 0.331647703 |
| relation_filtered f1 | 0.385454545 | 0.398783886 |

Both corpus aggregates improve, but the ontology regressions are material:

| Corpus / ontology | Strict F1 before → after | Filtered F1 before → after |
|---|---:|---:|
| dbpedia_webnlg / 16_city | 0.109090909 → 0.106194690 | 0.123711340 → 0.121212121 |
| wikidata_tekgen / 4_book | 0.307692308 → 0.305949008 | 0.421875000 → 0.420233463 |
| wikidata_tekgen / 5_military | 0.313253012 → 0.304347826 | 0.412698413 → 0.424242424 |

The candidate adds **45 TP and 103 strict FP** (52 filtered FP). Strict precision declines slightly even as recall and F1 improve. The nonzero regression gate covers strict and filtered F1 separately at aggregate, corpus and ontology levels; a favourable aggregate cannot hide a regressed ontology. All 29 ontology pairs, changed case IDs and source hashes are in [comparison.json](comparison.json).

## What changed

The old parser accepts JSON or a response consisting entirely of one JSON fence. In this sample, 135 of the 141 malformed responses contain one valid JSON fence plus explanatory prose. The candidate accepts exactly one explicit JSON fence, refuses multiple/unterminated/wrong-language fences and unsupported top-level shapes, and never searches arbitrary prose for braces. Fenced trailing-comma repair tracks JSON strings, preserving literal comma/brace values that a regex can change.

The parser receives only raw response text. It receives no case ID, ontology, sentence or gold. Ontology membership, sentence evidence, grounding, normalisation, selected IDs and metric arithmetic are unchanged. Tests exercise ambiguity, invalid shapes, escaped/string byte preservation, grounding rejection, response hash tampering, missing selected responses, baseline drift, and an improving aggregate hiding a regressed ontology.

## Remaining work

Malformed outputs: **141 → 6**. Syntax-wrapper handling is counted separately from malformed recovery; 994 syntax-repaired rows does not mean 994 failed responses were recovered. Accepted triples: **2,388 → 2,536**; generated candidates: **2,712 → 2,902**. There are still **1,423 selected-sample false negatives**. These are unresolved errors, not declared non-goals.

The six unresolved malformed case IDs are:

- `ont_10_culture_test_28`
- `ont_3_sport_test_13`
- `ont_3_sport_test_406`
- `ont_4_building_test_87`
- `ont_6_politician_test_13`
- `ont_9_astronaut_test_12`

The other **5,076 unrun cases** remain outside this experiment. No full-manifest pass or SOTA-parity claim follows. Gold-based ontology allowlists or case-specific parser exceptions would overfit this replay and are not used. New model inference needs an approved run and an independently evaluated selection.

## Reproduce without inference

Use the pinned Text2KGBench checkout at `50a3d255371b8817cdff70fd88459ac82b339cfe`:

```sh
python3 scripts/compare-text2kg-parsers.py \
  --dataset-root /tmp/Text2KGBench \
  --manifest evaluations/text2kgbench/full-manifest.json \
  --sample evaluations/text2kgbench/results/2026-09-05-claude-code-stratified-1000 \
  --output /tmp/parser-comparison.json
```

Expected exit **3**, with the report written and `regression_gate: FAIL`. Exit 2 means the source/selection/hash/baseline precondition failed; exit 0 requires no F1 regression in any measured class/group. The manifest, dataset commit, selected IDs, raw response hashes and exact legacy per-case replay are checked before comparison. No provider module is loaded.

Individual scoring remains available through `evaluate-text2kgbench.py score --parser fenced-json-v2` (or the default `strict-v1`). Its rows cover the full manifest; retain only selected IDs for this experiment. The comparison command performs that restriction and refuses missing or extra responses.

Validation: **74 tests passed**. Two complete offline comparison runs returned exit3 and byte-identical JSON. Comparison SHA256: `cb756067af75acb85d689b206c3bb6c30a99ffbe7fb653efbac30bc7c27c47ff`. Parser and comparison implementation hashes are embedded in the artifact.
