# Local protocol and service lifecycle

`protocol/cognitive.fbs` defines protocol v1 with file identifier COGF.
Each message is prefixed by a four-byte big-endian length, bounded to 256 KiB.
The receiver verifies the FlatBuffer before accessing fields and rejects unknown
versions, unknown message/mode enums, missing required fields and mixed payloads.
Ask, Reply and Error envelopes carry request correlation IDs. Task IDs identify
persistent execution independently. The generator and Rust runtime use 25.12.19.

The single-user service executes one inference task, with no application task queue.
Up to eight connection handlers permit controls during inference. Excess connections
are closed; additional asks receive correlated busy errors. Peer UID from OS credentials must match the
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

Outstanding protocol work includes a general module event API, deadlines and identities in module envelopes,
authentication between separate actors, and general compatibility fixtures.

The v1 response schema now appends optional ResourceUsage and stop_reason fields.
Existing field IDs are unchanged. Legacy v1 replies without these fields decode;
unknown token usage uses optional ulong scalars, never fabricated zero counts.
A compatibility regression covers absent appended fields and metrics round trips.

AskRequest appends optional conversation_ref and explain fields, preserving prior
field IDs and defaults. Correlation IDs are bounded to 128 bytes, requests to
12,000 bytes, and conversation references to valid UUIDs before dispatch.

### Owner service controls
Additive v1 envelope kinds Control (3) and ControlResponse (4) carry typed Status, CancelActive, and Stop actions. Exactly one payload is permitted; unknown actions fail validation. OS peer UID authentication applies to all controls. Status reports whether inference is active; cancel-active signals only the current execution, and stop cancels work and joins handlers before removing the socket. Up to eight connections may be pending; one execution permit bounds inference. Additional asks receive a correlated busy error. Legacy peers reject unknown controls; existing ask/reply fields remain compatible.

## Internal inference module broker

`protocol/module.fbs` defines the separate COGM v1 FlatBuffers packet used by the
in-process Tokio broker in `src/fabric.rs`. `src/fabric/protocol.rs` validates
both sides of each RPC. Packets carry task/correlation UUIDs, sender, destination,
registered adapter version, expiry, typed role, prompt/output bounds and optional
provider usage. Exactly one request, response or error payload is allowed. Frames
are capped at 256 KiB; prompts at 32 KB and generated text at 96 KB.

Trusted Rust host code registers `InferenceBackend` implementations through
`Broker::register`; model text and socket-supplied identities cannot register them.
Descriptors allow only inference roles. Active role conflicts, duplicate immutable
versions, more than 32 active modules or 128 historical versions are rejected.
Task startup pins a registry snapshot and records adapter versions. Replacement
or unregistration affects new tasks; pinned tasks retain their adapter handles.
These are adapter versions, not immutable neural weight attestations.

Each adapter has a lazy worker, one active request and four queued slots. Overflow
fails explicitly. Workers validate encoded identity against the runtime-owned
request handle, check actor/task policy and expiry, and correlate the response.
Dropping the request receiver cancels the backend future; queued cancelled work is
skipped. Session ceilings/timeouts remain enforced independently. An upper bound
of 300 seconds is encoded for each RPC, while task/request cancellation can end it
earlier. External inference servers may continue their own computation after HTTP
disconnect; dropping a client future is not a server-side GPU cancellation guarantee.

`modules` shows the direct runtime's adapter registry. `modules --socket` discovers
the running owner-local service via additive control action Modules (3) and typed
ModuleInfo entries. Older control responses decode with an empty module list.
Registration is currently a trusted Rust API; no executable plugin or remote actor
registration is exposed. General event subscriptions, separate-process module
authentication and non-inference module routing remain incomplete.

The `inference_ms` resource counter measures dispatch duration, including broker
queueing, coordination, HTTP time and cancellation; it is not measured GPU time.
