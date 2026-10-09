import struct

import msgpack
import pytest

from cogfabric.protocol import Envelope, Kind, ProtocolError, MAX_FRAME, decode, encode


def test_roundtrip_and_frame_length():
    event = Envelope(Kind.PLAN, "jev-r", "jev-plus", "task-1", {"score": 0.8, "values": [1, 2, 3]})
    frame = encode(event)
    assert struct.unpack("!I", frame[:4])[0] == len(frame) - 4
    got = decode(frame)
    assert got.payload == event.payload and got.kind == event.kind and got.task_id == event.task_id
    assert len(got.message_id) == 32


def test_rejects_corrupted_or_untrusted_envelopes():
    event = Envelope(Kind.REQUEST, "client", "daemon", "abc", {})
    frame = encode(event)
    with pytest.raises(ProtocolError):
        decode(frame[:-1])
    with pytest.raises(ProtocolError):
        decode(struct.pack("!I", MAX_FRAME + 1))
    with pytest.raises(ProtocolError):
        Envelope(Kind.REQUEST, "../../forged", "daemon", "abc", {})
    raw = msgpack.packb({"v": 999, "id": "x", "k": "request", "s": "a", "t": "b", "task": "c", "p": {}}, use_bin_type=True)
    with pytest.raises(ProtocolError):
        decode(struct.pack("!I", len(raw)) + raw)


def test_rejects_extra_fields():
    raw = msgpack.packb({"v": 1, "id": "x", "k": "request", "s": "a", "t": "b", "task": "c", "p": {}, "admin": True})
    with pytest.raises(ProtocolError):
        decode(struct.pack("!I", len(raw)) + raw)
