"""Exercise a fake localhost OpenAI-compatible endpoint, not a real model."""
import asyncio
import json

import pytest

from cogfabric.models import OpenAICompatibleBackend


def test_reject_remote_endpoint():
    with pytest.raises(ValueError):
        OpenAICompatibleBackend("https://example.com/v1", "unsafe", 1)
    with pytest.raises(ValueError):
        OpenAICompatibleBackend("http://example.com/v1", "unsafe", 1)


def test_local_http_contract():
    async def scenario():
        received = []

        async def handle(reader, writer):
            first_line = await reader.readline()
            headers = {}
            while True:
                line = await reader.readline()
                if line == b"\r\n":
                    break
                key, value = line.decode().split(":", 1)
                headers[key.lower()] = value.strip()
            content_len = int(headers["content-length"])
            body = json.loads(await reader.readexactly(content_len))
            received.append((first_line, body))
            payload = json.dumps({"choices": [{"message": {"content": "Verified mock response"}}],
                                  "usage": {"total_tokens": 12}}).encode()
            writer.write(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n"
                         + f"Content-Length: {len(payload)}\r\nConnection: close\r\n\r\n".encode() + payload)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        server = await asyncio.start_server(handle, host="127.0.0.1", port=0)
        try:
            port = server.sockets[0].getsockname()[1]
            async with server:
                adapter = OpenAICompatibleBackend(f"http://127.0.0.1:{port}/v1", "mock-llm", 3)
                answer = await adapter.generate("solver", [{"role": "user", "content": "Solve this"}], 64)
                assert answer == "Verified mock response"
                assert adapter.calls == 1 and adapter.estimated_tokens == 12
                assert received[0][0].startswith(b"POST /v1/chat/completions")
                assert received[0][1]["model"] == "mock-llm"
        finally:
            server.close()
            await server.wait_closed()

    asyncio.run(scenario())
