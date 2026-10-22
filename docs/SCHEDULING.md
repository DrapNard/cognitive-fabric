# Trusted subtask scheduling

By default Jev+ executes bounded advisory plans sequentially, preserving dependency results
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
not invoke it. Arbitrary callback loading is not exposed to CLI/model/socket callers.

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
a different handle. Default execution remains sequential with a four-node graph
limit; opt-in ready waves are described below. This is replaceable dependency ordering, not general module/GPU scheduling,
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

## Configured critical-path ordering

Set the optional `subtask_scheduling` field in the owner-controlled `config.json`
to `"critical_path"`. Omission, null or `"stable"` retains original-order stable
topological scheduling; unknown names are rejected. Default serialization omits
the field, preserving legacy configuration fingerprints. An explicit selection
is included in execution-settings snapshots and empirical configuration scope.

`CriticalPathScheduler` validates the original four-node plan before computing
unit-cost downstream heights. Among currently ready nodes, the longest remaining
dependency chain runs first; equal heights use ascending step IDs. Step indices
and dependency-array order do not determine ties. A chain `a → b → c` therefore
runs before an independent `zz` even if `zz` appeared first in the planner JSON.
This is a deterministic graph heuristic: step names/dependencies are untrusted
suggestions, and heights are not estimated GPU durations or utility scores.

Engine and direct Session construction install this configured built-in policy,
covering their CLI/service callers. A trusted host may still explicitly replace
the Engine handle with `with_subtask_scheduler`; already cloned engines retain
their own handle. Accepted graph execution persists the existing intent and
validated decision before any subtask. The original call/token/prompt/tool/time
ceilings and first-final-solver admission are unchanged. No model call is added.

Recovery uses the saved order, never a new policy computation or the current
configuration file. `tests/critical_path_scheduling.rs` exercises actual CLI/local
HTTP execution and resumes a saved prefix after changing current config to stable:
completed work and planning are not replayed. Unit tests cover every permutation
of all 64 edge subsets of a four-node labelled topological DAG, malformed public
plans, configuration fingerprints, direct Session installation and audit denial.

Critical-path ordering alone is sequential. Reordering cannot establish lower total compute,
faster completion, better answers or optimal scheduling; no real-model improvement
has been measured. Dynamic costs, physical GPU/memory allocation and calibrated
utility remain separate incomplete work. Protocol schemas and permissions do not
change.

## Opt-in parallel advisory execution

An operator can set `"subtask_parallelism": 2` in `config.json`; the allowed
range is 1–4. Omission preserves the existing serialized configuration and
sequential behavior. A value of 1 also uses the sequential path. This changes
only accepted advisory planner graphs, each still limited to four subtasks.

Jev+ scans the committed scheduler order and groups currently dependency-ready
nodes into bounded waves. A blocked node does not prevent a later independent
node joining a wave. All dependencies must have recorded results from previous
waves. Results remain unverified advice for the final whole-goal solver.
Within each wave, results and reasoning events are reconciled in deterministic
dispatch order regardless of backend completion order.

The complete graph admission reserves room for all pending subtask calls,
outputs and bounded prompts plus the final solver. Each wave authorizes and
durably reserves its individual calls before polling any backend future.
Required audit failure stops dispatch. Shared resource-pool admission still
applies to every call; a graph concurrency ceiling does not create provider
slots, GPU memory or permission. Engine construction also configures the local
FlatBuffers inference broker with the same bounded module concurrency, so its
legacy serial queue does not prevent overlap. The module keeps its four-request
queue, active-count tracking and owned cancellation/shutdown jobs. Omitted/one
configuration retains single-request module execution. A provider or an external
process transport may still serialize requests itself.

Backend futures are locally owned. Cancellation drops unfinished futures and
retains original reservations; it does not prove remote provider termination.
Incomplete usage remains unknown. Per-node resource records avoid shared-counter
deltas; aggregate `inference_ms` sums local admitted-call elapsed time, including
queue/setup wait, and can exceed task wall time for overlapping calls. It is not
GPU execution time. Original deadlines and output ceilings still apply. Parallel work never grants fresh task calls, tokens, tool allowances or
wall time. No new executable, external action or model is activated.

A required `subtask_parallel_schedule` event binds the pinned graph, original
scheduler order, ceiling and complete ready-wave partition, with verification
and permission flags false. Graph observation validates its bounded shape,
chronology and wave dependencies before accepting overlapping running states.
Every member must enter the recorded running cohort before a wave result is
accepted; this validates trace admission, not actual provider-level overlap.
The existing sequential audit contract remains unchanged for default tasks.

Operator-approved continuation now supports completed-wave prefixes and ready
final-solver boundaries after strict call/result/schedule validation. It skips
completed waves and reuses the original full marker and saved concurrency;
original charges/deadlines remain authoritative. Incomplete or unknown-outcome
waves are refused and no work is replayed/refunded. Fresh independent checking
of a durable saved candidate keeps its existing separate rules and cannot use
the parallel marker as proof. See [parallel recovery](RECOVERY.md#completed-parallel-advisory-waves).

Verification: formatting, locked workspace/all-target build, **833 tests across
49 targets** (none ignored) and Clippy with denied warnings pass. New coverage
includes seven helper, three broker, six end-to-end Engine and four graph-audit
regressions. Barrier-controlled backend calls overlap; dependencies wait, reverse
completion still yields deterministic result order, and planner/subtask/final
call/output/prompt reservations remain exact. Cancellation drops active work and
records unknown usage without refunds. Cap enforcement, queued work, shutdown,
serial compatibility, insufficient budget, schedule/reservation audit failures,
14 altered markers, wave crossing and read-only continuation refusal are covered.
The existing backend-panic health test retains its closed-worker behavior.

The initial overlap test caught broker serialization; fixing the worker enabled
actual overlap. Lint findings and an intermediate source-fingerprint failure
caused by concurrent edits were fixed, followed by a full frozen rerun. Bundled
protocol pairing/integrity checks pass; schemas/bindings are unchanged and absent
`flatc` prevents fresh generation. Localhost port 1234 refused connections during
approved discovery. Mock overlap is concurrency evidence, not live-model latency,
quality or physical-resource proof. Jev+ and Jev-R remain Partial.

### Parallel continuation verification (2026-10-22)

The full workspace passes formatting, locked all-target build, **840 tests across
50 targets** (none ignored), and Clippy with warnings denied. Five HTTP-backed
recovery regressions cover whole-wave reuse, a wave prefix that differs from the
scheduler's total-order prefix, simultaneous pending requests, deterministic
final context, original criterion checking, ready-solver and pre-marker crash
windows, nine read-only refusal cases and transactional admission rollback.
Two new executor unit cases retain hard caps and dependency readiness even for
invalid internal saved partitions. Original marked history stays single and
immutable. No planner or completed-subtask replay is observed; the authored
fixture finishes with six charged calls and 1408 reserved outputs after three
new calls. These are functional/accounting checks, not live-model performance
or neural safety evidence. Local port 1234 was unavailable; unchanged bundled
protocol pairing tests pass, while absent `flatc` prevents fresh generation.
