# Local protocol and service lifecycle

`protocol/cognitive.fbs` defines protocol v1 with file identifier COGF.
Each message is prefixed by a four-byte big-endian length, bounded to 256 KiB.
The receiver verifies the FlatBuffer before accessing fields and rejects unknown
versions, unknown message/mode enums, missing required fields and mixed payloads.
Ask, Reply and Error envelopes carry request correlation IDs. Task IDs identify
persistent execution independently. The generator and Rust runtime use 25.12.19.

The single-user service accepts one in-flight request, with no application queue.
Excess connections are closed. Peer UID from OS credentials must match the
socket's owner. Owner-only directory/socket permissions are required; same-user
malicious processes are outside this isolation boundary. Remote IPC is unsupported.
Message-supplied identities are never used to authorize the connection.

Use `serve`, then `ask 'calc: 6 * 7' --mode fast --socket` in another terminal.
One connection carries exactly one request and response. The client must keep
its write side open while waiting for the result. Disconnecting, half-closing
or sending extra bytes cancels in-flight execution. Ctrl-C on a socket client
drops the connection. Direct CLI execution also handles Ctrl-C and persists
cancellation. Failed, rejected, cancelled and exhausted direct task outcomes
produce a nonzero CLI exit code after printing their result.

Ctrl-C on the service stops accepts, signals cancellation, joins handlers and
removes its own socket. Incomplete frames are limited to ten seconds; responses
are limited to two seconds. Shutdown interrupts incomplete reads immediately.
A device/inode guard prevents cleanup from deleting a substituted socket. An
existing socket is never silently deleted, even when it might be stale.

Engine cancellation drops in-flight HTTP futures, persists a terminal cancelled
state and records resource/outcome events. Dispatched model calls count even
when the request is later cancelled or fails. Independent evidence determines
verified arithmetic completion; model text remains unverified.

Integration tests exercise real Unix sockets, permissions, malformed lengths,
existing-socket preservation, interrupted frames, disconnect/shutdown during
loopback inference and durable cancelled state. They require local socket access.
The HTTP backend in these tests is a fixture, not a real model.

Outstanding protocol work includes a broker/module event API, explicit typed
cancel/status/stop commands, deadlines and identities in module envelopes,
authentication between separate actors, and general compatibility fixtures.

The v1 response schema now appends optional ResourceUsage and stop_reason fields.
Existing field IDs are unchanged. Legacy v1 replies without these fields decode;
unknown token usage uses optional ulong scalars, never fabricated zero counts.
A compatibility regression covers absent appended fields and metrics round trips.

AskRequest appends optional conversation_ref and explain fields, preserving prior
field IDs and defaults. Correlation IDs are bounded to 128 bytes, requests to
12,000 bytes, and conversation references to valid UUIDs before dispatch.
