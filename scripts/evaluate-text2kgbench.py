#!/usr/bin/env python3
"""Repeatable Text2KGBench extraction + Camayoc/Quipu ingress evaluation."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

from text2kg_grounded import canonical_relation, reconcile
from text2kg_general import (PIPELINE, canonical_json, compile_ontology, request_hash,
                             score_suite, sha256_bytes, smoke_ids, validate_manifest,
                             validate_reconcilers)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def norm(triple) -> tuple[str, str, str]:
    clean = lambda value: re.sub(r"[_\s]+", "", str(value)).lower()
    return tuple(clean(value) for value in triple)


def score(gold: set, predicted: set) -> dict:
    correct = len(gold & predicted)
    precision = correct / len(predicted) if predicted else 0.0
    recall = correct / len(gold) if gold else 0.0
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
    return {"correct": correct, "predicted": len(predicted), "gold": len(gold),
            "precision": precision, "recall": recall, "f1": f1}


def run(command: list[str], **kwargs) -> subprocess.CompletedProcess:
    result = subprocess.run(command, text=True, capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {' '.join(command)}\n"
            f"stdout: {result.stdout[-2000:]}\nstderr: {result.stderr[-2000:]}"
        )
    return result


def wait_health(port: int) -> None:
    for _ in range(100):
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=.2) as response:
                if response.status == 200:
                    return
        except Exception:
            time.sleep(.05)
    raise RuntimeError("disposable Quipu server did not become healthy")


def graph_readback(port: int) -> dict:
    with urllib.request.urlopen(f"http://127.0.0.1:{port}/graphs", timeout=2) as response:
        return json.load(response)


def legacy_main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset-root", required=True, type=Path)
    parser.add_argument("--camayoc-root", required=True, type=Path)
    parser.add_argument("--camayoc-python", required=True)
    parser.add_argument("--quipu-server", default="quipu-server")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--port", type=int, default=39173)
    parser.add_argument("--extraction-only", action="store_true",
                        help="score extraction without starting Camayoc/Quipu ingress")
    args = parser.parse_args(argv)

    root = Path(__file__).resolve().parents[1]
    cfg = json.loads((root / "evaluations/text2kgbench/config.json").read_text())
    dataset = cfg["dataset"]
    actual_commit = run(["git", "-C", str(args.dataset_root), "rev-parse", "HEAD"]).stdout.strip()
    if actual_commit != dataset["commit"]:
        raise SystemExit(f"dataset commit mismatch: {actual_commit}")
    paths = {name: args.dataset_root / item["path"] for name, item in dataset["files"].items()}
    hashes = {name: digest(path) for name, path in paths.items()}
    for name, actual in hashes.items():
        if actual != dataset["files"][name]["sha256"]:
            raise SystemExit(f"{name} sha256 mismatch: {actual}")

    count = dataset["selection"]["count"]
    gold_rows = jsonl(paths["gold"])[:count]
    predictions = {row["id"]: row for row in jsonl(paths["predictions"])}
    ontology = json.loads(paths["ontology"].read_text())
    ontology_relations = {canonical_relation(item["label"]) for item in ontology["relations"]}
    all_gold, strict_predictions, filtered_predictions = set(), set(), set()
    ingress_rows, cases, remaining_false_negatives = [], [], []
    errors = {"missing_output": 0, "malformed_triple": 0,
        "duplicate_prediction": 0, "relation_outside_ontology": 0,
        "relation_not_in_case_gold": 0, "false_positive_after_filter": 0, "false_negative": 0}

    for gold_row in gold_rows:
        case_id = gold_row["id"]
        gold = {norm((t["sub"], t["rel"], t["obj"])) for t in gold_row["triples"]}
        all_gold |= {(case_id, *triple) for triple in gold}
        raw = predictions.get(case_id, {}).get("triples")
        if raw is None:
            errors["missing_output"] += 1
            raw = []
        raw = reconcile(gold_row["sent"], raw)
        valid = []
        for index, triple in enumerate(raw):
            if not isinstance(triple, list) or len(triple) != 3:
                errors["malformed_triple"] += 1
                continue
            valid.append(triple)
            ingress_rows.append({"row_id": f"{case_id}-{index}", "case_id": case_id,
                "subject": str(triple[0]), "relation": str(triple[1]), "object": str(triple[2])})
        normalized = [norm(triple) for triple in valid]
        errors["duplicate_prediction"] += len(normalized) - len(set(normalized))
        strict = set(normalized)
        case_relations = {triple[1].replace(" ", "_") for triple in
                          [(t["sub"], t["rel"], t["obj"]) for t in gold_row["triples"]]}
        filtered = {triple for triple in strict if triple[1] in {norm(("", r, ""))[1] for r in case_relations}}
        errors["relation_outside_ontology"] += sum(
            1 for triple in valid if canonical_relation(triple[1]) not in ontology_relations
        )
        errors["relation_not_in_case_gold"] += len(strict - filtered)
        errors["false_positive_after_filter"] += len(filtered - gold)
        errors["false_negative"] += len(gold - filtered)
        remaining_false_negatives.extend(
            {"id": case_id, "subject": triple[0], "relation": triple[1], "object": triple[2]}
            for triple in sorted(gold - filtered)
        )
        strict_predictions |= {(case_id, *triple) for triple in strict}
        filtered_predictions |= {(case_id, *triple) for triple in filtered}
        cases.append({"id": case_id, "strict": score(gold, strict), "benchmark_filtered": score(gold, filtered)})

    args.output.mkdir(parents=True, exist_ok=True)
    source = args.output / "predictions.json"
    source.write_text(json.dumps(ingress_rows, indent=2, sort_keys=True) + "\n")
    ingress = None
    if not args.extraction_only:
        mapping = root / "evaluations/text2kgbench/mapping.ttl"
        rml = args.camayoc_root / "scripts/rml_executor.py"
        rml_cmd = [args.camayoc_python, str(rml), "execute",
            "https://example.invalid/text2kgbench/prediction-map", "--mapping-file", str(mapping),
            "--source-file", str(source), "--allowed-root", str(args.output)]
        materialized = json.loads(run(rml_cmd + ["--dry-run"]).stdout)

        with tempfile.TemporaryDirectory(prefix="text2kg-quipu-") as temp:
            db = Path(temp) / "store.db"
            server = subprocess.Popen([args.quipu_server, "--db", str(db), "--bind", f"127.0.0.1:{args.port}"],
                                      cwd=temp, stdout=subprocess.DEVNULL,
                                      stderr=subprocess.PIPE, text=True)
            try:
                wait_health(args.port)
                plane_env = {**os.environ, "QUIPU_SERVER": f"http://127.0.0.1:{args.port}",
                             "CAMAYOC_PLANE_NS": "https://camayoc.local/plane/"}
                run([args.camayoc_python, str(args.camayoc_root / "scripts/planes.py"),
                     "ensure", "--timestamp", "2026-09-03T00:00:00Z"], env=plane_env)
                committed = json.loads(run(rml_cmd + ["--server", f"http://127.0.0.1:{args.port}",
                                                       "--actor", "text2kgbench-eval"]).stdout)
                readback = graph_readback(args.port)
            finally:
                server.terminate()
                server.wait(timeout=5)
        ingress = {"input_triples": len(ingress_rows), "materialized_quads": materialized["output_count"],
                   "mapping_hash": materialized["mapping_hash"], "source_hash": materialized["source_hash"],
                   "write": committed["write"], "graph_readback": readback}

    report = {"schema_version": 1,
        "evaluation_scope": {
            "extraction": "hash-pinned upstream text-to-triples replay",
            "ingress": "Camayoc governed RML write to disposable Quipu",
            "claim": "separate boundary measurements; upstream artifact is not graph-extract output",
        },
        "dataset": dataset, "observed_hashes": hashes,
        "inference": cfg["inference"], "scorer": cfg["scorer"], "cases": len(gold_rows),
        "extraction": {"strict_micro": score(all_gold, strict_predictions),
                       "benchmark_filtered_micro": score(all_gold, filtered_predictions),
                       "per_case": cases},
        "error_decomposition": errors,
        "remaining_false_negatives": remaining_false_negatives,
        "declared_non_goals": cfg["scorer"]["declared_non_goals"],
        "ingress": ingress}
    args.output.joinpath("report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"report": str(args.output / "report.json"), "strict": report["extraction"]["strict_micro"],
                      "filtered": report["extraction"]["benchmark_filtered_micro"],
                      "errors": errors, "ingress": report["ingress"]}, indent=2, sort_keys=True))
    floors = cfg["scorer"]["regression_floor"]
    observed = report["extraction"]
    regressions = []
    for label, actual, floor in (
        ("strict_micro_f1", observed["strict_micro"]["f1"], floors["strict_micro_f1"]),
        ("benchmark_filtered_micro_f1", observed["benchmark_filtered_micro"]["f1"],
         floors["benchmark_filtered_micro_f1"]),
        ("recall", observed["strict_micro"]["recall"], floors["recall"]),
    ):
        if actual < floor:
            regressions.append(f"{label} {actual:.6f} < {floor:.6f}")
    if regressions:
        print("REGRESSION: " + "; ".join(regressions))
        return 2
    declared_ids = {case_id for item in cfg["scorer"]["declared_non_goals"]
                    for case_id in item["ids"]}
    unexplained = sorted({item["id"] for item in remaining_false_negatives} - declared_ids)
    if unexplained:
        print("UNDECLARED NON-GOALS: " + ", ".join(unexplained))
        return 3
    return 0


def load_manifest(path: Path) -> dict:
    return json.loads(path.read_text())


def load_provider(manifest: dict, requested: str | None):
    """Resolve a provider id to (module, profile) through providers.json.

    Selection is configuration. Nothing here names a module path, so adding an
    inference back end is a registry edit plus a file that satisfies the
    preflight/create_message contract -- never an edit to this driver.

    The two drift guards matter more than the indirection: a provider module that
    disagrees with the registry about its own model, and a default provider that
    disagrees with the manifest, both silently change what `request_hash` binds.
    That is exactly the kind of divergence a response ledger cannot show you later.
    """
    from importlib.util import module_from_spec, spec_from_file_location
    root = Path(__file__).resolve().parents[1] / "evaluations/text2kgbench"
    registry = json.loads((root / "providers.json").read_text())
    provider_id = requested or manifest["inference"]["provider"]
    if provider_id not in registry["providers"]:
        raise SystemExit(f"unknown provider {provider_id!r}; "
                         f"registered: {sorted(registry['providers'])}")
    profile = {"id": provider_id, **registry["providers"][provider_id]}
    if provider_id == manifest["inference"]["provider"]:
        for field in ("model", "decoding"):
            if profile[field] != manifest["inference"][field]:
                raise SystemExit(f"provider registry {field} differs from the manifest's "
                                 f"pinned inference {field} for {provider_id}")
    path = root / profile["module"]
    spec = spec_from_file_location(f"text2kg_provider_{provider_id}", path)
    assert spec and spec.loader
    module = module_from_spec(spec); spec.loader.exec_module(module)
    if hasattr(module, "configure"):
        module.configure(profile)
    declared = getattr(module, "MODEL", None)
    if declared is not None and declared != profile["model"]:
        raise SystemExit(f"provider {provider_id} module pins model {declared!r}, "
                         f"registry says {profile['model']!r}")
    return module, profile


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def general_main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Full-suite ontology-guided Text2KGBench evaluation")
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("inventory", "score"):
        command = sub.add_parser(name)
        command.add_argument("--dataset-root", required=True, type=Path)
        command.add_argument("--manifest", required=True, type=Path)
        if name == "score":
            command.add_argument("--responses", required=True, type=Path)
            command.add_argument("--output", required=True, type=Path)
            command.add_argument("--lever", choices=("L0", "L1"), default="L0")
    command = sub.add_parser("run")
    command.add_argument("--dataset-root", required=True, type=Path)
    command.add_argument("--manifest", required=True, type=Path)
    command.add_argument("--responses", required=True, type=Path)
    command.add_argument("--output", required=True, type=Path)
    command.add_argument("--smoke", action="store_true")
    command.add_argument("--max-cases", type=int, metavar="N",
                         help="stop after the first N cases in the manifest's deterministic "
                              "order (sorted by corpus, then ontology id, then row order). A "
                              "CONTIGUOUS PREFIX, not a sample: the same N always selects the "
                              "same cases, so a block is re-runnable and comparable. Counts "
                              "cases SELECTED, not cases newly inferred, so a resumed run "
                              "covers the same block rather than extending past it.")
    command.add_argument("--confirm-cost", type=float)
    command.add_argument("--provider", help="provider id from evaluations/text2kgbench/providers.json "
                                            "(default: the manifest's inference.provider)")
    args = parser.parse_args(argv)
    manifest = load_manifest(args.manifest)
    provider = profile = None
    if args.command == "run":
        provider, profile = load_provider(manifest, args.provider)
        provider.preflight()
    actual = run(["git", "-C", str(args.dataset_root), "rev-parse", "HEAD"]).stdout.strip()
    if actual != manifest["dataset"]["commit"]:
        raise SystemExit(f"dataset commit mismatch: {actual}")
    inventory = validate_manifest(args.dataset_root, manifest)
    registry_path = Path(__file__).resolve().parents[1] / "evaluations/text2kgbench/reconcilers.json"
    registry = json.loads(registry_path.read_text())
    if digest(registry_path) != manifest["reconciler_registry_sha256"]:
        raise SystemExit("reconciler registry hash mismatch")
    validate_reconcilers(registry, {(item["corpus"], item["id"]) for item in manifest["ontologies"]})
    if args.command == "inventory":
        print(json.dumps(inventory, indent=2, sort_keys=True))
        return 0
    if args.command == "score":
        result = score_suite(args.dataset_root, manifest, args.responses, args.lever)
        args.output.mkdir(parents=True, exist_ok=True)
        write_json(args.output / "cases.json", result["cases"])
        write_json(args.output / "errors.json", result["stages"])
        write_json(args.output / "ontology-summary.json", result["ontologies"])
        write_json(args.output / "corpus-summary.json", result["corpora"])
        write_json(args.output / "strata-summary.json", result["strata"])
        artifacts = {path.name: digest(path) for path in sorted(args.output.glob("*.json"))}
        report = {"schema_version": 3, "pipeline": PIPELINE, "scope": inventory,
                  "scorer": manifest["scorer"], "artifacts": artifacts}
        write_json(args.output / "report.json", report)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0

    if not args.smoke and args.confirm_cost is None:
        raise SystemExit("full L1 run requires --confirm-cost <amount> after smoke projection")
    assert provider is not None
    selected = set(manifest["smoke_panel"]["ids"]) if args.smoke else None
    requests_path = args.output / "requests.jsonl"
    responses_path = args.output / "responses.jsonl"
    args.output.mkdir(parents=True, exist_ok=True)
    # Cases considered so far, for --max-cases. Counted after the smoke filter and
    # BEFORE the already-done check, so the block is a fixed prefix of the case
    # order rather than "N more than last time" (aegis-xid7v6).
    considered = 0
    for entry in sorted(manifest["ontologies"], key=lambda item: (item["corpus"], item["id"])):
        if args.max_cases is not None and considered >= args.max_cases:
            break
        ontology = json.loads((args.dataset_root / entry["files"]["ontology"]["path"]).read_text())
        rows = jsonl(args.dataset_root / entry["files"]["test"]["path"])
        destination = args.responses / entry["corpus"] / f"{entry['id']}.jsonl"
        existing = {row["id"]: row for row in jsonl(destination)} if destination.exists() else {}
        output_rows = list(existing.values())
        for row in rows:
            if selected is not None and row["id"] not in selected:
                continue
            if args.max_cases is not None and considered >= args.max_cases:
                break
            considered += 1
            request = compile_ontology(ontology, row["sent"])
            prompt = canonical_json(request)
            decoding = profile["decoding"]
            expected_hash = request_hash(request, profile["model"], decoding)
            if row["id"] in existing:
                if existing[row["id"]]["request_hash"] != expected_hash:
                    raise SystemExit(f"refusing to overwrite changed request: {row['id']}")
                continue
            request_row = {"id": row["id"], "corpus": entry["corpus"], "ontology": entry["id"],
                           "request_hash": expected_hash, "prompt_sha256": sha256_bytes(prompt.encode())}
            with requests_path.open("a") as handle:
                handle.write(canonical_json(request_row) + "\n")
            started = time.monotonic()
            response = provider.create_message({"prompt": prompt, "model": profile["model"],
                                                "decoding": decoding})
            response_row = {**request_row, **response, "latency_seconds": time.monotonic() - started,
                            "decoding": decoding, "provider": profile["id"],
                            "response_sha256": sha256_bytes(response["raw_response"].encode())}
            output_rows.append(response_row)
            with responses_path.open("a") as handle:
                handle.write(canonical_json(response_row) + "\n")
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text("".join(canonical_json(item) + "\n" for item in sorted(output_rows, key=lambda x: x["id"])))
    return 0


def main() -> int:
    import sys
    if len(sys.argv) > 1 and sys.argv[1] in {"inventory", "run", "score"}:
        return general_main(sys.argv[1:])
    return legacy_main()


if __name__ == "__main__":
    raise SystemExit(main())
