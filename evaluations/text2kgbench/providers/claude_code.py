"""Claude Code CLI adapter for Text2KGBench L1.

Same input/output contract as ``anthropic_messages`` so ``run`` and ``score`` do
not change. The difference is the billing path: this adapter shells out to the
headless Claude Code CLI (``claude -p --output-format json``), which authenticates
with the operator's Claude subscription OAuth credentials. It therefore spends
subscription budget (governor points), not API dollars.

Two honest divergences from the Anthropic Messages adapter, recorded here and in
``providers.json`` rather than papered over:

1. The CLI exposes no temperature / top_p / max_tokens controls, so decoding is
   the CLI default. The profile declares those as null; claiming ``temperature: 0``
   for this path would manufacture provenance. The thinking budget IS controllable
   and the profile pins it to 0 -- left at its default it spent up to 7,223 thinking
   tokens on one triple extraction, which is not a comparable generation to the
   Anthropic path's 512-token non-thinking cap.
2. One ``claude -p`` invocation issues more than the single scored request (the
   session's own bookkeeping call). ``input_tokens`` / ``output_tokens`` therefore
   report the ``modelUsage`` SESSION totals -- the figure that is actually billed --
   while ``api_input_tokens`` / ``api_output_tokens`` carry the scored request's own
   usage. Reporting only the latter would understate the cost of the full run.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import time

# Defaults; ``configure()`` overrides both from providers.json so the pinned model
# and CLI version are config, not code.
MODEL = "claude-haiku-4-5-20251001"
CLI_PIN = "2.1.261"
# Environment pins the profile declares (e.g. the thinking budget). Config, not code.
ENV_PINS: dict[str, str] = {}

# Session/bridge variables that would leak the CALLING agent's context into the
# extraction subprocess. Cleared so a case's prompt is the whole input.
_SCRUBBED_PREFIXES = ("CLAUDE_CODE_", "ANTHROPIC_")
_SCRUBBED_NAMES = ("CLAUDE_PID", "CLAUDE_EFFORT", "CLAUDE_CONFIG_DIR",
                   # CLAUDECODE has no underscore after CLAUDE, so the
                   # "CLAUDE_CODE_" prefix does not match it. It IS set in
                   # an agent session, and leaking it tells the child it is
                   # running inside Claude Code — exactly the nondeterminism
                   # the scrub exists to remove.
                   "CLAUDECODE")

# Every flag here removes a source of nondeterminism or of fleet blast radius.
_BASE_ARGS = (
    "--print",
    "--output-format", "json",
    "--tools", "",                 # no tool use at all: this is a text-in/text-out call
    "--system-prompt", "",         # drop the agent system prompt; the case prompt is the input
    "--disable-slash-commands",
    "--strict-mcp-config",         # with no --mcp-config: no MCP servers
    "--setting-sources", "",       # ignore user/project/local settings
    "--permission-prompts", "none",
    "--no-session-persistence",    # 6,076 sessions must not accumulate on disk
)


def configure(profile: dict) -> None:
    """Adopt the model, CLI pin and environment pins declared in providers.json."""
    global MODEL, CLI_PIN, ENV_PINS
    MODEL = profile["model"]
    CLI_PIN = profile["runtime_pin"]
    ENV_PINS = dict(profile.get("env") or {})


def _binary() -> str:
    path = shutil.which("claude")
    if not path:
        raise RuntimeError("claude CLI not found on PATH")
    return path


def preflight() -> str:
    """Refuse anything that would change the auth path, the endpoint, or the build.

    The mirror image of the Anthropic adapter's preflight: there an absent API key
    is the failure; here a PRESENT one is, because it silently moves the run off
    the subscription and onto metered API billing.
    """
    for name in ("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"):
        if os.environ.get(name):
            raise RuntimeError(
                f"{name} is set: the Claude Code provider must spend subscription "
                "budget, not API credit. Unset it or use the anthropic_messages provider."
            )
    if os.environ.get("ANTHROPIC_BASE_URL") or os.environ.get("HTTPS_PROXY") or os.environ.get("HTTP_PROXY"):
        raise RuntimeError("custom Anthropic endpoints and proxies are forbidden")
    return _verified_cli_version()


_CLI_VERSION_CACHE: dict[str, str] = {}


def _verified_cli_version() -> str:
    """The pinned CLI version, probed once per process.

    `create_message` calls `preflight` on EVERY case and the driver already
    calls it once up front, so this shelled `claude --version` about 1,000
    extra times in a 1,000-case run — pure wall-clock inside the very window
    the run exists to measure.

    Only the SUBPROCESS is cached. The environment checks above stay per-call:
    they are free, and they are the ones that catch a variable appearing
    mid-run. The binary's version cannot change under a running process in a
    way this probe would catch anyway.

    Keyed by CLI_PIN so `configure()` switching profiles re-probes rather than
    returning a version verified against a different pin.
    """
    cached = _CLI_VERSION_CACHE.get(CLI_PIN)
    if cached is not None:
        return cached
    installed = subprocess.run([_binary(), "--version"], text=True, capture_output=True,
                               timeout=30).stdout.strip().split()[0]
    if installed != CLI_PIN:
        raise RuntimeError(f"claude CLI mismatch: {installed} != {CLI_PIN}")
    _CLI_VERSION_CACHE[CLI_PIN] = installed
    return installed


def _environment() -> dict:
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(_SCRUBBED_PREFIXES) and key not in _SCRUBBED_NAMES}
    env["CLAUDE_CODE_ENTRYPOINT"] = "text2kgbench-eval"
    env.update(ENV_PINS)
    return env


def _invoke(prompt: str, model: str, timeout: float) -> subprocess.CompletedProcess:
    return subprocess.run([_binary(), *_BASE_ARGS, "--model", model],
                          input=prompt, text=True, capture_output=True,
                          timeout=timeout, env=_environment())


def _transient(payload: dict | None, returncode: int) -> bool:
    if payload is None:
        return returncode != 0
    status = payload.get("api_error_status")
    if isinstance(status, int):
        return status in {408, 409, 429} or status >= 500
    return bool(payload.get("is_error")) and payload.get("subtype") != "success"


def create_message(request: dict, *, sleep=time.sleep) -> dict:
    installed = preflight()
    model = request.get("model") or MODEL
    decoding = request.get("decoding") or {}
    timeout = float(decoding.get("timeout_seconds") or 60)
    delays = (1, 2, 4)
    error: Exception | None = None
    for attempt in range(4):
        payload, returncode = None, -1
        try:
            completed = _invoke(request["prompt"], model, timeout)
            returncode = completed.returncode
            try:
                payload = json.loads(completed.stdout)
            except json.JSONDecodeError:
                payload = None
            if payload is not None and returncode == 0 and not payload.get("is_error"):
                return _row(payload, installed, model)
            error = RuntimeError(
                f"claude -p failed (exit {returncode}, subtype "
                f"{(payload or {}).get('subtype')}, api_error_status "
                f"{(payload or {}).get('api_error_status')}): "
                f"{completed.stderr[-500:] or completed.stdout[-500:]}"
            )
        except subprocess.TimeoutExpired as timeout_error:
            error, payload, returncode = timeout_error, None, -1
        if not _transient(payload, returncode) or attempt == 3:
            raise error
        sleep(delays[attempt])
    raise AssertionError("unreachable")


def _row(payload: dict, installed: str, requested_model: str) -> dict:
    text = payload.get("result") or ""
    usage = payload.get("usage") or {}
    model_usage = payload.get("modelUsage") or {}
    # The model the CLI reports it actually used, not the one we asked for.
    reported = sorted(model_usage) or [requested_model]
    totals = {"input": 0, "output": 0, "cost": 0.0, "thinking": 0}
    for entry in model_usage.values():
        totals["input"] += entry.get("inputTokens", 0) + entry.get("cacheReadInputTokens", 0) \
            + entry.get("cacheCreationInputTokens", 0)
        totals["output"] += entry.get("outputTokens", 0)
        totals["cost"] += entry.get("costUSD", 0.0)
        totals["thinking"] += entry.get("thinkingTokens", 0)
    return {
        "raw_response": text,
        "model": ",".join(reported),
        "provider_request_id": payload.get("session_id", ""),
        "input_tokens": totals["input"],
        "output_tokens": totals["output"],
        "finish_reason": payload.get("stop_reason"),
        "refusal": not bool(text.strip()),
        "sdk_version": f"claude-code=={installed}",
        # Claude Code specific ledger fields; the scorer ignores them, the cost
        # projection needs them.
        "api_input_tokens": usage.get("input_tokens", 0),
        "api_output_tokens": usage.get("output_tokens", 0),
        "thinking_tokens": totals["thinking"],
        "list_cost_usd": totals["cost"],
        "duration_api_ms": payload.get("duration_api_ms"),
        "num_turns": payload.get("num_turns"),
    }
