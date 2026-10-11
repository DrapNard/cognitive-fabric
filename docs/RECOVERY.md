# Interrupted task recovery

Run `cogfabric recover-tasks` against the original state directory after an
interrupted runtime. It marks abandoned `running` tasks as `interrupted`, records
a per-task recovery audit, and leaves completed results unchanged. Repeating it
returns an empty list. It never retries a tool, resends inference, claims unknown
usage totals, or treats unfinished subtask results as verified. Start a new task
explicitly when another attempt is appropriate.

Every persistent Engine execution holds a shared operating-system flock from
before task creation until its execution future finishes or is dropped. Recovery
requires an exclusive nonblocking flock, then one immediate SQLite transaction.
Other local processes cannot start Engine execution during recovery. Recovery
fails while any engine lease is held, even within the same process. Kernel locks
release on process death; neither a PID file nor task age establishes abandonment.
A crash during recovery rolls back task and audit writes together. Batches above
10,000 unfinished tasks fail closed for offline inspection.

The persistent lease file is `fabric.sqlite3.runtime.lock` beside the database.
It must be an owner-only regular file, with one hard link and no final symlink.
Do not delete, replace or rename it while any process uses the state directory:
locks belong to an inode, and replacing it would split ownership. Use one canonical
local database path; hard-link aliases, copied live state and network filesystems
are unsupported. The owner can modify local files, so this is cooperative runtime
coordination, not isolation from a malicious owner or modified fork. Custom callers
that write `running` rows directly must obtain equivalent coordination; raw Store
mutation is not a task execution API.

Recovery is explicit, operator-only and independent of model reasoning. An idle
service holds no task execution lease, so recovery may inspect its database when
no task executes. The command does not remove stale sockets, resume subtask graphs,
repair corrupted databases or prove that an external inference server stopped
computing. Those cases need separate handling. Recovered task snapshots can enter
episodic memory only through explicit operator publication; they cannot become
verified arithmetic memories.

Logical storage admission and reviewed retention are documented in `STORAGE.md`.
Interrupted rows count toward the global eight-running-task admission ceiling
until recovery marks them terminal; recovery itself does not require free task
slots and never silently deletes their history.

## Interrupted worker audit sessions

Run `cogfabric recover-workers` to reconcile sessions that recorded session_started
but no session_stopped, session_failed or prior session_interrupted. It requires
exclusive runtime ownership and leaves task statuses/results unchanged. Use
recover-tasks separately for abandoned running tasks. No PID is killed or treated
as proof of abandonment; no inference is replayed and no unknown success/resource
usage is invented. External HTTP/GPU computation remains unverified.

Both managed host startup and persistent worker serving hold shared execution
leases. A surviving worker therefore blocks reconciliation even after its host
dies. New worker launch checks running status under its lease and refuses terminal
tasks. Lease replacement/aliases and hostile same-owner code retain the caveats
above. Standalone workers using in-memory audit stores do not support durable
recovery.

Worker reconciliation uses one immediate transaction, validates session/module
metadata and records bounded session_interrupted events under the original trusted
anchor. Completed task answers remain intact, even when their worker audit ended
abruptly. Repetition is idempotent. At most 10,000 sessions are processed, with
five-second progress/deadline checks; existing SQLite busy/I/O and fsync behavior
are soft rather than hard real-time bounds. Malformed metadata, quota/SQL errors
and audit-write failures roll back the whole operation. The temporary progress
callback is removed on success or error.

This reconciles audit history, not stale temporary socket directories or arbitrary
executable processes. Operator-reviewed filesystem cleanup remains separate work.

## Bounded audit inspection

`cogfabric --json task-events TASK_UUID --after CURSOR` returns up to 32 complete
audit events, each with its durable sequence, kind, source and parsed payload.
Save `cursor` and continue while `more` is true. Empty terminal pages preserve the
supplied cursor. IDs are global, so gaps caused by other tasks are normal; events
remain scoped to the requested task. Unknown tasks, invalid/overflowing cursors,
malformed JSON and oversized stored fields fail closed. This is read-only operator
inspection, not replay of task execution or a checkpoint acceptance decision.

`task` / `Store::task` full traces now refuse histories beyond 4,096 events or
8,000,000 raw payload bytes and direct callers to pagination. Each event is bounded
to 256 KiB payload plus 64-byte kind/source; SQL excludes oversized fields before
copying them into Rust. Task query/answer and metadata fields are bounded too.
The full trace includes each event sequence. Invalid JSON is an error rather than
silently becoming null. The original durable audit is never truncated or deleted.

A page holds at most 32 payloads of 256 KiB (8 MiB raw), with normal JSON parsing
and serialization overhead in addition. These are allocation/admission bounds,
not a hard process-memory quota or hard filesystem/SQLite execution deadline.
The indexed `(task_id,id)` lookup supports ordered incremental inspection.
Benchmark identity disclosure uses bounded selected audit records so large traces
do not require full-trace allocation merely to report model configurations.

## Inference reservation audit

`cogfabric --json inference-audit TASK_UUID` reconstructs the recorded reservation
ledger in one read transaction. New reasoning results carry their charged `call`
number, correlating them with prior model_dispatch reservations. The summary exposes
call/role, reserved output tokens, prompt-byte charges, reported model/usage and
result-recorded state without prompts or answer bodies. Result hashes and role/call
identity are checked. Duplicate/unmatched results, invalid metrics, noncontiguous
reservations and oversized history fail closed.

At most 64 reservations and 128 selected audit records are admitted, with the
existing 256 KiB payload bound. Provider token totals are available only when every
reservation has a matched recorded result with both usage values and no legacy
uncorrelated results. Missing/legacy usage remains null. Legacy reasoning events
without call identity are counted explicitly, never matched by guessing their role
or position. Per-call known usage remains visible even when totals are unavailable.

`result_not_recorded` does not assert that an inference is still running, failed,
or consumed zero resources: crashes, failed calls and incomplete audit writes can
all produce it. The command does not refund reservations, retry calls, authorize
resumption, attest weights or decide that a model answer is verified. A cryptographic
content hash checks local consistency, not trust against a hostile database owner.
This is a prerequisite for reviewed checkpoint continuation; automatic graph
resumption still needs pinned configuration, state and remaining-budget admission.
