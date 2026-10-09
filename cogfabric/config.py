"""Small typed configuration surface; no implicit network or shell tools."""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import os


@dataclass(frozen=True)
class Config:
    data_dir: Path
    backend: str = "demo"
    model: str = "local-model"
    api_base: str = "http://127.0.0.1:1234/v1"
    timeout_s: float = 45.0
    max_calls: int = 6
    max_cycles: int = 3
    max_tokens: int = 1600
    max_query_chars: int = 12000

    @classmethod
    def load(cls, data_dir: str | Path | None = None, backend: str | None = None) -> "Config":
        raw = data_dir or os.getenv("COGFABRIC_HOME") or (Path.home() / ".local" / "share" / "cogfabric")
        path = Path(raw).expanduser().resolve()
        return cls(data_dir=path, backend=backend or os.getenv("COGFABRIC_BACKEND", "demo"),
                   model=os.getenv("COGFABRIC_MODEL", "local-model"),
                   api_base=os.getenv("COGFABRIC_API_BASE", "http://127.0.0.1:1234/v1"))

    @property
    def db_path(self) -> Path:
        return self.data_dir / "fabric.sqlite3"

    @property
    def socket_path(self) -> Path:
        return self.data_dir / "fabric.sock"

    def prepare(self) -> None:
        self.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.data_dir.chmod(0o700)
