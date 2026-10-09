"""Atomic SQLite repository with append-only event and evaluation records."""
from __future__ import annotations

import json
from hashlib import sha256
from pathlib import Path
import sqlite3
from time import time
from typing import Any
import uuid

SCHEMA = """
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY, query TEXT NOT NULL, mode TEXT NOT NULL,
  status TEXT NOT NULL, answer TEXT, created REAL NOT NULL, updated REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL,
  kind TEXT NOT NULL, source TEXT NOT NULL, payload_json TEXT NOT NULL, created REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_events_task ON events(task_id, id);
CREATE TABLE IF NOT EXISTS skills (
  id TEXT PRIMARY KEY, description TEXT NOT NULL, tags_json TEXT NOT NULL,
  active_version INTEGER NOT NULL DEFAULT 0, created REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, title TEXT NOT NULL,
  body TEXT NOT NULL, source TEXT NOT NULL, license TEXT NOT NULL,
  sha256 TEXT NOT NULL UNIQUE, created REAL NOT NULL,
  FOREIGN KEY(skill_id) REFERENCES skills(id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS doc_index USING fts5(title, body, content='documents', content_rowid='rowid');
CREATE TRIGGER IF NOT EXISTS documents_ai AFTER INSERT ON documents BEGIN
 INSERT INTO doc_index(rowid, title, body) VALUES(new.rowid, new.title, new.body);
END;
CREATE TABLE IF NOT EXISTS candidates (
 id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, kind TEXT NOT NULL,
 version INTEGER NOT NULL, artifact TEXT NOT NULL, checksum TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('candidate','rejected','approved')),
 created REAL NOT NULL, approved_at REAL,
 source TEXT NOT NULL DEFAULT 'unknown', license TEXT NOT NULL DEFAULT 'unknown',
 UNIQUE(skill_id, version),
 FOREIGN KEY(skill_id) REFERENCES skills(id)
);
CREATE TABLE IF NOT EXISTS evaluations (
 id INTEGER PRIMARY KEY AUTOINCREMENT, candidate_id TEXT NOT NULL,
 suite TEXT NOT NULL, success INTEGER NOT NULL, score REAL NOT NULL,
 details TEXT NOT NULL, created REAL NOT NULL,
 FOREIGN KEY(candidate_id) REFERENCES candidates(id)
);
CREATE TABLE IF NOT EXISTS model_registry (
 id TEXT PRIMARY KEY, backend TEXT NOT NULL, model_path TEXT NOT NULL,
 checksum TEXT, capabilities_json TEXT NOT NULL, active INTEGER NOT NULL DEFAULT 0
);
"""


def dumps(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


class Store:
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self.conn = sqlite3.connect(str(path), timeout=10, isolation_level=None)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA foreign_keys=ON")
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.executescript(SCHEMA)
        # Upgrade earlier local development stores safely and idempotently.
        existing = {r["name"] for r in self.conn.execute("PRAGMA table_info(candidates)")}
        for name in ("source", "license"):
            if name not in existing:
                self.conn.execute(f"ALTER TABLE candidates ADD COLUMN {name} TEXT NOT NULL DEFAULT 'unknown'")

    def close(self) -> None:
        self.conn.close()

    def task_create(self, query: str, mode: str) -> str:
        task_id = uuid.uuid4().hex
        now = time()
        self.conn.execute("INSERT INTO tasks VALUES(?,?,?,?,?,?,?)",
                          (task_id, query, mode, "running", None, now, now))
        return task_id

    def task_finish(self, task_id: str, status: str, answer: str) -> None:
        self.conn.execute("UPDATE tasks SET status=?, answer=?, updated=? WHERE id=?",
                          (status, answer, time(), task_id))

    def event(self, task_id: str, kind: str, source: str, payload: dict) -> None:
        self.conn.execute("INSERT INTO events(task_id,kind,source,payload_json,created) VALUES(?,?,?,?,?)",
                          (task_id, kind, source, dumps(payload), time()))

    def events(self, task_id: str) -> list[dict]:
        rows = self.conn.execute("SELECT * FROM events WHERE task_id=? ORDER BY id", (task_id,))
        return [dict(row) | {"payload": json.loads(row["payload_json"])} for row in rows]

    def task(self, task_id: str) -> dict | None:
        row = self.conn.execute("SELECT * FROM tasks WHERE id=?", (task_id,)).fetchone()
        return dict(row) if row else None

    def ensure_skill(self, skill_id: str, description: str, tags: list[str]) -> None:
        self.conn.execute("INSERT OR IGNORE INTO skills(id,description,tags_json,created) VALUES(?,?,?,?)",
                          (skill_id, description, dumps(tags), time()))

    def skills(self) -> list[dict]:
        return [dict(row) | {"tags": json.loads(row["tags_json"])} for row in self.conn.execute("SELECT * FROM skills ORDER BY id")]

    def add_document(self, doc: dict) -> bool:
        self.conn.execute("INSERT OR IGNORE INTO documents VALUES(?,?,?,?,?,?,?,?)",
                          (doc["id"], doc["skill_id"], doc["title"], doc["body"],
                           doc["source"], doc["license"], doc["sha256"], time()))
        return bool(self.conn.execute("SELECT changes()").fetchone()[0])

    def search(self, query: str, skill_id: str | None = None, limit: int = 4) -> list[dict]:
        # The FTS query is never interpolated into SQL, and bounded to lexical tokens.
        import re
        words = re.findall(r"[a-zA-Z0-9_]{2,40}", query)[:8]
        if not words:
            return []
        match = " OR ".join('"' + w + '"' for w in words)
        args: list = [match]
        q = ("SELECT d.id,d.skill_id,d.title,d.body,d.source,d.license "
             "FROM doc_index x JOIN documents d ON d.rowid=x.rowid WHERE doc_index MATCH ?")
        if skill_id:
            q += " AND d.skill_id=?"
            args.append(skill_id)
        q += " ORDER BY bm25(doc_index) LIMIT ?"
        args.append(min(20, max(1, limit)))
        return [dict(row) for row in self.conn.execute(q, args)]

    def candidate(self, id_: str) -> dict | None:
        row = self.conn.execute("SELECT * FROM candidates WHERE id=?", (id_,)).fetchone()
        return dict(row) if row else None

    def create_candidate(self, skill_id: str, kind: str, artifact: str, checksum: str,
                         source: str = "unknown", license_name: str = "unknown") -> dict:
        if kind not in ("rag", "lora"):
            raise ValueError("Candidate kind must be rag or lora")
        if not self.conn.execute("SELECT 1 FROM skills WHERE id=?", (skill_id,)).fetchone():
            raise ValueError("Unknown skill")
        version = self.conn.execute("SELECT COALESCE(MAX(version),0)+1 FROM candidates WHERE skill_id=?", (skill_id,)).fetchone()[0]
        id_ = uuid.uuid4().hex
        if (not source.strip() or not license_name.strip() or
                source == "unknown" or license_name == "unknown"):
            raise ValueError("Explicit candidate provenance and license required")
        if kind == "rag":
            candidate_path = Path(artifact)
            if (candidate_path.is_symlink() or not candidate_path.is_file()
                    or candidate_path.suffix.lower() not in (".md", ".txt")
                    or candidate_path.stat().st_size > 256_000):
                raise ValueError("RAG candidate requires a regular .md/.txt file <=256 KB")
        self.conn.execute("INSERT INTO candidates(id,skill_id,kind,version,artifact,checksum,status,created,source,license) VALUES(?,?,?,?,?,?,?,?,?,?)",
                          (id_, skill_id, kind, version, artifact, checksum, "candidate", time(), source, license_name))
        return self.candidate(id_)

    def record_evaluation(self, id_: str, suite: str, success: bool, score: float, details: dict) -> None:
        if not self.candidate(id_):
            raise ValueError("Unknown candidate")
        self.conn.execute("INSERT INTO evaluations(candidate_id,suite,success,score,details,created) VALUES(?,?,?,?,?,?)",
                          (id_, suite, int(success), score, dumps(details), time()))

    def evaluations(self, id_: str) -> list[dict]:
        return [dict(r) for r in self.conn.execute("SELECT * FROM evaluations WHERE candidate_id=? ORDER BY id", (id_,))]

    def promote(self, id_: str) -> dict:
        candidate = self.candidate(id_)
        if not candidate or candidate["status"] != "candidate":
            raise ValueError("Candidate not eligible for promotion")
        evs = self.evaluations(id_)
        latest = {e["suite"]: e for e in evs}
        suites = {suite for suite, e in latest.items() if e["success"] and e["score"] >= 0.9}
        if not {"quality", "integrity"}.issubset(suites):
            raise PermissionError("Both quality and integrity evaluation gates must pass")
        # A neural model cannot be evaluated by the RAG retrieval-quality suite.
        if candidate["kind"] != "rag":
            raise PermissionError("LoRA promotion is disabled until model-specific behavioral evaluation exists")
        path = Path(candidate["artifact"])
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 256_000:
            raise PermissionError("Candidate artifact is missing or invalid")
        body = path.read_text("utf-8")
        digest = sha256(body.encode("utf-8")).hexdigest()
        if digest != candidate["checksum"]:
            raise PermissionError("Artifact changed since registration")
        self.conn.execute("BEGIN IMMEDIATE")
        try:
            # RAG content becomes searchable *only after* explicit promotion.
            self.add_document({"id": uuid.uuid4().hex, "skill_id": candidate["skill_id"],
                               "title": path.name, "body": body,
                               "source": candidate["source"], "license": candidate["license"],
                               "sha256": digest})
            self.conn.execute("UPDATE candidates SET status='approved', approved_at=? WHERE id=? AND status='candidate'", (time(), id_))
            self.conn.execute("UPDATE skills SET active_version=? WHERE id=?", (candidate["version"], candidate["skill_id"]))
            self.conn.execute("COMMIT")
        except Exception:
            self.conn.execute("ROLLBACK")
            raise
        return self.candidate(id_)

    def candidates(self) -> list[dict]:
        return [dict(r) for r in self.conn.execute("SELECT * FROM candidates ORDER BY created DESC")]
