"""Single-user Unix-socket server. Per-request source identity is broker-owned."""
from __future__ import annotations

import asyncio
import os
from pathlib import Path
import stat
import uuid

from .config import Config
from .engine import CognitiveEngine
from .protocol import Envelope, Kind, receive, send, ProtocolError
from .skills import bootstrap
from .storage import Store


class FabricService:
    def __init__(self, config: Config):
        self.config = config
        self.config.prepare()
        self.store = Store(config.db_path)
        bootstrap(self.store)
        self.engine = CognitiveEngine(config, self.store)
        self.lock = asyncio.Lock()

    async def handle(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        try:
            event = await asyncio.wait_for(receive(reader), timeout=10)
            if event.kind is not Kind.REQUEST or event.target != "daemon":
                raise ProtocolError("Only request messages to daemon accepted")
            operation = event.payload.get("operation")
            if operation == "ask":
                query = event.payload.get("query")
                mode = event.payload.get("mode", "standard")
                async with self.lock:
                    # Untrusted client may not impersonate internal agents; source is ignored.
                    response = (await self.engine.run(query, mode)).as_dict()
            elif operation == "status":
                response = {"version": 1, "skills": len(self.store.skills()),
                            "backend": self.config.backend}
            else:
                raise ProtocolError("Operation not allowed")
            reply = Envelope(Kind.RESPONSE, "daemon", "client", event.task_id, response)
        except (ProtocolError, ValueError, TimeoutError, asyncio.IncompleteReadError) as exc:
            reply = Envelope(Kind.ERROR, "daemon", "client", "error", {"error": str(exc)[:300]})
        except Exception as exc:
            reply = Envelope(Kind.ERROR, "daemon", "client", "error",
                             {"error": f"Internal failure: {type(exc).__name__}"})
        try:
            await send(writer, reply)
        finally:
            writer.close()
            await writer.wait_closed()

    async def serve(self) -> None:
        socket = self.config.socket_path
        if socket.exists():
            # Never unlink a random file at the configured socket path.
            if not stat.S_ISSOCK(socket.lstat().st_mode):
                raise RuntimeError("Socket path occupied by non-socket")
            raise RuntimeError("Socket exists; another daemon may already be running")
        server = await asyncio.start_unix_server(self.handle, path=str(socket))
        socket.chmod(0o600)
        try:
            async with server:
                await server.serve_forever()
        finally:
            socket.unlink(missing_ok=True)
            self.store.close()


async def request(config: Config, operation: str, **payload) -> dict:
    reader, writer = await asyncio.open_unix_connection(str(config.socket_path))
    try:
        event = Envelope(Kind.REQUEST, "client", "daemon", uuid.uuid4().hex,
                         {"operation": operation, **payload})
        await send(writer, event)
        answer = await asyncio.wait_for(receive(reader), timeout=180)
        if answer.kind is Kind.ERROR:
            raise RuntimeError(answer.payload.get("error", "Unknown server error"))
        return answer.payload
    finally:
        writer.close()
        await writer.wait_closed()
