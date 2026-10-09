from pathlib import Path

import pytest

from cogfabric.storage import Store
from cogfabric.skills import bootstrap, ingest, route
from cogfabric.security import SecurityKernel, SecurityError


@pytest.fixture()
def store(tmp_path):
    repo = Store(tmp_path / "test.db")
    bootstrap(repo)
    yield repo
    repo.close()


def test_bootstrap_and_route(store):
    assert len(store.skills()) == 7
    assert route(store, "Please debug a Unity shader")[0]["skill"] == "unity.s1"
    assert route(store, "Unrelated question") == []


def test_ingestion_duplicate_prevention_and_search(store, tmp_path):
    path = tmp_path / "readme.md"
    path.write_text("Tokio async mutex best practices. Rust async mutex.", encoding="utf8")
    first = ingest(store, SecurityKernel(), path, "rust.s1", "original", "CC0")
    second = ingest(store, SecurityKernel(), path, "rust.s1", "original", "CC0")
    assert first["added"] and not second["added"]
    assert store.search("async mutex", "rust.s1")[0]["id"] == first["id"]
    assert store.search("bad' OR DROP TABLE skills") == []


def test_symlink_not_ingested(store, tmp_path):
    original = tmp_path / "original.md"
    original.write_text("Hello")
    path = tmp_path / "link.md"
    path.symlink_to(original)
    with pytest.raises(SecurityError):
        ingest(store, SecurityKernel(), path, "rust.s1", "original", "CC0")


def test_dynamic_skill_routing(store):
    store.ensure_skill("rust.async.tokio.s1", "Tokio runtime research", ["tokio", "async"])
    assert route(store, "tokio async runtime")[0]["skill"] == "rust.async.tokio.s1"
