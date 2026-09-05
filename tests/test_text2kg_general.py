from __future__ import annotations

import importlib.util
import json
import os
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from text2kg_general import (SMOKE_SEED, compile_ontology, parse_response, score_suite, stratified_ids,
                             smoke_ids, validate_candidate, validate_manifest,
                             validate_reconcilers)


ONTOLOGY = {
    "id": "synthetic",
    "title": "Synthetic",
    "concepts": [{"qid": "B", "label": "Beta"}, {"qid": "A", "label": "Alpha"}],
    "relations": [{"pid": "P2", "label": "zeta", "domain": "A", "range": "B"},
                  {"pid": "P1", "label": "alpha", "domain": "A", "range": "B"}],
}


def test_compiler_is_deterministic_data_guided_and_target_last():
    first = compile_ontology(ONTOLOGY, "Alice alpha Bob.")
    renamed = compile_ontology({**ONTOLOGY, "relations": [{**ONTOLOGY["relations"][0], "label": "omega"}]},
                               "Alice omega Bob.")
    assert list(first)[-1] == "sentence"
    assert first["output_schema"]["properties"]["triples"]["items"]["properties"]["relation"]["enum"] == ["alpha", "zeta"]
    assert renamed["output_schema"]["properties"]["triples"]["items"]["properties"]["relation"]["enum"] == ["omega"]
    assert compile_ontology(ONTOLOGY, "Alice alpha Bob.") == first


def test_gold_canary_never_enters_prompt():
    prompt = json.dumps(compile_ontology(ONTOLOGY, "Alice alpha Bob."))
    assert "UNIQUE_GOLD_ONLY_CANARY" not in prompt


def test_validator_rejects_unknown_relation_and_ungrounded_span():
    good = {"subject": "Alice", "relation": "alpha", "object": "Bob", "evidence_span": "Alice alpha Bob"}
    assert validate_candidate(good, "Alice alpha Bob.", ONTOLOGY)[0] == "accepted"
    assert validate_candidate({**good, "relation": "unknown"}, "Alice alpha Bob.", ONTOLOGY)[0] == "relation_outside_ontology"
    assert validate_candidate({**good, "evidence_span": "gold-only-token"}, "Alice alpha Bob.", ONTOLOGY)[0] == "evidence_not_grounded"


def test_parser_only_repairs_fenced_json_and_trailing_comma():
    payload, repaired = parse_response('```json\n{"triples": [],}\n```')
    assert payload == {"triples": []} and repaired
    assert parse_response("prefix {\"triples\": []} suffix") == (None, False)


def test_smoke_selection_is_seeded_and_order_independent():
    ids = [f"case-{index}" for index in range(20)]
    assert SMOKE_SEED == "aegis-tydvlg.4-l1-smoke-v1"
    assert smoke_ids(ids) == smoke_ids(reversed(ids))
    assert len(smoke_ids(ids)) == 5


def make_suite(tmp_path: Path) -> tuple[Path, dict]:
    dataset = tmp_path / "dataset"
    files = dataset / "data/example"
    files.mkdir(parents=True)
    values = {
        "test.jsonl": '{"id":"case-1","sent":"Alice alpha Bob."}\n',
        "gold.jsonl": '{"id":"case-1","sent":"Alice alpha Bob.","triples":[{"sub":"Alice","rel":"alpha","obj":"Bob"}]}\n',
        "ontology.json": json.dumps(ONTOLOGY),
        "upstream.jsonl": '{"id":"case-1","triples":[["Alice","alpha","Bob"]]}\n',
    }
    for name, value in values.items():
        (files / name).write_text(value)
    import hashlib
    records = {key.removesuffix(".jsonl").removesuffix(".json"): {
        "path": f"data/example/{key}", "sha256": hashlib.sha256(value.encode()).hexdigest()}
        for key, value in values.items()}
    records["upstream_response"] = records.pop("upstream")
    manifest = {"totals": {"ontologies": 1, "cases": 1, "gold_triples": 1}, "ontologies": [{
        "id": "synthetic", "corpus": "example", "case_count": 1, "gold_triple_count": 1,
        "files": records}]}
    return dataset, manifest


def test_missing_response_stays_in_denominator(tmp_path: Path):
    dataset, manifest = make_suite(tmp_path)
    result = score_suite(dataset, manifest, tmp_path / "absent", "L1")
    assert len(result["cases"]) == 1
    assert result["cases"][0]["status"] == "missing_output"
    assert result["corpora"][0]["strict"]["fn"] == 1


def test_manifest_wrong_universe_fails(tmp_path: Path):
    dataset, manifest = make_suite(tmp_path)
    with pytest.raises(ValueError, match="expected 29 ontologies"):
        validate_manifest(dataset, manifest)


def test_committed_manifest_pins_complete_smoke_panel():
    manifest = json.loads((ROOT / "evaluations/text2kgbench/full-manifest.json").read_text())
    assert manifest["totals"] == {"ontologies": 29, "cases": 6076, "gold_triples": 13491}
    assert len(manifest["ontologies"]) == 29
    assert manifest["smoke_panel"]["count"] == len(manifest["smoke_panel"]["ids"]) == 145
    assert all(len(item["smoke_ids"]) == 5 for item in manifest["ontologies"])


def test_reconciler_registry_rejects_gold_and_partial_general_scope():
    universe = {("a", "one"), ("b", "two")}
    base = {"id": "x", "version": "1", "implementation_sha256": "a" * 64,
            "scope": {"corpora": ["a"], "ontologies": ["one"]}, "pipeline": "p",
            "mode": "domain_specific", "input": ["sentence"], "forbidden": ["gold"], "claim": "x"}
    validate_reconcilers({"reconcilers": [base]}, universe)
    with pytest.raises(ValueError, match="permits gold"):
        validate_reconcilers({"reconcilers": [{**base, "input": ["gold"]}]}, universe)
    with pytest.raises(ValueError, match="cover all"):
        validate_reconcilers({"reconcilers": [{**base, "mode": "general"}]}, universe)


def test_adapter_fails_before_import_without_key(monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    path = ROOT / "evaluations/text2kgbench/providers/anthropic_messages.py"
    spec = importlib.util.spec_from_file_location("adapter_under_test", path)
    module = importlib.util.module_from_spec(spec); assert spec and spec.loader
    spec.loader.exec_module(module)
    with pytest.raises(RuntimeError, match="ANTHROPIC_API_KEY"):
        module.create_message({"prompt": "never sent"})


# ── stratified selection (aegis-mgw4x1) ──────────────────────────────────────
#
# These exist because `--max-cases` shipped without a unit test: `run` validates
# the 29-ontology universe and the dataset's git commit, so testing selection
# through the CLI needs a whole fixture dataset (aegis-687e2d). Extracting the
# selector into a pure function is what makes it testable with a plain dict.

def _groups(**sizes):
    return [(gid, [f"{gid}_case_{i}" for i in range(n)]) for gid, n in sizes.items()]


def test_stratified_covers_every_ontology_even_when_small():
    """The floor is the point: a small ontology must not round to zero.

    A prefix of the corpus covered 10 of 29 ontologies; that is the failure this
    replaces, and a proportional sample without a floor reproduces it quietly.
    """
    groups = _groups(big=1000, medium=100, tiny=3)
    picked = stratified_ids(groups, 50)
    assert len(picked) == 50
    covered = {cid.rsplit("_case_", 1)[0] for cid in picked}
    assert covered == {"big", "medium", "tiny"}, f"an ontology vanished: {covered}"


def test_stratified_is_proportional_to_group_size():
    groups = _groups(big=900, small=100)
    picked = stratified_ids(groups, 100)
    counts = {}
    for cid in picked:
        counts[cid.rsplit("_case_", 1)[0]] = counts.get(cid.rsplit("_case_", 1)[0], 0) + 1
    assert sum(counts.values()) == 100
    # 90/10 split, allowing the floor and largest-remainder rounding a little room
    assert 85 <= counts["big"] <= 92, counts
    assert 8 <= counts["small"] <= 15, counts


def test_stratified_is_deterministic_and_order_independent():
    """Same (groups, total) -> same ids, however the input is ordered.

    Reproducibility is not traded away for representativeness: a stratified block
    must be re-runnable and comparable exactly as a prefix block is.
    """
    groups = _groups(a=50, b=30, c=20)
    first = stratified_ids(groups, 25)
    assert first == stratified_ids(list(reversed(groups)), 25)
    assert first == stratified_ids(groups, 25)


def test_stratified_totals_are_exact():
    groups = _groups(a=17, b=23, c=41, d=7)
    for total in (4, 5, 10, 33, 60, 87):
        picked = stratified_ids(groups, total)
        assert len(picked) == total, f"total {total} produced {len(picked)}"
        assert len(set(picked)) == len(picked), "duplicate case ids"


def test_stratified_returns_everything_when_the_budget_exceeds_the_corpus():
    groups = _groups(a=3, b=2)
    assert len(stratified_ids(groups, 99)) == 5


def test_stratified_below_group_count_still_spreads():
    """Fewer cases than groups: pick distinct groups, never several from one."""
    groups = _groups(a=10, b=10, c=10, d=10)
    picked = stratified_ids(groups, 2)
    assert len(picked) == 2
    assert len({cid.rsplit("_case_", 1)[0] for cid in picked}) == 2


def test_stratified_ignores_empty_groups():
    groups = _groups(a=5) + [("empty", [])]
    picked = stratified_ids(groups, 3)
    assert len(picked) == 3
    assert all(cid.startswith("a_") for cid in picked)
