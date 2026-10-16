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


## HTTP continuation regression coverage

`public_resume_preserves_custom_order_through_real_http_without_replay` saves an
independent two-node graph with order `[1,0]` after node 1 completes. Public
operator-approved continuation contacts a real loopback HTTP fixture exactly twice:
node 0 and the final whole-goal solver. Requests retain the saved result and
remaining result, with no planner or completed-subtask replay. The test asserts
one original execution-settings record, one schedule intent/decision, four total
charged calls, 1,024 reserved output tokens and the original remaining allowances.
A deterministic sorting checker verifies the final answer; this is integration
coverage rather than a model-quality result.

`corrupted_custom_schedule_denies_resume_before_http_or_state_change` changes the
completed prefix order, removes the required decision or changes its descriptor.
Both preview and approved resume reject each case with unchanged task/event state,
unchanged charges and no connection to the listening HTTP fixture. This extends
custom-order execution/reconstruction coverage to actual public HTTP continuation.
It does not prove arbitrary within-call recovery, backend weight identity or
safety against an owner who rewrites every record consistently.

## Shared resource admission

`resource_scheduler::ResourcePool` now supplies a separate native shared FIFO
allocator for inference compute slots and declared host/device working memory.
It coordinates wrapped backends across brokers, with bounded queue/execution
deadlines and quarantine of uncertain dispatched work. See
[RESOURCE_SCHEDULING.md](RESOURCE_SCHEDULING.md) for host configuration, original
budget preservation, audit gates and remaining physical/global scheduling gaps.
Dependency ordering and shared allocation remain distinct responsibilities.
