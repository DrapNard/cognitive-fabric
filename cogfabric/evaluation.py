"""Reproducible functional benchmarks and limited artifact integrity checks."""
from __future__ import annotations

import asyncio
from hashlib import sha256
import json
from pathlib import Path
from time import perf_counter

from .engine import CognitiveEngine
from .storage import Store

MAX_DATASET_BYTES = 2_000_000


def read_cases(path: Path) -> list[dict]:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_DATASET_BYTES:
        raise ValueError("Invalid benchmark file")
    cases = []
    for number, line in enumerate(path.read_text("utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        try:
            item = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"Invalid JSON on line {number}") from exc
        if not isinstance(item, dict) or not isinstance(item.get("input"), str) or not isinstance(item.get("expected"), str):
            raise ValueError(f"Invalid input/expected on line {number}")
        if len(item["input"]) > 12000 or len(item["expected"]) > 16000:
            raise ValueError("Benchmark case too long")
        cases.append(item)
    if not cases or len(cases) > 1000:
        raise ValueError("Benchmark requires 1–1000 cases")
    return cases


async def benchmark(engine: CognitiveEngine, cases: list[dict], mode: str) -> dict:
    details = []
    started = perf_counter()
    for case in cases:
        result = await engine.run(case["input"], mode)
        expected = " ".join(case["expected"].lower().split())
        actual = " ".join(result.answer.lower().split())
        success = result.status == "completed" and expected == actual
        details.append({"input": case["input"], "expected": case["expected"],
                        "actual": result.answer, "passed": success,
                        "latency_ms": result.elapsed_ms, "model_calls": result.calls,
                        "task_id": result.task_id})
    return {"total": len(cases), "passed": sum(int(r["passed"]) for r in details),
            "accuracy": sum(int(r["passed"]) for r in details) / len(cases),
            "elapsed_ms": round((perf_counter() - started)*1000, 3),
            "model_calls": sum(r["model_calls"] for r in details), "details": details}


def sha256_file(path: Path) -> str:
    digest = sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def integrity_check(store: Store, candidate_id: str) -> dict:
    """Checks artifact existence and checksum, NOT model behavioral safety."""
    candidate = store.candidate(candidate_id)
    if candidate is None:
        raise ValueError("Unknown candidate")
    path = Path(candidate["artifact"])
    ok = path.is_file() and not path.is_symlink() and sha256_file(path) == candidate["checksum"]
    report = {"artifact": str(path), "checksum_matches": ok,
              "warning": "Integrity is not a behavioral or cybersecurity audit."}
    store.record_evaluation(candidate_id, "integrity", ok, float(ok), report)
    return report


async def quality_check(store: Store, engine: CognitiveEngine, candidate_id: str,
                        dataset: Path) -> dict:
    if store.candidate(candidate_id) is None:
        raise ValueError("Unknown candidate")
    report = await benchmark(engine, read_cases(dataset), "fast")
    # Requirement is configurable at code review, never adjustable by a model.
    passed = report["total"] >= 3 and report["accuracy"] >= 0.9
    store.record_evaluation(candidate_id, "quality", passed, report["accuracy"],
                            {"dataset": str(dataset), "total": report["total"],
                             "accuracy": report["accuracy"]})
    return report | {"gate_passed": passed}
