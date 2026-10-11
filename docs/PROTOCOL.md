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

## Progress envelope

### Worker-side authority and bounded sessions

`protocol/bootstrap.fbs` defines bounded COGB v1 bootstrap settings for one task.
`fabric::unix::bootstrap::Bootstrap` validates task UUID, backend, loopback endpoint,
model/role settings and resource ceilings using the existing Config rules. Frames
are limited to 16 KiB; duplicate roles, unknown versions, malformed buffers and
nonlocal endpoints fail closed. Generation settings are pinned in the frame;
workspace paths, model-registry authority and executable grants are absent.
`authority` converts validated settings to a single-task inference grant with the
same cumulative ceilings. The codec does not authenticate its sender: callers must
check peer credentials before accepting bootstrap data. This is the typed setup
interface used by the opt-in same-binary managed launcher. `receive`
checks approved host UID/PID before reading a bounded length-prefixed frame under
a ten-second deadline; `send` has a two-second write deadline. Role/model
names remain declarations, not neural weight attestations.

`fabric::unix::worker::Authority` is created by trusted setup code, not deserialized
from requests. It pins the host UID/PID, module descriptor and 1–64 unique approved
task UUIDs. Worker requests must match a grant, role-derived actor, destination,
version and an expiry at most 300 seconds ahead. Unapproved tasks close the session
before inference. This is mutual connected-peer checking, not executable attestation.

Per-task cumulative ceilings default to 8 calls, 1600 reserved output tokens and
128 KB prompt bytes; configurable ceilings are bounded to 128 calls, 1,048,576
tokens and 4 MiB prompt bytes. Reservations count failed calls. Session limits
follow the approved task wall ceiling (1–3600 seconds; standalone authorities
default to 300) and 1024 exchanges. Idle time consumes the session lifetime;
after the first frame byte, the remaining prefix/body allows 10 seconds. Writes
allow 2 seconds. One inference runs at a time. Disconnect, pipelined bytes, shutdown,
expiry or invalid output terminates the session and drops the backend future.
Closing the shutdown channel also stops the session. Backend-side GPU cancellation
is still not guaranteed. Host task ceilings remain independently enforced.

The supplied Rust `serve` API handles worker-side authorization; tests cover real
adapter/worker round trips, cumulative budgets, forged actors/unapproved tasks,
version/deadline denials and active cancellation. The managed launcher supplies
single-task grants to its own worker executable; generalized executable module
launch, sandboxing and automatic replacement remain unfinished. Task grants are immutable for a session;
new grants require trusted creation of another authority/session.

COGB appends max_wall_seconds with the legacy 300-second default. Current managed
launchers send the task's configured ceiling; invalid/zero/over-limit lifetimes
fail validation. The worker audits its approved lifetime. Runtime expiration is
monotonic, while each request also retains the bounded absolute expiry check.

Worker `serve` also requires an `Audit` bound by trusted setup to an existing
SQLite task. Session start/stop/failure, authorization/denial and inference outcome
events carry versioned session/module/task/correlation IDs and cumulative charged
counts. Prompts, answers, backend errors and provider metadata are excluded.
Unknown request tasks are recorded under the trusted anchor, never used as event
owners. Anchor existence and event insertion are atomic. Audit failure before
dispatch prevents inference; outcome-write failure closes the session, though
already executed inference cannot be undone. Database busy/I/O blocking is subject
to SQLite's existing limits rather than a hard real-time guarantee. A process
crash or externally dropped serve future can leave an unterminated session record;
recover-workers reconciles these records under an exclusive runtime lease without
replaying inference or changing task results. In-memory Stores are
useful for tests but do not supply persistence; operators must supply a file Store.

### Host-connected Unix inference adapters

`fabric::unix::UnixInference::connected` accepts an already-connected UnixStream,
immutable module descriptor and host-approved peer UID/PID. OS credentials must
match both values, and platforms without peer PID support fail closed. Trusted
host code supplies and registers the adapter; no model-supplied socket path,
process spawning, executable discovery or self-registration is exposed.

Requests and responses use four-byte big-endian lengths and verified COGM frames
with the same 256 KiB bound. Registered roles, task policy, version, task and
correlation UUIDs, expiry and output-token ceilings are checked. Exchanges are
serialized, with a 300-second ceiling including waiting. Use the broker to enforce
its four-slot admission queue rather than creating unbounded direct callers.
Any cancelled, malformed, mismatched or failed transport exchange closes the
adapter permanently; explicit host replacement is required. Valid remote error
packets may reuse the connection. This prevents stale response reuse across tasks.

Tests include a separate fixture process authenticated by its actual PID plus
wrong UID/PID, forged task, oversized length and cancellation denials. This is a
connected-process identity check, not executable attestation, sandboxing or neural
weight verification. Same-user compromise and inherited/passed descriptors remain
outside isolation guarantees. Generalized executable module lifecycle,
stale-resource cleanup and general module event routing remain incomplete; remote
computation may continue after the host closes its connection.

`configure-worker --enabled true` selects managed task-local inference in new
Engines/services. The host launches its own hidden worker entry point, provisions
authenticated COGB settings, and explicitly closes/reaps the child on termination.
No process launches until the first model call. See `WORKERS.md` for ownership,
fallback cleanup, timing scope and remaining limitations.

`protocol/events.fbs` adds independent `COGE` version-1 frames generated with the
same pinned flatc/Rust 25.12.19 pair. Bounded typed milestones cross local Tokio
subscription queues; no JSON message payload replaces internal FlatBuffers.
SQLite JSON audit metadata is projected into validated typed fields before
publication. See `EVENTS.md` for capacity, overflow and durable cursor semantics.
The inference-RPC schema remains unchanged; owner-IPC additions are described below.

COGF v1 now appends TaskProgress (4) and CancelTask (5) control actions. Requests
append optional task UUID and after-sequence fields; task actions require UUIDs,
and only progress accepts a nonzero cursor. Control responses append optional
active-task UUID and a task-bound progress page of at most 32 independently
verified COGE frames. Existing action ordinals and absent-field defaults remain
compatible. Nested malformed frames, task mismatches and unordered sequences fail
closed. Clients correlate responses and reject progress preceding their cursor.

SubscribeProgress (6) appends bounded streaming observation. It requires a task
UUID and accepts an after-sequence cursor. ControlResponse appends optional
`stream_state`: `update`, `complete`, or `timeout`, valid only with a progress page.
Legacy responses omit this field. One request yields correlated validated pages
until termination; it does not grant execution authority. See `EVENTS.md` for
queue, watcher, write-deadline and replay limits.

ResourceUsage additionally appends optional unsigned byte counts
`process_rss_bytes` and `process_lifetime_peak_rss_bytes`. Legacy frames decode
these as unavailable. They describe the Rust process, not the external backend
or task-attributable peak; see `BENCHMARKING.md` for measurement scope.

### Optional broker queue timing

The module InferenceResponse and owner IPC ResourceUsage append optional
`queue_wait_ms` double fields without reordering existing schema fields. Legacy
frames omit them and decode as unavailable. Values must be finite and within
0–300,000 ms, the maximum module request lifetime. The runtime broker overwrites
backend queue claims with its own enqueue-to-worker-receive measurement. Absent,
negative, nonfinite and over-limit cases have compatibility/negative tests.

## Host inference broker shutdown

`Broker::shutdown()` permanently stops one broker and all module versions it
created, including retired versions retained by pinned tasks. It clears active
discovery, refuses registration/new pinning/dispatch, signals active backend
futures for cancellation and drops queued replies. Shutdown is idempotent; worker
loops observe the signal asynchronously, and the method does not join them.
Ordinary replacement/unregistration still permits already pinned tasks to finish.
This trusted Rust host API is not exposed as a model capability or an
unauthenticated protocol command. Dropping an HTTP inference future cannot prove
that the external server stopped GPU work. General executable lifecycle and
independent actor provisioning remain incomplete.
