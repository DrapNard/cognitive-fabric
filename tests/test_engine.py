import asyncio
from dataclasses import replace

from cogfabric.config import Config
from cogfabric.engine import CognitiveEngine
from cogfabric.models import Backend
from cogfabric.skills import bootstrap
from cogfabric.storage import Store


class RecordingBackend(Backend):
    def __init__(self):
        self.roles = []
        self.calls = 0
        self.estimated_tokens = 0

    async def generate(self, role, messages, max_tokens):
        self.roles.append((role, max_tokens))
        self.calls += 1
        return {"planner": "plan", "solver": "solution", "critic": "review", "explorer": "alternate"}[role]


def make_engine(tmp_path, backend=None, **overrides):
    cfg = replace(Config.load(tmp_path), **overrides)
    store = Store(cfg.db_path)
    bootstrap(store)
    return CognitiveEngine(cfg, store, backend), store


def test_verified_calculator_has_no_model_calls(tmp_path):
    engine, store = make_engine(tmp_path)
    result = asyncio.run(engine.run("calc: (7+5)*3"))
    assert result.answer == "36" and result.calls == 0
    assert "verified:arithmetic_ast" in result.evidence
    assert store.task(result.task_id)["status"] == "completed"
    assert store.events(result.task_id)[-1]["kind"] == "outcome"
    store.close()


def test_cooperative_roles_and_budgets(tmp_path):
    backend = RecordingBackend()
    engine, store = make_engine(tmp_path, backend, max_calls=3, max_tokens=800, max_cycles=2)
    result = asyncio.run(engine.run("Explain a Rust mutex", "research"))
    assert result.calls == 2  # Token cap stops the remaining roles.
    assert len(backend.roles) == 2
    assert sum(item[1] for item in backend.roles) <= 800
    assert "solution" in result.answer
    assert result.skills == ["rust.s1"]
    store.close()


def test_demo_is_not_reported_as_real_completion(tmp_path):
    engine, store = make_engine(tmp_path)
    result = asyncio.run(engine.run("Give me a complete proof", "deep"))
    assert result.status == "demo"
    assert "No generative model is configured" in result.answer
    store.close()


def test_failed_model_is_persisted(tmp_path):
    class FailingBackend(RecordingBackend):
        async def generate(self, *args, **kwargs):
            raise RuntimeError("unavailable")
    engine, store = make_engine(tmp_path, FailingBackend())
    result = asyncio.run(engine.run("Explain Rust"))
    assert result.status == "failed"
    assert store.task(result.task_id)["status"] == "failed"
    store.close()
