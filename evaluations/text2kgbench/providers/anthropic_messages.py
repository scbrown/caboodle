"""Pinned Anthropic Messages adapter for Text2KGBench L1."""

from __future__ import annotations

import importlib.metadata
import os
import time

MODEL = "claude-haiku-4-5-20251001"
SDK_VERSION = "0.120.2"


def preflight() -> str:
    if not os.environ.get("ANTHROPIC_API_KEY"):
        raise RuntimeError("ANTHROPIC_API_KEY is required")
    if os.environ.get("ANTHROPIC_BASE_URL") or os.environ.get("HTTPS_PROXY") or os.environ.get("HTTP_PROXY"):
        raise RuntimeError("custom Anthropic endpoints and proxies are forbidden")
    installed = importlib.metadata.version("anthropic")
    if installed != SDK_VERSION:
        raise RuntimeError(f"anthropic SDK mismatch: {installed} != {SDK_VERSION}")
    return installed


def create_message(request: dict, *, sleep=time.sleep) -> dict:
    installed = preflight()
    import anthropic
    client = anthropic.Anthropic(api_key=os.environ["ANTHROPIC_API_KEY"])
    delays = (1, 2, 4)
    for attempt in range(4):
        try:
            response = client.messages.create(
                model=MODEL, temperature=0, top_p=1, max_tokens=512,
                timeout=60, messages=[{"role": "user", "content": request["prompt"]}],
            )
            text = "".join(block.text for block in response.content if getattr(block, "type", None) == "text")
            return {"raw_response": text, "model": response.model, "provider_request_id": response.id,
                    "input_tokens": response.usage.input_tokens, "output_tokens": response.usage.output_tokens,
                    "finish_reason": response.stop_reason, "refusal": not bool(text),
                    "sdk_version": installed}
        except (anthropic.APITimeoutError, anthropic.APIConnectionError) as error:
            transient = True
        except anthropic.APIStatusError as error:
            transient = error.status_code in {408, 409, 429} or error.status_code >= 500
        if not transient or attempt == 3:
            raise error
        sleep(delays[attempt])
    raise AssertionError("unreachable")
