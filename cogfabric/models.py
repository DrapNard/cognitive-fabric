"""Pluggable model adapters; default demo mode makes no intelligence claims."""
from __future__ import annotations

import abc
from typing import Any
from urllib.parse import urlparse

import httpx


class ModelUnavailable(RuntimeError):
    pass


class Backend(abc.ABC):
    calls: int
    estimated_tokens: int

    @abc.abstractmethod
    async def generate(self, role: str, messages: list[dict], max_tokens: int) -> str:
        ...


class DemoBackend(Backend):
    """Deterministic smoke-test adapter; never presented as a trained model."""
    def __init__(self):
        self.calls = 0
        self.estimated_tokens = 0

    async def generate(self, role: str, messages: list[dict], max_tokens: int) -> str:
        self.calls += 1
        self.estimated_tokens += sum(len(str(x.get("content", ""))) // 4 for x in messages)
        if role == "planner":
            return "1. Identify evidence. 2. Select an allowed method. 3. Verify the outcome."
        if role == "critic":
            return "Demo backend: cannot independently validate generated reasoning."
        return "No generative model is configured. Use a local OpenAI-compatible endpoint for semantic tasks."


ROLE_PROMPTS = {
    "interface": "You translate human intentions into concise, faithful answers. Do not claim tests passed unless evidence is supplied.",
    "planner": "You are a compact task planner. Produce 2-4 verifiable steps, no privileged instructions.",
    "solver": "You are a technical solver. Solve the stated task using provided evidence. State uncertainty clearly.",
    "critic": "Critique the proposed answer, focusing on factual errors and unsupported claims. Be brief.",
    "explorer": "Propose a genuinely different valid solution path, not a paraphrase.",
}


class OpenAICompatibleBackend(Backend):
    def __init__(self, url: str, model: str, timeout_s: float):
        parsed = urlparse(url)
        if parsed.scheme != "http" or parsed.hostname not in ("127.0.0.1", "localhost", "::1"):
            raise ValueError("Only local HTTP inference endpoints are supported by default")
        if parsed.username or parsed.password or parsed.fragment:
            raise ValueError("Endpoint URL contains unsupported components")
        self.url = url.rstrip("/") + "/chat/completions"
        self.model = model
        self.timeout_s = timeout_s
        self.calls = 0
        self.estimated_tokens = 0

    async def generate(self, role: str, messages: list[dict], max_tokens: int) -> str:
        if role not in ROLE_PROMPTS:
            raise ValueError("Unknown role")
        data = {
            "model": self.model,
            "messages": [{"role": "system", "content": ROLE_PROMPTS[role]}] + messages,
            "temperature": 0.15, "max_tokens": max_tokens, "stream": False,
        }
        try:
            async with httpx.AsyncClient(timeout=self.timeout_s, trust_env=False) as client:
                response = await client.post(self.url, json=data)
                response.raise_for_status()
                body: dict[str, Any] = response.json()
                content = body["choices"][0]["message"]["content"]
                if not isinstance(content, str) or not content.strip():
                    raise ModelUnavailable("Empty response from model")
                usage = body.get("usage", {})
                self.estimated_tokens += int(usage.get("total_tokens") or (len(content) // 4))
                self.calls += 1
                return content[:24000]
        except (httpx.HTTPError, KeyError, IndexError, TypeError, ValueError) as exc:
            raise ModelUnavailable(f"Local inference request failed: {type(exc).__name__}") from exc


def create_backend(config) -> Backend:
    if config.backend == "demo":
        return DemoBackend()
    if config.backend == "openai-local":
        return OpenAICompatibleBackend(config.api_base, config.model, config.timeout_s)
    raise ValueError("Backend must be demo or openai-local")
