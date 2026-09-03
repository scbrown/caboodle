"""Ontology-guided, full-suite Text2KGBench evaluation primitives."""

from __future__ import annotations

import hashlib
import json
import re
from collections import defaultdict
from pathlib import Path
from typing import Iterable


PIPELINE = "caboodle-text2kg-v3-ontology-guided"
SMOKE_SEED = "aegis-tydvlg.4-l1-smoke-v1"
PROMPT_TEMPLATE = "ontology-guided-json-evidence-v1"


def canonical_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def normalize(value: object) -> str:
    return re.sub(r"[_\s]+", "", str(value)).lower()


def normalized_triple(triple: Iterable[object]) -> tuple[str, str, str]:
    return tuple(normalize(value) for value in triple)  # type: ignore[return-value]


def metric(tp: int, fp: int, fn: int) -> dict:
    precision = tp / (tp + fp) if tp + fp else 0.0
    recall = tp / (tp + fn) if tp + fn else 0.0
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
    return {"tp": tp, "fp": fp, "fn": fn, "precision": precision, "recall": recall, "f1": f1}


def compile_ontology(ontology: dict, sentence: str) -> dict:
    """Compile ontology data into one canonical, domain-independent request."""
    concepts = sorted(
        ({"id": item["qid"], "label": item["label"]} for item in ontology["concepts"]),
        key=lambda item: (item["label"], item["id"]),
    )
    grouped: dict[str, set[tuple[str, str]]] = defaultdict(set)
    for item in ontology["relations"]:
        grouped[item["label"]].add((item["domain"], item["range"]))
    relations = [{"relation": label,
                  "signatures": [{"domain": domain, "range": range_}
                                 for domain, range_ in sorted(signatures)]}
                 for label, signatures in sorted(grouped.items())]
    schema = {
        "type": "object",
        "required": ["triples"],
        "properties": {"triples": {"type": "array", "items": {
            "type": "object", "required": ["subject", "relation", "object", "evidence_span"],
            "properties": {
                "subject": {"type": "string"},
                "relation": {"type": "string", "enum": [item["relation"] for item in relations]},
                "object": {"type": "string"},
                "evidence_span": {"type": "string"},
            },
        }}},
    }
    return {
        "instruction": "Extract only facts stated or unambiguously entailed by the isolated sentence.",
        "concepts": concepts,
        "relations": relations,
        "output_schema": schema,
        "sentence": sentence,
    }


def request_hash(request: dict, model: str, decoding: dict) -> str:
    return sha256_bytes(canonical_json({"request": request, "model": model, "decoding": decoding}).encode())


def smoke_ids(case_ids: Iterable[str], count: int = 5) -> list[str]:
    ranked = sorted(case_ids, key=lambda case_id: (
        hashlib.sha256(f"{SMOKE_SEED}\0{case_id}".encode()).hexdigest(), case_id))
    return ranked[:count]


def validate_candidate(candidate: object, sentence: str, ontology: dict) -> tuple[str, dict | None]:
    if not isinstance(candidate, dict) or any(k not in candidate for k in
                                               ("subject", "relation", "object", "evidence_span")):
        return "invalid_triple_shape", None
    values = {key: str(candidate[key]) for key in ("subject", "relation", "object", "evidence_span")}
    relation = next((item for item in ontology["relations"] if item["label"] == values["relation"]), None)
    if relation is None:
        return "relation_outside_ontology", None
    if values["evidence_span"] not in sentence:
        return "evidence_not_grounded", None
    if normalize(values["subject"]) not in normalize(sentence):
        return "subject_not_grounded", None
    if normalize(values["object"]) not in normalize(sentence):
        return "object_not_grounded", None
    return "accepted", values


def parse_response(raw: str) -> tuple[dict | None, bool]:
    try:
        return json.loads(raw), False
    except json.JSONDecodeError:
        match = re.fullmatch(r"\s*```(?:json)?\s*(\{.*\})\s*```\s*", raw, re.DOTALL)
        if not match:
            return None, False
        repaired = re.sub(r",\s*([}\]])", r"\1", match.group(1))
        try:
            return json.loads(repaired), True
        except json.JSONDecodeError:
            return None, False


def validate_manifest(dataset_root: Path, manifest: dict) -> dict:
    errors: list[str] = []
    entries = manifest.get("ontologies", [])
    if len(entries) != 29:
        errors.append(f"expected 29 ontologies, found {len(entries)}")
    seen_ids: set[str] = set()
    totals: dict[str, dict[str, int]] = defaultdict(lambda: {"ontologies": 0, "cases": 0, "gold": 0})
    declared_gold: set[str] = set()
    for entry in entries:
        totals[entry["corpus"]]["ontologies"] += 1
        files = entry["files"]
        for role, record in files.items():
            path = dataset_root / record["path"]
            if not path.is_file():
                errors.append(f"missing {role}: {record['path']}")
            elif sha256_file(path) != record["sha256"]:
                errors.append(f"hash mismatch {role}: {record['path']}")
        declared_gold.add(files["gold"]["path"])
        ontology_path = dataset_root / files["ontology"]["path"]
        if ontology_path.is_file():
            ontology = json.loads(ontology_path.read_text())
            compiled = compile_ontology(ontology, "CANARY SENTENCE")
            if len(compiled["relations"]) != len({item["label"] for item in ontology.get("relations", [])}):
                errors.append(f"ontology compiler relation loss: {entry['id']}")
            if compiled != compile_ontology(ontology, "CANARY SENTENCE"):
                errors.append(f"nondeterministic ontology compiler: {entry['id']}")
        if not errors or (dataset_root / files["gold"]["path"]).is_file():
            gold_rows = read_jsonl(dataset_root / files["gold"]["path"])
            test_rows = read_jsonl(dataset_root / files["test"]["path"])
            gold_ids, test_ids = [row["id"] for row in gold_rows], [row["id"] for row in test_rows]
            duplicate = (set(gold_ids) & seen_ids) | {x for x in gold_ids if gold_ids.count(x) > 1}
            if duplicate:
                errors.append(f"duplicate case ids in {entry['id']}: {sorted(duplicate)[:3]}")
            seen_ids.update(gold_ids)
            if gold_ids != test_ids:
                errors.append(f"test/gold id mismatch: {entry['id']}")
            cases, gold = len(gold_rows), sum(len(row["triples"]) for row in gold_rows)
            if cases != entry["case_count"] or gold != entry["gold_triple_count"]:
                errors.append(f"count mismatch: {entry['id']}")
            totals[entry["corpus"]]["cases"] += cases
            totals[entry["corpus"]]["gold"] += gold
    actual_gold = {str(path.relative_to(dataset_root)) for path in
                   (dataset_root / "data").glob("*/ground_truth/*_ground_truth.jsonl")}
    extra = sorted(actual_gold - declared_gold)
    if extra:
        errors.append(f"unmanifested ground truth: {extra}")
    expected = manifest["totals"]
    observed = {"ontologies": len(entries), "cases": len(seen_ids),
                "gold_triples": sum(value["gold"] for value in totals.values())}
    if observed != expected:
        errors.append(f"suite totals mismatch: {observed} != {expected}")
    if errors:
        raise ValueError("; ".join(errors))
    return {"pipeline": PIPELINE, "totals": observed, "corpora": dict(totals)}


def load_predictions(path: Path) -> dict[str, dict]:
    return {row["id"]: row for row in read_jsonl(path)}


def validate_reconcilers(registry: dict, ontology_ids: set[tuple[str, str]]) -> None:
    required = {"id", "version", "implementation_sha256", "scope", "pipeline", "mode",
                "input", "forbidden", "claim"}
    for item in registry.get("reconcilers", []):
        missing = required - set(item)
        if missing:
            raise ValueError(f"reconciler {item.get('id', '<unknown>')} missing {sorted(missing)}")
        if set(item) != required:
            raise ValueError(f"reconciler {item['id']} has unknown fields {sorted(set(item) - required)}")
        if "gold" in item["input"] or "gold" not in item["forbidden"]:
            raise ValueError(f"reconciler {item['id']} permits gold input")
        scope = {(corpus, ontology) for corpus in item["scope"].get("corpora", [])
                 for ontology in item["scope"].get("ontologies", [])}
        if not scope or not scope <= ontology_ids:
            raise ValueError(f"reconciler {item['id']} has invalid scope")
        if item["mode"] == "general" and scope != ontology_ids:
            raise ValueError(f"general reconciler {item['id']} must cover all ontologies")
        if item["mode"] not in {"general", "domain_specific"}:
            raise ValueError(f"reconciler {item['id']} has invalid mode")


def score_suite(dataset_root: Path, manifest: dict, response_root: Path, lever: str = "L0") -> dict:
    per_ontology, case_rows, stage_rows = [], [], []
    aggregate: dict[str, dict[str, list[int]]] = defaultdict(
        lambda: {"strict": [0, 0, 0], "relation_filtered": [0, 0, 0]})
    for entry in sorted(manifest["ontologies"], key=lambda x: (x["corpus"], x["id"])):
        ontology = json.loads((dataset_root / entry["files"]["ontology"]["path"]).read_text())
        gold_rows = read_jsonl(dataset_root / entry["files"]["gold"]["path"])
        response_file = (dataset_root / entry["files"]["upstream_response"]["path"] if lever == "L0"
                         else response_root / entry["corpus"] / f"{entry['id']}.jsonl")
        responses = load_predictions(response_file) if response_file.is_file() else {}
        counts = {"strict": [0, 0, 0], "relation_filtered": [0, 0, 0]}
        for row in gold_rows:
            case_id, gold = row["id"], {normalized_triple((t["sub"], t["rel"], t["obj"])) for t in row["triples"]}
            response = responses.get(case_id)
            accepted: set[tuple[str, str, str]] = set()
            stage = defaultdict(int)
            if response is None:
                stage["missing_output"] = 1
            else:
                payload, repaired = (({"triples": response["triples"]}, False)
                                     if lever == "L0" and isinstance(response.get("triples"), list)
                                     else parse_response(str(response.get("raw_response", response.get("response", "")))))
                if repaired:
                    stage["syntax_repaired"] += 1
                if payload is None:
                    stage["malformed_output"] += 1
                else:
                    candidates = payload.get("triples", []) if isinstance(payload, dict) else []
                    stage["generated"] = len(candidates)
                    for candidate in candidates:
                        if lever == "L0" and isinstance(candidate, list) and len(candidate) == 3:
                            verdict, value = "accepted", {"subject": candidate[0], "relation": candidate[1],
                                                          "object": candidate[2]}
                        else:
                            verdict, value = validate_candidate(candidate, row["sent"], ontology)
                        stage[verdict] += 1
                        if value:
                            normalized = normalized_triple((value["subject"], value["relation"], value["object"]))
                            if normalized in accepted:
                                stage["accepted"] -= 1
                                stage["duplicate"] += 1
                            else:
                                accepted.add(normalized)
                    rejected = sum(value for key, value in stage.items() if key not in
                                   {"generated", "accepted", "syntax_repaired"})
                    if stage["generated"] != len(accepted) + rejected:
                        raise RuntimeError(f"stage conservation failed for {case_id}")
            relations = {triple[1] for triple in gold}
            filtered = {triple for triple in accepted if triple[1] in relations}
            strict_values = (len(gold & accepted), len(accepted - gold), len(gold - accepted))
            filtered_values = (len(gold & filtered), len(filtered - gold), len(gold - filtered))
            for label, values in (("strict", strict_values), ("relation_filtered", filtered_values)):
                for index in range(3):
                    counts[label][index] += values[index]
            case_rows.append({"corpus": entry["corpus"], "ontology": entry["id"], "id": case_id,
                              "status": "ok" if response else "missing_output",
                              "strict": metric(*strict_values), "relation_filtered": metric(*filtered_values)})
            stage_rows.append({"corpus": entry["corpus"], "ontology": entry["id"], "id": case_id, **stage})
        for label in counts:
            for index in range(3):
                aggregate[entry["corpus"]][label][index] += counts[label][index]
        per_ontology.append({"corpus": entry["corpus"], "ontology": entry["id"],
                             **{label: metric(*values) for label, values in counts.items()}})
    corpus_rows = [{"corpus": corpus, **{label: metric(*values) for label, values in counts.items()}}
                   for corpus, counts in sorted(aggregate.items())]
    selected_counts = {"strict": [0, 0, 0], "relation_filtered": [0, 0, 0]}
    selected_ids: set[str] = set()
    for entry in manifest["ontologies"]:
        selected_file = entry["files"].get("selected_ids")
        if selected_file:
            selected_ids.update(line.strip() for line in
                                (dataset_root / selected_file["path"]).read_text().splitlines() if line.strip())
    for row in case_rows:
        if row["id"] in selected_ids:
            for label in selected_counts:
                for index, field in enumerate(("tp", "fp", "fn")):
                    selected_counts[label][index] += row[label][field]
    strata = [{"name": "wikidata_selected", "cases": len(selected_ids),
               **{label: metric(*values) for label, values in selected_counts.items()}}]

    unseen_counts = {"strict": [0, 0, 0], "relation_filtered": [0, 0, 0]}
    unseen_cases = 0
    for entry in manifest["ontologies"]:
        files = entry["files"]
        if "unseen_gold" not in files:
            continue
        ontology = json.loads((dataset_root / files["ontology"]["path"]).read_text())
        gold_rows = read_jsonl(dataset_root / files["unseen_gold"]["path"])
        response_file = (dataset_root / files["unseen_response"]["path"] if lever == "L0" else
                         response_root / entry["corpus"] / "unseen" / f"{entry['id']}.jsonl")
        responses = load_predictions(response_file) if response_file.is_file() else {}
        unseen_cases += len(gold_rows)
        for row in gold_rows:
            gold = {normalized_triple((t["sub"], t["rel"], t["obj"])) for t in row["triples"]}
            response = responses.get(row["id"], {})
            predicted: set[tuple[str, str, str]] = set()
            if lever == "L0":
                predicted = {normalized_triple(item) for item in response.get("triples", [])
                             if isinstance(item, list) and len(item) == 3}
            else:
                payload, _ = parse_response(str(response.get("raw_response", "")))
                for candidate in payload.get("triples", []) if isinstance(payload, dict) else []:
                    verdict, value = validate_candidate(candidate, row["sent"], ontology)
                    if verdict == "accepted" and value:
                        predicted.add(normalized_triple((value["subject"], value["relation"], value["object"])))
            filtered = {item for item in predicted if item[1] in {gold_item[1] for gold_item in gold}}
            for label, values in (("strict", predicted), ("relation_filtered", filtered)):
                counts = (len(gold & values), len(values - gold), len(gold - values))
                for index in range(3):
                    unseen_counts[label][index] += counts[index]
    strata.append({"name": "wikidata_unseen", "cases": unseen_cases,
                   **{label: metric(*values) for label, values in unseen_counts.items()}})
    return {"pipeline": PIPELINE, "lever": lever, "corpora": corpus_rows, "ontologies": per_ontology,
            "strata": strata, "cases": case_rows, "stages": stage_rows}
