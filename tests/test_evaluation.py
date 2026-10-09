import asyncio
from hashlib import sha256

import pytest

from cogfabric.evaluation import integrity_check, quality_check, read_cases
from cogfabric.skills import bootstrap
from cogfabric.storage import Store


def test_evaluation_gates_and_manual_promotion(tmp_path):
    store = Store(tmp_path / "test.db")
    bootstrap(store)
    artifact = tmp_path / "adapter.md"
    artifact.write_text("Tokio async mutex guards require careful handling across await boundaries.")
    cand = store.create_candidate("rust.s1", "rag", str(artifact), sha256(artifact.read_bytes()).hexdigest(), "original-test", "CC0-1.0")
    with pytest.raises(PermissionError):
        store.promote(cand["id"])
    report = integrity_check(store, cand["id"])
    assert report["checksum_matches"]
    cases = tmp_path / "bench.jsonl"
    cases.write_text('''{"query":"Tokio async mutex", "expected_excerpt":"mutex guards"}
{"query":"careful handling await", "expected_excerpt":"across await boundaries"}
{"query":"Tokio guards", "expected_excerpt":"require careful handling"}
''')
    quality = asyncio.run(quality_check(store, cand["id"], cases))
    assert quality["gate_passed"]
    artifact.write_text("tampered")
    with pytest.raises(PermissionError):
        # The most recent integrity gate can invalidate the previously passing gate.
        integrity_check(store, cand["id"])
        store.promote(cand["id"])
    artifact.write_text("Tokio async mutex guards require careful handling across await boundaries.")
    assert integrity_check(store, cand["id"])["checksum_matches"]
    assert store.search("Tokio") == []  # Not activated before promotion.
    assert store.promote(cand["id"])["status"] == "approved"
    assert store.search("Tokio")[0]["skill_id"] == "rust.s1"
    assert next(s for s in store.skills() if s["id"] == "rust.s1")["active_version"] == 1
    store.close()


def test_invalid_benchmark_rejected(tmp_path):
    file = tmp_path / "x.jsonl"
    file.write_text('{"input":"hi"}')
    with pytest.raises(ValueError):
        read_cases(file)


def test_lora_promotion_remains_disabled(tmp_path):
    store = Store(tmp_path / "test.db")
    bootstrap(store)
    adapter = tmp_path / "adapter.safetensors"
    adapter.write_bytes(b"placeholder")
    candidate = store.create_candidate("rust.s1", "lora", str(adapter),
                                       sha256(adapter.read_bytes()).hexdigest(), "original-test", "CC0-1.0")
    assert integrity_check(store, candidate["id"])["checksum_matches"]
    store.record_evaluation(candidate["id"], "quality", True, 1.0, {"synthetic": True})
    with pytest.raises(PermissionError, match="LoRA promotion is disabled"):
        store.promote(candidate["id"])
    store.close()
