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
