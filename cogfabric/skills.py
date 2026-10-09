"""Minimal, deterministic skill selection and trusted-operator ingestion."""
from __future__ import annotations

from hashlib import sha256
from pathlib import Path
import re
import uuid

from .security import SecurityKernel
from .storage import Store

BUILTINS = {
    "rust.s1": ("Rust language and tooling", ["rust", "cargo", "tokio", "borrow", "lifetime"]),
    "linux.s1": ("Linux system and administration", ["linux", "arch", "nixos", "bash", "kernel"]),
    "security.s1": ("Defensive software security", ["security", "audit", "vulnerability", "secure"]),
    "reverse.s1": ("Binary format research", ["binary", "reverse", "disassembly", "metadata"]),
    "data.sql.s1": ("SQL and databases", ["sql", "sqlite", "database", "query"]),
    "git.s1": ("Version control", ["git", "commit", "merge", "repository"]),
    "unity.s1": ("Unity and C#", ["unity", "shader", "gameobject", "csharp", "urp"]),
}


def bootstrap(store: Store) -> None:
    for name, (desc, tags) in BUILTINS.items():
        store.ensure_skill(name, desc, tags)


def route(store: Store, query: str, limit: int = 2) -> list[dict]:
    words = set(re.findall(r"[a-zA-Z0-9_]+", query.lower()))
    scores = []
    for skill in store.skills():
        score = sum(3 for tag in skill["tags"] if tag in words)
        score += sum(1 for term in skill["id"].replace(".", " ").split() if term in words)
        if score:
            scores.append((score, skill["id"]))
    scores.sort(key=lambda s: (-s[0], s[1]))
    return [{"skill": name, "score": score} for score, name in scores[:limit]]


def ingest(store: Store, security: SecurityKernel, path: Path, skill_id: str,
           source: str, license_name: str) -> dict:
    path = security.check_ingest_file(path)
    if not source.strip() or not license_name.strip():
        raise ValueError("Source and license are required")
    if not store.conn.execute("SELECT 1 FROM skills WHERE id=?", (skill_id,)).fetchone():
        raise ValueError("Unknown skill")
    content = path.read_text(encoding="utf-8")
    digest = sha256(content.encode("utf-8")).hexdigest()
    doc_id = uuid.uuid4().hex
    added = store.add_document({"id": doc_id, "skill_id": skill_id, "title": path.name,
                                "body": content, "source": source, "license": license_name,
                                "sha256": digest})
    return {"id": doc_id if added else None, "added": added, "hash": digest}
