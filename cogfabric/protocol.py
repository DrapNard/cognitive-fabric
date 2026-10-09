"""Versioned MessagePack binary envelopes with length-delimited framing.

Protocol messages are not authorization credentials. The broker assigns sender
identity at the trust boundary. Large objects must be referenced by ID.
"""
from __future__ import annotations

import asyncio
from dataclasses import dataclass
from enum import StrEnum
import struct
from typing import Any
import uuid

import msgpack

PROTOCOL_VERSION = 1
MAX_FRAME = 1024 * 1024


class ProtocolError(ValueError):
    pass


class Kind(StrEnum):
    REQUEST = "request"
    RESPONSE = "response"
    INTENT = "intent"
    PLAN = "plan"
    REASONING = "reasoning"
    FEEDBACK = "feedback"
    SKILL = "skill"
    EVALUATION = "evaluation"
    ERROR = "error"


def _identity(value: str, *, label: str) -> str:
    if not isinstance(value, str) or not value or len(value) > 128:
        raise ProtocolError(f"Invalid {label}")
    if not all(ch.isascii() and (ch.isalnum() or ch in "._:-") for ch in value):
        raise ProtocolError(f"Invalid {label}")
    return value


@dataclass(frozen=True, slots=True)
class Envelope:
    kind: Kind
    source: str
    target: str
    task_id: str
    payload: dict[str, Any]
    message_id: str = ""
    version: int = PROTOCOL_VERSION

    def __post_init__(self) -> None:
        if not isinstance(self.kind, Kind):
            raise ProtocolError("Unknown event kind")
        for key in ("source", "target", "task_id"):
            _identity(getattr(self, key), label=key)
        if not isinstance(self.payload, dict) or any(not isinstance(k, str) for k in self.payload):
            raise ProtocolError("Payload must be a string-keyed map")
        if self.version != PROTOCOL_VERSION:
            raise ProtocolError("Unsupported protocol version")
        if self.message_id:
            _identity(self.message_id, label="message_id")


def encode(event: Envelope) -> bytes:
    data = {
        "v": event.version, "id": event.message_id or uuid.uuid4().hex,
        "k": event.kind.value, "s": event.source, "t": event.target,
        "task": event.task_id, "p": event.payload,
    }
    try:
        body = msgpack.packb(data, use_bin_type=True)
    except (TypeError, ValueError, OverflowError) as exc:
        raise ProtocolError("Unserializable payload") from exc
    if len(body) > MAX_FRAME:
        raise ProtocolError("Frame exceeds limit")
    return struct.pack("!I", len(body)) + body


def decode(frame: bytes) -> Envelope:
    if len(frame) < 4:
        raise ProtocolError("Frame too short")
    (size,) = struct.unpack("!I", frame[:4])
    if size == 0 or size > MAX_FRAME or size != len(frame) - 4:
        raise ProtocolError("Invalid frame length")
    try:
        raw = msgpack.unpackb(frame[4:], raw=False, strict_map_key=True, max_str_len=MAX_FRAME, max_bin_len=MAX_FRAME,
                              max_array_len=10000, max_map_len=10000, max_ext_len=1024)
        if not isinstance(raw, dict) or set(raw) != {"v", "id", "k", "s", "t", "task", "p"}:
            raise ProtocolError("Invalid envelope fields")
        event = Envelope(kind=Kind(raw["k"]), source=raw["s"], target=raw["t"],
                         task_id=raw["task"], payload=raw["p"], message_id=raw["id"],
                         version=raw["v"])
        return event
    except (ValueError, KeyError, TypeError, msgpack.ExtraData, msgpack.FormatError,
            msgpack.StackError) as exc:
        raise ProtocolError("Malformed envelope") from exc


async def receive(reader: asyncio.StreamReader) -> Envelope:
    header = await reader.readexactly(4)
    (size,) = struct.unpack("!I", header)
    if size == 0 or size > MAX_FRAME:
        raise ProtocolError("Invalid frame length")
    return decode(header + await reader.readexactly(size))


async def send(writer: asyncio.StreamWriter, event: Envelope) -> None:
    writer.write(encode(event))
    await writer.drain()
