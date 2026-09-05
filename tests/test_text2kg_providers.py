"""Provider selection is configuration, and the Claude Code adapter honours the contract."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from text2kg_general import request_hash  # noqa: E402

EVAL = ROOT / "evaluations/text2kgbench"
MANIFEST = json.loads((EVAL / "full-manifest.json").read_text())
REGISTRY = json.loads((EVAL / "providers.json").read_text())


def driver():
    spec = importlib.util.spec_from_file_location("evaluate_text2kgbench",
                                                  ROOT / "scripts/evaluate-text2kgbench.py")
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


def load_adapter(name: str):
    path = EVAL / "providers" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(f"adapter_{name}", path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


def test_default_provider_keeps_the_manifest_pin_and_request_hashes():
    """The registry must not quietly redefine the pinned L1 configuration."""
    module, profile = driver().load_provider(MANIFEST, None)
    assert profile["id"] == MANIFEST["inference"]["provider"] == "anthropic_messages"
    assert profile["model"] == MANIFEST["inference"]["model"]
    assert profile["decoding"] == MANIFEST["inference"]["decoding"]
    assert module.MODEL == profile["model"]
    request = {"sentence": "Alice alpha Bob."}
    assert request_hash(request, profile["model"], profile["decoding"]) == request_hash(
        request, MANIFEST["inference"]["model"], MANIFEST["inference"]["decoding"])


def test_claude_code_is_selectable_by_id_and_adopts_registry_pins():
    module, profile = driver().load_provider(MANIFEST, "claude_code")
    assert profile["id"] == "claude_code"
    assert profile["billing"] == "claude-subscription-oauth"
    assert module.MODEL == profile["model"]
    assert module.CLI_PIN == profile["runtime_pin"]


def test_claude_code_declares_unpinned_decoding_rather_than_claiming_zero():
    """The CLI has no sampling controls; the profile must not claim it set them."""
    decoding = REGISTRY["providers"]["claude_code"]["decoding"]
    assert decoding["temperature"] is None and decoding["top_p"] is None
    assert decoding != MANIFEST["inference"]["decoding"]


def test_unknown_provider_names_what_is_registered():
    with pytest.raises(SystemExit, match="unknown provider"):
        driver().load_provider(MANIFEST, "does_not_exist")


def test_registry_disagreeing_with_the_manifest_is_refused():
    """If the default provider's registry row drifts from the pinned inference block,
    every request_hash silently changes. Refuse instead."""
    manifest = {**MANIFEST, "inference": {**MANIFEST["inference"], "provider": "claude_code"}}
    with pytest.raises(SystemExit, match="differs from the manifest"):
        driver().load_provider(manifest, "claude_code")


def test_module_disagreeing_with_the_registry_is_refused(tmp_path, monkeypatch):
    """A provider module that pins a different model than its registry row."""
    registry = {"default": "rogue", "providers": {"rogue": {
        "module": "providers/rogue.py", "model": "claude-haiku-4-5-20251001",
        "runtime": "x", "runtime_pin": "1", "billing": "x", "decoding": {}}}}
    staging = tmp_path / "evaluations/text2kgbench/providers"
    staging.mkdir(parents=True)
    (staging.parent / "providers.json").write_text(json.dumps(registry))
    (staging / "rogue.py").write_text("MODEL = 'some-other-model'\n")
    driver_copy = tmp_path / "scripts/evaluate-text2kgbench.py"
    driver_copy.parent.mkdir(parents=True)
    driver_copy.write_text((ROOT / "scripts/evaluate-text2kgbench.py").read_text())
    spec = importlib.util.spec_from_file_location("driver_copy", driver_copy)
    copied = importlib.util.module_from_spec(spec)
    monkeypatch.syspath_prepend(str(ROOT / "scripts"))
    spec.loader.exec_module(copied)
    with pytest.raises(SystemExit, match="module pins model"):
        copied.load_provider({"inference": {"provider": "rogue",
                                            "model": "claude-haiku-4-5-20251001",
                                            "decoding": {}}}, "rogue")


def test_preflight_refuses_an_api_key_because_that_bills_money(monkeypatch):
    """Mirror image of the Anthropic adapter: a PRESENT key is the failure here."""
    monkeypatch.setenv("ANTHROPIC_API_KEY", "sk-must-not-be-used")
    adapter = load_adapter("claude_code")
    with pytest.raises(RuntimeError, match="subscription budget"):
        adapter.create_message({"prompt": "never sent"})


def test_preflight_refuses_custom_endpoints(monkeypatch):
    monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
    monkeypatch.setenv("ANTHROPIC_BASE_URL", "http://elsewhere.invalid")
    adapter = load_adapter("claude_code")
    with pytest.raises(RuntimeError, match="endpoints and proxies"):
        adapter.create_message({"prompt": "never sent"})


PAYLOAD = {
    "result": '{"triples": []}',
    "session_id": "0884bca2-f89b-44a6-8dc5-df224b0ef578",
    "stop_reason": "end_turn",
    "is_error": False,
    "subtype": "success",
    "num_turns": 1,
    "duration_api_ms": 1941,
    "usage": {"input_tokens": 936, "output_tokens": 578},
    "modelUsage": {"claude-haiku-4-5-20251001": {
        "inputTokens": 2000, "outputTokens": 594, "cacheReadInputTokens": 500,
        "cacheCreationInputTokens": 35, "costUSD": 0.005505, "thinkingTokens": 53}},
}


def test_row_reports_session_totals_not_just_the_scored_request():
    """One `claude -p` bills more than the scored call; the ledger must say so."""
    adapter = load_adapter("claude_code")
    row = adapter._row(PAYLOAD, "2.1.261", "claude-haiku-4-5-20251001")
    assert row["input_tokens"] == 2535        # 2000 + 500 cache read + 35 cache creation
    assert row["api_input_tokens"] == 936     # the scored request alone
    assert row["input_tokens"] > row["api_input_tokens"]
    assert row["output_tokens"] == 594 and row["api_output_tokens"] == 578
    assert row["model"] == "claude-haiku-4-5-20251001"
    assert row["provider_request_id"] == PAYLOAD["session_id"]
    assert row["finish_reason"] == "end_turn" and row["refusal"] is False
    assert row["sdk_version"] == "claude-code==2.1.261"
    assert row["list_cost_usd"] == pytest.approx(0.005505)


def test_empty_result_is_recorded_as_a_refusal():
    adapter = load_adapter("claude_code")
    assert adapter._row({**PAYLOAD, "result": "   "}, "2.1.261", "m")["refusal"] is True


def _stub(adapter, monkeypatch, outcomes):
    calls = {"n": 0}

    def fake(prompt, model, timeout):
        index = min(calls["n"], len(outcomes) - 1)
        calls["n"] += 1
        payload, code = outcomes[index]
        return subprocess.CompletedProcess([], code, stdout=json.dumps(payload), stderr="")

    monkeypatch.setattr(adapter, "_invoke", fake)
    monkeypatch.setattr(adapter, "preflight", lambda: "2.1.261")
    return calls


def test_transient_api_status_is_retried_then_succeeds(monkeypatch):
    adapter = load_adapter("claude_code")
    overloaded = ({"is_error": True, "subtype": "error", "api_error_status": 529}, 1)
    calls = _stub(adapter, monkeypatch, [overloaded, (PAYLOAD, 0)])
    row = adapter.create_message({"prompt": "p"}, sleep=lambda _: None)
    assert calls["n"] == 2 and row["raw_response"] == '{"triples": []}'


def test_non_transient_failure_is_raised_without_retrying(monkeypatch):
    adapter = load_adapter("claude_code")
    refused = ({"is_error": True, "subtype": "error", "api_error_status": 400}, 1)
    calls = _stub(adapter, monkeypatch, [refused])
    with pytest.raises(RuntimeError, match="claude -p failed"):
        adapter.create_message({"prompt": "p"}, sleep=lambda _: None)
    assert calls["n"] == 1


def test_session_context_is_scrubbed_from_the_child_environment(monkeypatch):
    """The calling agent's session must not leak into an extraction case."""
    monkeypatch.setenv("CLAUDE_CODE_SESSION_ID", "parent-session")
    monkeypatch.setenv("CLAUDE_CODE_MESSAGING_TOKEN", "parent-token")
    monkeypatch.setenv("CLAUDE_EFFORT", "high")
    monkeypatch.setenv("ANTHROPIC_API_KEY", "sk-leak")
    adapter = load_adapter("claude_code")
    env = adapter._environment()
    assert "CLAUDE_CODE_SESSION_ID" not in env and "CLAUDE_CODE_MESSAGING_TOKEN" not in env
    assert "CLAUDE_EFFORT" not in env and "ANTHROPIC_API_KEY" not in env
    assert env["CLAUDE_CODE_ENTRYPOINT"] == "text2kgbench-eval"


def test_profile_env_pins_reach_the_child_environment():
    """The thinking budget is the difference between 145 and 7,223 output tokens
    per case, so the pin must be provably applied, not merely declared."""
    module, profile = driver().load_provider(MANIFEST, "claude_code")
    assert profile["env"]["MAX_THINKING_TOKENS"] == "0"
    assert profile["decoding"]["thinking_budget_tokens"] == 0
    assert module._environment()["MAX_THINKING_TOKENS"] == "0"
