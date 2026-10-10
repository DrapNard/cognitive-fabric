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
notifications. Independent-process streaming subscriptions, general executable
module event routing and Unix-socket task-specific controls remain incomplete.
Progress frames are data, never commands or evidence that a model answer is true.
