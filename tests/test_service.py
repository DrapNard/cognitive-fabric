import asyncio
import stat
import uuid

from cogfabric.config import Config
from cogfabric.protocol import Envelope, Kind, receive, send
from cogfabric.service import FabricService, request


def test_local_daemon_request_and_denial(tmp_path):
    async def check():
        cfg = Config.load(tmp_path)
        service = FabricService(cfg)
        server = await asyncio.start_unix_server(service.handle, path=str(cfg.socket_path))
        cfg.socket_path.chmod(0o600)
        try:
            assert stat.S_IMODE(cfg.socket_path.stat().st_mode) == 0o600
            async with server:
                result = await request(cfg, "ask", query="calc: 9*9", mode="fast")
                assert result["answer"] == "81"
                reader, writer = await asyncio.open_unix_connection(str(cfg.socket_path))
                await send(writer, Envelope(Kind.REQUEST, "jev-r", "daemon", uuid.uuid4().hex,
                                            {"operation": "candidate.promote"}))
                answer = await receive(reader)
                assert answer.kind is Kind.ERROR
                writer.close()
                await writer.wait_closed()
        finally:
            cfg.socket_path.unlink(missing_ok=True)
            service.store.close()
    asyncio.run(check())
