# Trusted subtask scheduling

Jev+ executes bounded advisory plans sequentially, preserving dependency results
and a final whole-goal solver budget. Plans contain at most four subtasks and
remain untrusted model suggestions. The default `StableScheduler` uses stable
topological order, retaining the previous runtime behavior.

A trusted Rust host can install an `Arc<dyn scheduling::SubtaskScheduler>` with
`Engine::with_subtask_scheduler`. The descriptor has a bounded canonical ID and
semantic version; installation captures it once. Cloned engines retain their
installed handle. `subtask_scheduler_descriptor` and benchmark metadata expose
the configured policy, with implementation attestation explicitly false.

The callback receives an immutable validated `Plan` and returns step indices.
The host rejects any selection that omits, duplicates or invents a step, or runs
a step before its dependencies. The scheduler cannot edit objectives, change
tool grants, bypass admission, allocate fresh budgets or label results verified.
Every actual inference/tool operation still crosses the existing kernel checks.
A malformed order fails the task before subtask dispatch. The callback is used
only when a planner result is accepted as a graph; profiles without planning do
not invoke it. There is no CLI/model/socket policy loader.

## Durable decisions and recovery

New plan events require a committed scheduling record. The runtime writes
`subtask_schedule_intent` before invoking the callback, then a validated
`subtask_schedule` before any pending/running subtask events. Audit failure
prevents the corresponding next action. The decision records format version,
cached policy descriptor, canonical graph hash and index order; it grants no
permissions. Graph audit validates one intent/decision, graph/descriptor binding,
record ordering and sequential execution against that order. Duplicate, missing,
malformed or changed records fail closed. Payloads and sources are bounded in SQL
before copying. A failed callback can leave an intent without a decision; it does
not authorize continuation or a fallback.

Checkpoint prefix validation and continuation use the recorded order rather than
calling a potentially replaced policy. Existing supported continuation limits,
leases, original model/budget snapshots and operator approvals remain in force.
Legacy plans lacking the new required-record marker retain the old stable order.
A new marked plan cannot silently fall back when its decision is missing.
These are owner-local consistency checks, not signatures against a database owner
who can replace all records and hashes.

## Limits

Native callbacks are operator-trusted code: they must make short, nonblocking
metadata decisions. This interface cannot preempt or sandbox callbacks, prevent
their arbitrary host side effects, or attest their implementation. External I/O,
inference and untrusted executable policies need separately budgeted/isolation
integration; declared versions are not code identity. Existing in-flight task
snapshots and already committed orders do not change when another engine installs
a different handle. Current execution remains sequential with a four-node graph
limit. This is replaceable dependency ordering, not general module/GPU scheduling,
calibrated expected utility or proof of faster/better reasoning.
