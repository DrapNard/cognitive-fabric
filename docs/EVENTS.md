# Typed task progress events

`protocol/events.fbs` defines the version-1 `COGE` FlatBuffers progress envelope.
It carries a task UUID, durable audit sequence, known stage/source/state, optional
role/subtask identity, optional actual resource counters and verification status.
Absent counters mean unknown; zero means a measured zero. Unknown versions,
unknown stages, malformed frames, invalid identities and frames above 4 KB fail
validation before consumers access their fields.

Runtime audit milestones produce these typed events after SQLite persistence:
start, routing, model dispatch, reasoning step, subtask update, verification,
terminal outcome and interrupted-task recovery. These envelopes do not carry
prompts, answers, retrieved documents, credentials or arbitrary model annotations.
Full traces remain in the audit store. Audit payloads are capped at 256 KB for new
writes; progress reads fail closed on oversized stored records.

Owner-local Rust consumers call `Store::subscribe_progress(task_uuid)` and await
`Subscription::receive()`. Subscriptions are scoped to one existing task. Each
Store hub supports 64 active subscriptions with 16 queued frames per consumer.
Dropping a subscription cancels it. Producers never wait for a slow consumer:
overflow disconnects that subscription and future reads report an explicit gap.
The subscriber must reload durable progress and resubscribe. These runtime-owned
APIs are trusted in-process interfaces, not authenticated third-party plugin entry
points. Model actors cannot request the observation capability.

`Store::progress_since(task_uuid, cursor)` catches up from SQLite, also across
processes and after restart. `cogfabric task-progress TASK_UUID --after CURSOR`
exposes the same bounded page. Each page scans at most 32 audit records, returning
only typed milestones. Save the returned cursor even for empty event pages, and
continue while `more` is true. Sequence numbers are global audit IDs, so gaps
between task events are normal. For live observers, subscribe first, then catch
up durably and deduplicate by sequence to avoid a registration race.

Live subscriptions observe writes through that Store instance; another process
must use durable catch-up. SQLite remains authoritative. Recovery events publish
only after their transaction commits. Per-Store insertion/publication is ordered;
this is neither a distributed consensus service nor exactly-once delivery.

The event hub extends the inference broker's typed communication with progress
notifications. General executable module event routing remains incomplete.
Unix-socket progress polling, streaming and task-specific cancellation
are available as described below.
Progress frames are data, never commands or evidence that a model answer is true.

## Owner-local socket controls

While `cogfabric serve` runs, `cogfabric service-status` reports its active task
UUID. Use `cogfabric task-progress TASK_UUID --after CURSOR --socket` to request a
bounded durable page over the owner-authenticated Unix socket, or
`cogfabric cancel-task TASK_UUID` to cancel that specific active execution. Wrong,
completed and stale task targets return `accepted: false`; they do not cancel a
subsequent task. The current-task identity and cancellation signal are coordinated
under one runtime mutex. Ask correlation IDs do not select execution identities.

The appended COGF control fields preserve legacy defaults. Progress pages contain
verified nested COGE FlatBuffers frames, bound to the requested task and strictly
increasing sequences. Both peers validate frame counts, cursor bounds and task
identity. Controls use the existing five-second client deadline and bounded socket
handlers; inference cancellation and progress controls remain available while work
is running. A just-admitted UUID can briefly precede its database task creation;
a progress request then returns an explicit unknown-task error, so retry after
creation.

## Bounded socket subscriptions

`cogfabric watch-task TASK_UUID --after CURSOR` connects to the service and catches
up durably before delivering live service-owned updates. With `--json`, each line
is one object containing `task_id`, `state` and `page`. Save `page.cursor` after
every update, including empty pages. `complete` means a terminal task event was
observed; it does not imply a verified answer. `timeout` ends the watch normally
after 60 seconds; reconnect with the last cursor to continue. Ctrl-C or dropping
the watcher closes observation without cancelling the task.

The owner-authenticated service allows two watchers, preserving handler capacity
for task submission and controls. Each watcher has 16 queued live frames and a
two-second write deadline. Overflow, shutdown or protocol errors terminate the
connection explicitly; reconnect and replay durable progress. Catch-up pages scan
32 audit records each. Task IDs, request correlation, frame bounds and monotonic
sequences are validated on both peers. SQLite is authoritative; writes from other
Store instances are available through reconnect/catch-up but do not trigger this
service's live event hub. No exactly-once or cross-process publication guarantee
is claimed.

## Versioned host module subscriptions

Embedding hosts can register `events::modules::EventModule` through
`Store::register_event_module`, discover active descriptors, and unregister them.
Descriptors contain a bounded local module ID, semantic version and a nonempty
unique subset of typed `Stage` values. They contain no executable path or model
instructions. The registry permits 32 active IDs and 128 immutable ID/version
pairs per Store lifetime; unregistering does not erase version history.

`Store::subscribe_module_progress(task, module_id)` subscribes a registered module
to one existing task and pins that descriptor/stage filter. Replacement or
unregistration affects future subscriptions; existing handles retain their pinned
settings until dropped, overflow or Store shutdown. Inspect `descriptor()` and
await `receive()` on `ModuleSubscription`. All module subscribers share the existing
64-consumer ceiling and 16-frame queues with owner observers. Filtering happens
before queue admission, so unrelated tasks/stages consume no consumer capacity.
Overflow remains explicit and durable task progress supplies catch-up.

Delivery uses the same verified COGE FlatBuffers frames and ordered after-commit
publication as owner observation. Prompts, answers, arbitrary annotations and
credentials remain excluded. No additional task execution or capabilities result.
The registry and handles are trusted in-process host interfaces, not authenticated
remote/module actors. They are process-local and are not restored from the database.
Host application code must re-register modules on startup and release handles on
cancellation. A descriptor string is routing metadata, not identity attestation.
No new model permissions, executable plugins or general module command transport
are introduced. General cross-process module registration/routing remains partial.

## Observer cancellation and module replay

`Subscription::cancel()` closes only that observer and discards buffered frames.
`receive_until(cancel_future)` gives an immediately ready cancellation priority over
queued delivery, closes observation and returns `None`. Future receives also return
`None`; task execution and durable audits are unchanged. Closing releases shared
subscription capacity even while the Rust handle remains alive. The same methods
are available on `ModuleSubscription`. Cancellation intentionally abandons live
delivery; durable history remains the recovery source.

`ModuleSubscription::progress_since(store, cursor)` binds replay to its original
task UUID and pinned stage filter, including after replacement, overflow or
cancellation. It uses the existing bounded durable scanner; filtering preserves
`cursor` and `more`. Empty filtered pages may still advance the cursor and require
another page. Subscribe before catch-up and deduplicate live/replayed sequences.
No events from unrelated tasks or newly configured module stages enter replay.
The supplied Store remains a trusted host reference, not an IPC identity assertion.


## Authenticated process event modules

`events::unix` adds a trusted embedding-host API for event consumers in separate
local processes. The host registers an `EventModule`, accepts an already connected
Unix stream, and calls `EventSession::connected(store, task, module_id, stream,
uid, pid)` with an explicitly approved peer UID/PID. Kernel peer credentials must
match before subscription. `session.grant()` supplies the immutable task/module
version/stage grant to trusted provisioning code. The consumer constructs an
`EventReceiver` with that grant and the expected host UID/PID. Neither peer chooses
an executable, self-registers through IPC, or receives task/tool authority.

`EventSession::serve(seconds, cancel)` emits a typed open frame, filtered live
progress and a terminal control frame. Sessions permit 1–300 seconds after opening
and at most 4,096 events, with two-second write deadlines. Consumers require each
complete frame within ten seconds, including idle time. All streams share the
existing 64-subscription and 16-frame queue ceilings. Queue loss is an explicit
error; a slow peer cannot block event producers. Upstream bytes are forbidden and
close the session as a protocol error. Host cancellation closes observation only.
Registry replacement/unregistration preserves the existing pinned stream grant.

`protocol/event_stream.fbs` defines version-1 COGS frames (8 KB maximum), wrapping
verified COGE progress with exact task/module/version identity. Wrong protocol,
identity, stage, packet shape or non-increasing event sequence fails closed.
Cancelling a receiver future permanently drops its connection. `end_reason()`
distinguishes normal terminal observation, cancellation, timeout and frame limit;
`Completed` means a Finished milestone was observed, including unverified outcomes.
Transport closure never proves task verification. Abrupt I/O/audit failure may
leave only the durable session-start audit; delivery is not exactly once.

The host owns socket creation/permissions, child lifecycle and trusted grant
provisioning; the library does not start a plugin listener or expose a new CLI
endpoint. Live delivery observes this Store instance. Durable replay remains the
existing pinned `ModuleSubscription::progress_since` API; automatic IPC replay
and general module commands/registration are not implemented by this stream.
UID/PID checks do not attest executable contents or isolate malicious same-owner
code. Tests exercise a real child process, forged credentials, wrong grants,
malformed/versioned packets, stage/task denials, overflow, cancellation and
session/incomplete-frame deadlines. No new model capabilities are introduced.


## Durable process-stream catch-up (COGS v2)

`EventSession::serve_from(seconds, after, cancel)` explicitly selects COGS v2,
replays bounded durable pages through the pinned stage filter, then switches to
live delivery. Subscribe/construct the session before starting catch-up; queued
live events already covered by a replay cursor are discarded. `Cursor` control
frames advance through empty filtered pages without exposing excluded events.
`EventReceiver::cursor()` returns the latest consumed durable position, including
cursor/terminal frames; save it before reconnecting. A cancelled receive permanently
drops that connection while preserving the last consumed cursor for recovery.

Keep the trusted `Grant` returned by `session.grant()` and use
`EventSession::reconnect(store, grant, stream, uid, pid)` to restore the exact
original task/version/stage scope even after registry replacement or unregistration.
The host supplies the grant; IPC never restores permissions from received strings.
The receiver must use the same grant and authenticate the new approved host peer.
Host provisioning and child/socket lifecycle remain external responsibilities.

Replay counts both events and cursor markers toward the 4,096-frame session cap;
all existing session/write/queue limits apply. Consumers additionally cap total
frames at 4,098 (including Open/Closed) and wall time at 305 seconds from connection,
so a malicious cursor-only peer cannot prolong processing indefinitely. A normal
limit/timeout closes with the last consumed cursor; reconnect to continue. SQLite
is authoritative, delivery is not exactly once, and abrupt connection/audit loss
can require replay of already written frames.

The original `serve` API continues emitting live-only v1 frames. New receivers
accept v1 and v2; midstream version changes, backwards cursors, event/cursor
mismatches and v1 cursor packets fail closed. Old v1 readers reject v2 explicitly.
Tests cover filtered empty-page advancement, saved-position reconnect after
unregistration, replay/live overlap, pinned filters, protocol compatibility and
cursor-only frame/time exhaustion. This supersedes the IPC-replay limitation
above; automatic provisioning and general module commands remain incomplete.

### Host event-module rollback

`Store::activate_event_module(id, version)` restores a previously registered
immutable descriptor, including after unregistration. Future subscriptions use its
stage filter; existing subscriptions and authenticated process grants retain their
original task/version/filter. Durable replay uses each subscription's pinned
filter, regardless of the currently active version. `event_module_versions()`
lists retained metadata, including inactive entries, without creating observers.

History is capped at 128 versions and 32 active module IDs. Activation at full
history is allowed, but cannot exceed the active-module ceiling; unknown selectors
and forged version replacement fail without changing active routes. Metadata is
process-local and is lost when the Store/runtime closes. Retention contains bounded
descriptors, not executable backends or subscribed queues. This trusted host API
adds no model/IPC command or task-observation permission, and provides no backend
health, automatic process provisioning or persistent executable lifecycle.
