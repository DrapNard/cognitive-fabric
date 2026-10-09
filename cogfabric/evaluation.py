"""Reproducible functional benchmarks and limited artifact integrity checks."""
from __future__ import annotations

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


async def quality_check(store: Store, candidate_id: str, dataset: Path) -> dict:
    """Check retrieval of *this candidate*, not unrelated arithmetic capabilities.

    The held-out evaluation file uses {"query":..., "expected_excerpt":...}.
    This only evaluates corpus matching, NOT neural reasoning or behavioral safety.
    """
    candidate = store.candidate(candidate_id)
    if candidate is None:
        raise ValueError("Unknown candidate")
    if candidate["kind"] != "rag":
        raise ValueError("LoRA quality evaluation needs a separate neural model evaluator")
    path = Path(candidate["artifact"])
    if not path.is_file() or path.is_symlink() or sha256_file(path) != candidate["checksum"]:
        raise ValueError("Artifact failed integrity requirements")
    content = path.read_text("utf-8")
    if dataset.is_symlink() or not dataset.is_file() or dataset.stat().st_size > MAX_DATASET_BYTES:
        raise ValueError("Invalid held-out retrieval evaluation dataset")
    cases = []
    for line in dataset.read_text("utf-8").splitlines():
        if not line.strip():
            continue
        item = json.loads(line)
        if (not isinstance(item, dict) or not isinstance(item.get("query"), str)
                or not isinstance(item.get("expected_excerpt"), str)
                or not item["query"].strip() or not item["expected_excerpt"].strip()
                or len(item["query"]) > 500 or len(item["expected_excerpt"]) > 200):
            raise ValueError("Invalid query/expected_excerpt in evaluation dataset")
        cases.append(item)
    if not 3 <= len(cases) <= 1000:
        raise ValueError("Retrieval evaluation requires 3-1000 cases")
    # FTS5 candidate-only isolated corpus, not the production knowledge index.
    import tempfile
    with tempfile.TemporaryDirectory() as temp_dir:
        staging = Store(Path(temp_dir) / "evaluation.db")
        try:
            staging.ensure_skill(candidate["skill_id"], "candidate", ["test"])
            staging.add_document({"id": "candidate", "skill_id": candidate["skill_id"],
                                  "title": path.name, "body": content,
                                  "source": candidate["source"], "license": candidate["license"],
                                  "sha256": candidate["checksum"]})
            details = []
            for item in cases:
                hits = staging.search(item["query"], candidate["skill_id"], limit=1)
                ok = bool(hits) and item["expected_excerpt"].casefold() in hits[0]["body"].casefold()
                details.append({"query": item["query"], "passed": ok})
        finally:
            staging.close()
    score = sum(int(d["passed"]) for d in details) / len(details)
    passed = score >= 0.9
    report = {"total": len(details), "passed": sum(int(d["passed"]) for d in details),
              "accuracy": score, "gate_passed": passed, "details": details,
              "warning": "This checks retrieval only, not model quality or security."}
    store.record_evaluation(candidate_id, "quality", passed, score,
                            {"dataset": str(dataset), "total": len(details), "accuracy": score})
    return report
