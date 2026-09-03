#!/usr/bin/env python3
"""Generate the reviewed full Text2KGBench manifest from its pinned checkout."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from text2kg_general import (PROMPT_TEMPLATE, SMOKE_SEED, canonical_json, read_jsonl,
                             sha256_bytes, sha256_file, smoke_ids)


COMMIT = "50a3d255371b8817cdff70fd88459ac82b339cfe"


def record(root: Path, path: Path) -> dict:
    return {"path": str(path.relative_to(root)), "sha256": sha256_file(path)}


def response_path(base: Path, corpus: str, ontology: str) -> Path:
    if corpus == "wikidata_tekgen":
        return base / "baselines/Vicuna-13B/llm_responses" / f"ont_{ontology}_llm_responses.jsonl"
    return base / "baselines/Vicuna-13B/llm_responses" / f"{ontology}_Vicuna13B_responses.jsonl"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    entries, panel_ids = [], []
    for corpus in ("wikidata_tekgen", "dbpedia_webnlg"):
        base = args.dataset_root / "data" / corpus
        for gold in sorted((base / "ground_truth").glob("ont_*_ground_truth.jsonl")):
            ontology = gold.name.removeprefix("ont_").removesuffix("_ground_truth.jsonl")
            test = base / "test" / f"ont_{ontology}_test.jsonl"
            ontology_file = base / "ontologies" / f"{ontology}_ontology.json"
            paths = {"gold": record(args.dataset_root, gold), "test": record(args.dataset_root, test),
                     "ontology": record(args.dataset_root, ontology_file),
                     "upstream_response": record(args.dataset_root, response_path(base, corpus, ontology))}
            train = base / "train" / f"ont_{ontology}_train.jsonl"
            if train.exists():
                paths["train"] = record(args.dataset_root, train)
            validation = base / "validation" / f"ont_{ontology}_validation.jsonl"
            if validation.exists():
                paths["validation"] = record(args.dataset_root, validation)
            selected = base / "manually_verified_sentences" / f"selected_ont_{ontology}.txt"
            if selected.exists():
                paths["selected_ids"] = record(args.dataset_root, selected)
            unseen_test = base / "unseen_sentences/test" / f"ont_{ontology}_unseen_test.jsonl"
            unseen_gold = base / "unseen_sentences/ground_truth" / f"ont_{ontology}_unseen_ground_truth.jsonl"
            if unseen_test.exists() and unseen_gold.exists():
                paths["unseen_test"] = record(args.dataset_root, unseen_test)
                paths["unseen_gold"] = record(args.dataset_root, unseen_gold)
                unseen_response = (base / "baselines/Vicuna-13B/unseen/llm_responses" /
                                   f"{ontology}_unseen_Vicuna13B_responses.jsonl")
                paths["unseen_response"] = record(args.dataset_root, unseen_response)
            rows = read_jsonl(gold)
            chosen = smoke_ids((row["id"] for row in rows))
            panel_ids.extend(chosen)
            entries.append({"id": ontology, "corpus": corpus, "case_count": len(rows),
                            "gold_triple_count": sum(len(row["triples"]) for row in rows),
                            "files": paths, "smoke_ids": chosen})
    panel_ids = sorted(panel_ids)
    manifest = {
        "schema_version": 1,
        "dataset": {"repository": "https://github.com/cenguix/Text2KGBench.git", "commit": COMMIT},
        "pipeline": "caboodle-text2kg-v3-ontology-guided",
        "totals": {"ontologies": len(entries), "cases": sum(x["case_count"] for x in entries),
                   "gold_triples": sum(x["gold_triple_count"] for x in entries)},
        "inference": {"provider": "anthropic_messages", "sdk": "anthropic==0.120.2",
                      "python": "3.12", "model": "claude-haiku-4-5-20251001",
                      "prompt_template": PROMPT_TEMPLATE,
                      "prompt_template_sha256": sha256_bytes(PROMPT_TEMPLATE.encode()),
                      "decoding": {"temperature": 0, "top_p": 1, "samples": 1,
                                   "max_output_tokens": 512, "timeout_seconds": 60}},
        "scorer": {"version": "3", "normalization": "lowercase_remove_whitespace_and_underscores",
                   "source_sha256": sha256_file(Path(__file__).with_name("text2kg_general.py"))},
        "reconciler_registry_sha256": sha256_file(Path(__file__).parents[1] / "evaluations/text2kgbench/reconcilers.json"),
        "enabled_levers": ["L0", "L1"],
        "smoke_panel": {"seed": SMOKE_SEED, "count": len(panel_ids), "ids": panel_ids,
                        "sha256": hashlib.sha256(canonical_json(panel_ids).encode()).hexdigest()},
        "ontologies": entries,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
