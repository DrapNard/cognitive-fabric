# Interrupted task recovery

Run `cogfabric resume-preview TASK_UUID` against the original persistent store
to inspect current continuation eligibility, checkpoint phase, subtask states,
and consumed/remaining inference budgets. It uses the same admission checks as
`resume-task`, including prompt fingerprints, original deadlines and managed
workspace validation, under an exclusive runtime lease and a bounded read
transaction. CLI startup opens only an existing current-schema database in
read-only/query-only mode, bypassing directory creation, permission repair,
migrations and registry bootstrap. Missing or older databases are rejected.
It does not change tasks or events, launch workers, or grant
approval. Query, context and prior answers are omitted. The cooperative lease
file may be created. Wall-time availability is sampled and immediately ages;
actual resume revalidates everything and still requires `--human-approve`.
Unsupported or invalid checkpoints fail with an error rather than an eligible
report. Eligibility does not attest provider weights or guarantee inference.

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
A crash during recovery rolls back task and audit writes together. Task identities
are bounded before allocation and validated as UUIDs before any update. SQLite
checks a five-second recovery deadline every 1,000 virtual-machine instructions,
with additional checks between task updates and before commit. Deadline failure
rolls back the batch and removes the temporary callback. SQLite busy waits,
filesystem I/O and fsync are not hard real-time bounded. Batches above
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

At most 64 reservations, 128 inference records, one settings record, 64 tool reservations and 16 cycle reservations are admitted, with the
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

## Durable execution settings

New sessions record version-1 execution_settings before inference: validated runtime
configuration, requested reasoning mode and effective call/cycle ceilings. Engine
execution_options additionally records the ablation profile, explanation option
and explicit conversation reference. Workspace paths are excluded by configuration
serialization; dedicated environment credentials are never serialized. The engine
validates embedded configuration before task creation, and Session validates its
own entry point too. Snapshot write failures prevent model dispatch; initialization
failure may leave a running row for explicit lease-protected recovery.

These are runtime admission/settings records, not permission grants or backend
weight/configuration attestation. Custom backends and independently registered
modules may have settings outside Config. Existing tasks without snapshots cannot
retroactively acquire them. Restoring a snapshot must not restore authority from
its JSON: the deterministic kernel must issue new task-scoped permissions. Actual
checkpoint continuation still requires graph validation, immutable references,
remaining resource/time admission and handling of unrecorded calls.

## Advisory graph recovery inspection

`cogfabric --json graph-audit TASK_UUID` reconstructs a saved plan and subtask
transitions in one read transaction. It validates the bounded acyclic plan,
unchanged node definitions, dependency-result ordering and allowed state
transitions. A plan saved before initial node writes reports `not_recorded` for
those nodes; a task with no plan reports `graph_recorded: false`. Missing state is
never guessed. Duplicate plans, changed definitions, unknown nodes, impossible
transitions and oversized results fail closed.

The report exposes node IDs, dependencies, objective/result hashes and recorded
states, excluding objective and answer bodies. Results remain unverified advisory
data even after every node finishes. Up to 17 selected events with 256 KiB payload
bounds are read; each plan has at most four nodes and each result 1,200 characters.
The current scheduler creates one plan per task. Repeated plans require a future
explicit versioned graph protocol rather than implicit replacement here.

This is inspection, not checkpoint execution: no permissions are restored, no
inference is repeated, and `resume_authorized` remains false. Remaining budgets,
immutable expert versions, unresolved reservations and verification state must be
validated separately before implementing continuation. Hashes check consistency,
not authenticity against a database owner who can rewrite history.

## Saved inference admission ceilings

`inference-audit` validates a recorded execution_settings snapshot against the task
mode and deterministic mode/config ceilings before calculating
`remaining_inference_admission`. The report subtracts every recorded call,
output-token reservation and prompt-byte charge from its original saved limit.
Unmatched calls retain their entire charge; provider-reported token usage never
refunds reserved output. Later workspace configuration changes are irrelevant.

Without a settings snapshot this field is null. Duplicate, late, invalid or
mode-mismatched snapshots, inconsistent effective ceilings, uncorrelated legacy
results alongside new settings and charges exceeding original limits fail closed.
Wall time remains null. Older settings without a resource reservation version
retain null cycle/tool allowances. New versioned resource ledgers also reconstruct
remaining cycles and tools using the conservative rules below. These three remaining inference allowances
are bookkeeping only: `resume_authorized` stays false, including when all call
results are present. They do not attest backend settings, restore permissions or
prove external inference terminated.

## Durable tool and cycle reservations

New execution_settings includes resource_reservations_version 1. Before an
admitted tool is returned to its caller or a reasoning cycle starts, Session
records a versioned tool_reservation or cycle_reservation. Audit-write failure
prevents that work. Numbering is contiguous per task; denied tools consume no
invocation. A permission-decision event alone does not prove tool reservation.

`inference-audit` checks reservation sources, actors, capabilities, numbering and
original ceilings, and subtracts charges even without recorded results. Tool
reservations with no compatible settings marker fail closed. Older tasks lack
this completeness marker, so zero observed reservations cannot imply zero usage.
A reservation proves admission only, not that an operation ran or succeeded.
Remaining resource counts do not grant permission, authorize replay or prove that
an interrupted external process stopped. Actual continuation remains unimplemented.

## Runtime deadline enforcement

Engine wall time now starts before model pinning and setup, and Session receives
the same monotonic deadline. Expired setup finishes exhausted without polling
execution; ready cancellation retains priority. Session independently refuses new
tool admissions, reasoning cycles and inference after its deadline. Its deadline
can only be tightened, and each inference timeout is capped by remaining task time.
Reservation writes that consume the remaining time retain their charges while
preventing subsequent tool/model dispatch.

This prevents setup or repeated Session entry points from adding execution time.
It cannot preempt synchronous Rust callbacks, database I/O, filesystem operations
or blocking custom backends, and shutdown/final audit writes may outlast the
execution deadline. Monotonic Instant values cannot be restored across processes;
remaining recovery wall time stays unknown. No clock-based resume authorization
or hard real-time guarantee is implied.

## Immutable reasoning input snapshots

Before planning/solving, Session writes reasoning_context version 1 with the exact
original objective and assembled reference/advice context, their SHA-256 hashes,
the initial prompt-base hash and prior call/cycle charges. Source data remains
explicitly untrusted and grants no restorable permissions. The snapshot retains
inputs even if documents or registries later change. Interface/expert inference
may already have occurred while assembling context; this is not their prompt log.

Embedded inputs are bounded to a nonempty 12,000-byte objective and 96,000-byte
context before snapshot serialization, with the existing 256 KiB serialized audit
ceiling. JSON escaping can reach that ceiling even below raw input limits; failure
prevents subsequent planner/solver dispatch. Actual inference prompt limits still
apply independently. The snapshot contains the reference content sent to reasoning,
so it is private task history subject to the normal storage/retention policy.

Inspect it with task/task-events. Hashes establish local consistency, not owner
resistance or external provenance. Older tasks cannot reconstruct missing context
from mutable references. This supplies durable inputs for future continuation but
does not restore graphs, authorize replay or establish remaining wall time.

## Recover a provable saved answer

`cogfabric --json recover-verified-result TASK_UUID` can finish an abandoned
running/interrupted `sort:` or `order:` task using its latest saved solver answer.
It obtains exclusive runtime ownership, then validates the original inference
ledger and checks the answer against the original task criterion in one immediate
SQLite transaction. All inference reservations must have correlated results and
a versioned ledger must prove a remaining verification-tool allowance. Legacy,
malformed, over-budget, unresolved and already-terminal cases fail closed.

The fresh deterministic check consumes one original tool allowance. Correct
answers atomically become completed with a verification/recovery audit. Incorrect
answers leave task status/answer unchanged while retaining the check's tool charge
and audit; the JSON report says verified false. Audit-write/deadline failures roll
back the whole transaction. Repeating successful recovery refuses the now-terminal
task. Recovery has a five-second SQLite computation deadline with the same I/O
caveats as recover-tasks, and publishes typed recovery progress after commit.

This is an explicit operator reconciliation check, not restarted inference or a
fresh task wall allowance. It grants no model permissions and runs no shell or
external code. No graph nodes are replayed or declared verified, and unknown
original provider/resource metrics remain unknown. The full original objective
is checked even if the saved solver result came from a subgoal. Only satisfying
that original explicit criterion permits completion. General free-form answers,
missing results and automatic graph continuation remain unsupported.

## Per-subtask scheduling cost records

New scheduler executions write `subtask_resources` events alongside terminal
advisory node transitions. Each bounded metadata record identifies the node and
state, reserved call/output/prompt-byte deltas, observed inference and elapsed
milliseconds, optional provider-token and broker queue-wait deltas, and explicit
`result_verified: false`. Inspect them with task/task-events.

Skipped pending nodes have zero reservations and unavailable elapsed/provider
metrics. A cancelled running node retains its reservations and measured pending
inference time while missing provider usage stays null. Completed advisory
inference is distinct from a verified result. A cost-audit failure halts subsequent
advisory dispatch rather than silently dropping the measurement.

Deltas are taken from the original Session counters; no resource budget is reset
or refunded. If cumulative usage or queue metrics were already incomplete, local
deltas also remain unavailable. These are observations for future scheduling
analysis, not a utility predictor, restored budget ledger, parallel scheduler or
permission to resume an interrupted graph. Existing versioned reservations remain
authoritative for recovery admission; legacy traces can lack these records.

## Same-boot remaining wall-time evidence

New execution settings optionally include a versioned Linux `CLOCK_BOOTTIME`
snapshot and kernel boot UUID. Engine captures it before model pinning/setup and
passes the original start into Session; standalone sessions capture their own
start. The snapshot's nanosecond allowance must equal the original validated
configuration, with strict clock/version/UUID/field checks.

`inference-audit` now reports `remaining_inference_admission.wall_time` for these
snapshots. On the same boot, `remaining_ns` decreases against the kernel clock and
saturates at zero; `expired` is explicit. Another boot yields `different_boot`
with null remaining time, and unavailable kernel observations remain unavailable.
A backwards clock observation fails inspection. Legacy/unsupported-platform
snapshots retain null wall-time evidence. No civil timestamp, PID age, new process
start or configuration reload grants additional time.

CLOCK_BOOTTIME includes machine suspend, so this recovery observation can expire
more conservatively than the live process-local monotonic timer. The snapshot is
local evidence, not clock attestation or protection against an owner rewriting the
database, a changed time namespace or a compromised kernel. Restoring a database
onto another boot does not make its deadline portable. Fresh clocks cannot replace
missing legacy evidence.

This closes same-boot elapsed-time reconstruction only. Task state, original
permissions, immutable context/dependencies, all reservations and outstanding
outcomes still require validation before any continuation. `resume_authorized`
remains false; no inference or graph nodes are replayed by inspection.


## Explicit advisory graph continuation

`cogfabric --home STATE resume-task TASK_UUID --human-approve` can continue a
recorded advisory graph before the original whole-goal solver starts. Direct
execution holds an exclusive runtime lease throughout. Managed-worker admission
converts to a shared execution lease inside the SQLite write transaction, before
commit or inference. A competing exclusive owner cannot commit recovery during
that transaction; failed lease conversion rolls admission back. Shared ownership
then prevents reconciliation while allowing the new worker to hold its own lease.
Admission uses one immediate
transaction with a five-second SQLite progress deadline. The saved context hashes,
chronological call anchor, planner output, graph definitions, completed-node
results and original inference/resource ledger must agree. Unresolved calls and
running nodes are refused; completed advisory nodes remain unverified data.

Only Full/Collective tasks using the original default direct inference module are
currently supported. Registered direct models additionally require an enabled
original version whose validated manifest/checksum exactly matches the task
snapshot and whose endpoint, model and output limits match the saved settings.
Changes or missing versions fail admission; no newer version is selected.
Custom module versions,
conversation references, explanation synthesis and graphs already in whole-goal
reasoning are refused. A same-boot original deadline with positive remaining time
is required. Rebooted, expired and legacy clocks never receive a new allowance.
The restored backend uses the original saved configuration; this is no attestation
that the provider still serves identical weights. Fresh kernel authorization is
constructed rather than restored from model text or saved permissions. Compiled
core verification is used; historical checker expert metadata is not reselected.

Completed steps are reused, pending steps run under the original remaining calls,
output/input reservations and tool limits, then normal whole-goal solving and
independent verification proceed. If remaining decomposition is unaffordable, the
existing budgeted whole-goal fallback applies. Reports expose cumulative charges,
while inference/queue timing covers this continuation only. Unavailable prior
provider usage stays unavailable. Final task status, report and typed outcome are
committed atomically. Cancellation preserves dispatched reservations; failed
persistence never retries an external request. Interrupted or repeated continuation
is not generally supported: new context snapshots, failed nodes or unresolved
outcomes require inspection and fail admission rather than being replayed.

Tests cover real local HTTP continuation with default and registered models,
disabled/missing/tampered registrations, reuse without repeated planner/completed
steps, cumulative charges, leases/operator approval, corrupted graph/context,
unresolved calls, expired/unknown clocks, unsupported backends, cancellation and
terminal audit rollback. The HTTP server is a deterministic fixture, not a real
model-quality evaluation. General graph recovery remains partial.


Managed continuation launches a fresh authenticated same-binary worker, not a saved
PID. It requires the canonical `fabric.sqlite3` database in a private workspace.
The new worker grant receives remaining calls, output reservations and prompt bytes;
its whole-second session bound rounds remaining time upward while the host enforces
the original precise deadline. Existing explicit process shutdown closes transport,
waits and reaps the child on success, cancellation or inference failure. Cleanup
may take its bounded grace period after the inference deadline. This neither
replays unresolved work nor proves an external GPU server stopped. Old interrupted
worker audit sessions remain available for explicit `recover-workers` inspection.
CLI tests exercise actual resumed worker success and cancellation during HTTP,
assert child reaping and preserve unresolved cumulative charges. General later-phase
and repeated recovery, custom executable backends and stale-directory cleanup
remain unfinished.


## Solver-cycle boundaries

The runtime now persists a bounded `reasoning_boundary` before each new solver
cycle and before charging that cycle. It records hashes of the original objective,
context and prompt base, the exact truncated strategy used by the next prompt,
the previous full answer, repeat-detection hash and cumulative call/cycle/tool/input/
output reservations. Checkpoint persistence failure prevents the next reservation
and model dispatch. These are runtime state and untrusted answer/advice bytes,
never restored permissions or independent proof. Each record is at most 256 KiB;
there are at most the configured cycle limit (maximum 16) per execution session.
This adds bounded database writes/hashing, without extra inference calls.

`resume-task` also accepts an unconsumed `ready_solver` boundary after a completed
advisory graph. All graph results must be recorded and still correlated to the
original planner and advisory calls. Its resource anchors and context hashes must
match the current original ledger, its prior answer/hash must match the latest
solver result, and no subsequent cycle/tool/model reservation may exist. Restored
cycle counts and repeat detection prevent a new recursion allowance or an extra
revision after a repeated rejected answer. Normal verification charges apply.

This extends the earlier pre-solver-only limitation. It does not resume an arbitrary
instruction inside a cycle, replay a consumed boundary, repair unresolved calls,
handle tasks lacking a recorded graph, or support general repeated continuation.
A process interruption after a new reservation still requires outcome inspection.
Historical tasks without boundaries retain their earlier supported recovery paths.
Tests cover boundary-write failure, exact revision state, altered fields, consumed
boundaries, repeat stopping and resumed verification through fixture HTTP.


### Whole-goal and skipped-plan boundaries

Unconsumed solver boundaries also support tasks without an advisory graph, including
FAST execution and malformed-planner fallback, and graphs whose pending steps
were explicitly skipped by budget admission. The same context/ledger/deadline and
repeat-state checks apply. Missing graph data is refused when a recorded planner
result parses as a valid plan; absence alone never authorizes reconstructing or
ignoring a plan. Skipped graphs require an explicit solver boundary and never
execute their skipped nodes during this recovery path. Tests cover real HTTP
whole-goal revision, FAST zero-prior-call admission, budget-skipped plans, deleted
valid graphs and consumed boundaries. This supersedes the no-graph limitation
above. Within-cycle and general repeated continuation remain incomplete.


### Repeated saved-boundary recovery

A task may resume again at a later unconsumed solver boundary. Every assembled
context must preserve the first context's objective, reference bytes, hashes and
data/permission markers exactly. Its call and cycle anchors must equal the actual
chronological reservations before that event. The first context and original
settings/deadline remain authoritative; subsequent assemblies grant no new budget.
Admission bounds this selected history to 85 records, 8 MB and at most 17 contexts
(original plus up to 16 additional assemblies). Excess history fails closed.

Regression tests resume through two boundaries into the third original cycle,
verify cumulative calls and one settings record, and refuse changed context,
wrong chronological anchors, exhausted cycles and excessive assemblies. This
supersedes the blanket repeated-recovery limitation above. Repeating recovery
inside consumed cycles or with unresolved inference remains unsupported; arbitrary
within-cycle repair is not inferred from duplicate context acceptance.


### Recovering a recorded failed solver answer

`recover-verified-result TASK_UUID` still performs a freshly charged independent
sort/order check without inference. When the answer fails, its report now includes
`revision_ready`. A true value means a new bounded solver boundary was saved from
that rejection; it does not mean the task succeeded or that inference restarted.
The operator can subsequently use `resume-task TASK_UUID --human-approve`.

Admission requires the latest recorded inference to be that whole-goal solver,
its cycle to match the charged cycle ledger, remaining calls/cycles/output and a
future verification allowance, and no repeated rejected whole-goal answer. The
candidate boundary is checked through normal continuation admission, including
context, original deadline, backend version, graph and permission boundaries.
Unsupported/expired state retains the failed verification charge and reports false
without retaining a boundary. Checkpoint-write failure rolls back the complete
verification transaction. Existing successful result recovery is unchanged.

Tests model interruption after a solver result but before verification, then
prepare and execute the next revision through fixture HTTP without repeating that
solver. They also verify repeat-stop refusal and atomic checkpoint-failure rollback.
This adds a specific within-cycle reconciliation path. Unknown inference outcomes,
arbitrary critic/reflection positions and general within-cycle recovery remain
unfinished; no missing provider result is invented or replayed.

### Arithmetic and SQL worker audit recovery

`recover-workers` now also reconciles `tool_worker` sessions whose `started`
record has no matching `closed`, `failed` or `interrupted` terminal record.
It uses the same exclusive runtime lease and immediate transaction as inference
recovery; the 10,000-session and five-second progress limits cover the combined
batch. A live arithmetic/SQL worker holds a shared lease and blocks recovery.
Task answers/statuses and existing authorization/charge events remain unchanged.
Recovery adds metadata-only `interrupted` events with module/version/capability,
explicitly unknown resource completion and external-effect stopping. It does not
refund calls, replay SQL/arithmetic, kill recorded PIDs or remove temporary files.

Metadata requires valid task/session IDs, supported capability names and module
versions, and a zero initial call charge. New tool audit records use metadata
format 1; the original records without a format field remain recoverable. Unknown
explicit formats fail closed. Payloads over 4 KiB are rejected before copying them
into Rust. Malformed metadata or any audit failure rolls back the entire mixed
batch, including inference changes. Closed sessions stay untouched and repeated
recovery is empty. These are audit repairs under cooperative ownership, not proof
that arbitrary external effects stopped after a host crash.

## Reviewed worker socket cleanup

`worker-files-preview --after 0 --limit 128` returns a checksum-bound JSON plan.
Save the JSON with `--json`, inspect its entries, and use
`worker-files-apply PLAN.json --human-approve` to apply it. Both commands require
exclusive persistent runtime ownership and the human-only `worker.cleanup`
capability. The plan binds the workspace lease inode, journal page, task/event
anchors, directory/socket device/inode identities and observed socket presence.
Apply reconstructs the preview and rejects changed plans before deletion.

Managed inference/arithmetic/SQL launch now journals socket identities before
spawn. Cleanup accepts only those recorded UUID directory names under root-owned
sticky `/tmp`, owned 0700 directories and matching owner-only 0600 sockets. It
opens directories without following symlinks, inspects their stable descriptors,
rejects extra files, and uses descriptor-relative removal. It does not recursively
remove files, kill a PID or replay a task. An already-missing socket requires a
fresh preview and permits removal of only the matching empty directory.

Pages cover at most 128 journal records, including normally removed resources.
Continue with returned `cursor` while `more` is true. Metadata copying is bounded
to 4 KiB per journal; plan-file reading to 256 KiB. SQLite progress and per-entry
checks use a five-second ceiling, with filesystem/I/O and busy waits remaining
soft limits. Linux procfs is required for stable-descriptor directory inspection.

Intent must persist before deletion and identities are checked again after that
audit. Removal and its result audit are separate operations: filesystem changes
cannot roll back with SQLite. Failure may leave some entries removed, an empty
directory, or an intent without a result audit. Inspect task history and create a
fresh preview; never infer deletion success from an intent. A repeated stale plan
fails, while a fresh preview after successful cleanup is empty.

A real child-process crash regression proves kernel lease release and subsequent
reviewed cleanup; negative tests cover leases, checksum/workspace mismatches,
symlinks, regular/extra files, modes, inode replacement, malformed journals and
audit failures. This supersedes earlier statements that all stale-directory cleanup
is unfinished. Older unjournaled resources and crashes between directory creation
and journal persistence remain manual work. This cooperative same-owner boundary
does not defeat a malicious same-user process racing filesystem operations, and
cleanup does not prove an external inference server stopped.

Managed continuation also retains the original optional provider reasoning effort
from saved task configuration. Actual CLI/worker regressions change current host
settings after interruption, then assert original `none` is sent rather than the
new `high`, on success and cancellation. The latest operator configuration is
not overwritten. This extends immutable settings evidence; it does not attest
provider compliance or enable replay of unknown inference outcomes.

### Reasoning prompt format compatibility

New reasoning-context audit snapshots declare `prompt_format=quoted-references-v1`.
Recovery rejects unknown or null declared formats before dispatch and still checks
the exact saved prompt hash against reconstructed immutable objective/context.
Older snapshots without a format declaration remain eligible only when their hash
matches the current representation; metadata absence never bypasses validation.
Repeated compatible snapshots may add this declaration without changing source
bytes, budgets or permissions. Older nonempty unquoted-reference hashes are refused
with an explicit unsupported/altered-prompt error. There is no automatic conversion
to a different prompt or fallback to the earlier injection-vulnerable representation.
An operator must start a separately reviewed new task when formats are incompatible.

## Compiled role prompt identity

New execution settings record `runtime_role_prompts_sha256`, computed from the
versioned canonical list of local-adapter role/system prompts, including the
shared untrusted-reference boundary. The same construction supplies actual local
HTTP system messages. Continuation requires the recorded fingerprint to match
current compiled defaults before any dispatch or new reservation. Missing, null
or mismatched fingerprints fail closed. Older tasks remain inspectable and retain
their charged history; there is no automatic fingerprint migration or budget
reset. Audit reconciliation and independently checked result recovery remain
separate operations.

This identifies compiled default instructions, not provider behavior, tokenizer,
weights or arbitrary trusted-host backend implementations. It grants no new
permissions. A malicious same-owner database writer can replace hashes; this is
cooperative version consistency, not executable or database attestation.

### Uncertain native residency requests

Use `residency-inspect TASK_UUID --api-base NATIVE_BASE` to compare a bounded
recorded operator intent with fresh inventory. This observes desired state while
leaving the original task and unknown outcome unchanged. Matching instances do
not prove this framework caused the state or owns the instance. There is no
automatic retry/unload or migration; invalid/ambiguous intent fails before HTTP.
See MODELS.md for report semantics and the separate credential scope.

## Reconcile saved advisory inference after a crash

A crash can occur after the charged solver result is durably recorded but before
the running subtask stores its advisory result. For this specific boundary:

```sh
cogfabric --home STATE --json recover-advisory-result TASK_UUID --human-approve
cogfabric --home STATE --json resume-preview TASK_UUID
cogfabric --home STATE resume-task TASK_UUID --human-approve
```

Recovery acquires an exclusive runtime lease and requires one running node in an
abandoned/interrupted graph. The latest charged inference must be its recorded
cycle-zero solver result, chronologically after the node started. The complete
prospective checkpoint is validated with the same original settings, model/module
snapshots, prompt fingerprints, dependency schedule, remaining admission and
same-boot deadline as continuation. Missing/uncertain inference, changed history,
unsupported snapshots, expired clocks and live runtime leases are rejected.

One transaction links at most 1200 characters of existing advice as **unverified**
and records a metadata-only operator audit. Failure of either write or validation
rolls back both. No inference/tool is replayed, no charge refunded, and neither
permissions nor wall time are renewed. Task status/answer remain unchanged;
continuation still needs separate approval and revalidation. A repeated recovery
finds no running node and fails without another transition. The report excludes
advice text and includes its stored-result hash. This is not reconciliation of an
unknown provider outcome or recovery inside a consumed solver cycle.

## Saved graph with interrupted initialization

`resume-preview` now admits a saved validated planner result and graph when none
or only some initial pending nodes were persisted before interruption. It reports
`initial_nodes_to_record`, preserving the observed `not_recorded` states and
performing no writes. `resume-task --human-approve` revalidates the original
snapshots, schedule, reservations and aging deadline, then records only missing
pending transitions in the same transaction as continuation admission, before
constructing any backend. A write failure rolls back the complete admission.
The planner is never replayed. This requires all nodes to be either pending or
not recorded; missing initial records after any advanced node state fail closed.
Actual local HTTP regressions cover zero and partial initialization with a custom
saved order and unchanged total charges. Earlier blanket refusal of initial
missing node records is superseded by this narrowly validated path. General
unknown-result or consumed-cycle continuation remains unsupported.

## Recorded reflection before its next solver checkpoint

After a crash following a fully persisted solver, critic and reflector sequence:

```sh
cogfabric --home STATE --json recover-revision-result TASK_UUID --human-approve
cogfabric --home STATE --json resume-preview TASK_UUID
cogfabric --home STATE resume-task TASK_UUID --human-approve
```

Reconciliation requires consecutive correlated calls in the current charged cycle,
a strict critic request for useful revision, and the prior consumed solver boundary
with consistent stopping state. It records the truncated reflection as next-solver
strategy and the solver answer as previous unverified answer, preserving all
charges. Existing full checkpoint admission validates context, model/module pins,
role prompts, schedule, original same-boot deadline and remaining allowances.
Boundary and metadata-only operator audit commit atomically under an exclusive
runtime lease; any failure rolls both back. Repeating recovery fails after the
boundary has been recorded. Resume remains separately approved and revalidated.

No solver, critic or reflector inference is replayed. Missing/unknown outcomes,
non-admitting or malformed critiques, changed records, expired state, live leases
and completed tasks are rejected. Structured objectives use independently checked
result recovery instead. Real local HTTP tests exercise the one-new-solver path,
which remains unverified. This closes the completed-reflection crash window;
it does not implement arbitrary within-cycle recovery or provider stop proof.

### Recorded critic before reflection

The same approved `recover-revision-result` command accepts a fully recorded
solver/critic pair when reflection has not been dispatched. It records
`ready_reflector`, preserving the exact validated critique reason and prior answer.
`resume-preview` exposes that phase. Admission requires both reflection and revised
solver to fit the remaining original call/output/prompt budgets and deadline.
`resume-task --human-approve` invokes the missing reflector, then the revised
solver without redoing the recorded calls. Strict critique identity is rechecked
at resume; altering its reason invalidates the checkpoint.

If another crash occurs after the recovered reflection is recorded, the command
can reconcile that result into `ready_solver`. If reflection was dispatched but
its result is absent, the ledger rejects recovery; no outcome is inferred, call
refunded or inference retried. HTTP and second-interruption regressions cover
these cases. Earlier statements excluding all critic-only recovery are superseded
by this checked phase. Other arbitrary consumed-cycle positions remain unsupported.

### Recorded whole-goal solver before critique

For an unstructured DEEP/RESEARCH task with a fully recorded solver result,
`recover-revision-result TASK --human-approve` can now produce `ready_critic`.
It requires a consistent consumed solver boundary, a new answer hash (rather than
a repeated no-progress answer), the original validated snapshots/deadline, and
sufficient original budget for critic, reflection and revised solver. Structured
objectives retain their separate independent-checker recovery path.

After separate resume approval, only missing calls execute. The critic uses the
saved candidate. A valid admitting critique proceeds to reflection and solver;
a non-admitting or invalid critique stops with the original answer unverified,
without buying the two remaining calls. No independent verification is invented.
If another interruption follows a recovered critic, its durable result can be
reconciled to `ready_reflector`; an unresolved reservation is never replayed.
Actual local HTTP, repeat-interruption and negative budget tests cover these paths.
This supersedes the blanket exclusion of solver-before-critic recovery, while
general unknown-outcome and other-boot continuation remain unsupported.

### CLI and same-image process validation

`tests/resume_worker.rs` covers approved revision reconciliation, preview and resume
for saved solver, critic and reflector results through the real CLI and managed
inference worker. Each success and SIGINT case checks original settings, durable
calls, one worker launch and child reaping. Interrupted provider results remain
unknown and charged. These use deterministic local HTTP fixtures, not live models.
Run the same tests against an optimized installed CLI from this source:

```sh
COGFABRIC_TEST_INSTALLED_BINARY=/absolute/prefix/bin/cogfabric \
  cargo test --offline --test resume_worker
```

This is a test-only override. The Rust integration harness is built from the
checkout; CLI and child process use the selected executable. Record both source
revisions when they differ. It does not remove the need for a toolchain to compile
the harness or constitute a clean-OS installation study.

## Executive escalation policy identity

New reasoning tasks record the versioned escalation policy descriptor. Continuation
accepts matching built-in budget-policy snapshots and rejects unavailable custom,
changed-version or spoofed snapshots before any resumed inference. Repeated
matching built-in snapshots do not reset budgets. Older records without this
snapshot predate the replaceable-policy interface and use the built-in compatibility
path with current hard admission. Recovery never loads policy code from metadata
or silently replaces a pinned custom policy. Custom-policy reconstruction and
implementation attestation remain unsupported; see `ESCALATION.md`.

### Explicit executive policy restoration

The default API retains its unavailable-custom-policy denial. Trusted host callers
can explicitly supply the original native policy through
`continuation::preview_with_escalation_policy` and `resume_with_escalation_policy`.
The CLI supports the compiled empirical policy with paired `--retry-study` and
`--retry-thresholds` options on preview/resume. See [ESCALATION.md](ESCALATION.md)
for exact identity, original-allowance, source and budget validation. Source drift,
missing policy/decisions, mismatched thresholds and unavailable implementations
remain denials. No previous inference, policy decision or resource reservation is
replayed or refunded. Supplied native handles cannot claim legacy compatibility.

`resume-task` checks approval before reading inputs and uses an existing-only
current-schema Store writer, preserving the schema and avoiding bootstrap or
new workspace creation. This supersedes the blanket exclusion of all explicitly
restored custom handles above; general custom-policy reconciliation, code
attestation and automatic provisioning remain unsupported.


### Original shared resource pools

Continuation now refuses any checkpoint with a recorded `resource_intent`. The
native shared allocator retains uncertain dispatched reservations, and recovery
cannot safely recreate that process-local ownership or silently omit its limits.
Preview and approved resume reject before backend construction without changing
task/event state. Explicit pool restoration and crash reconstruction remain
incomplete; see [RESOURCE_SCHEDULING.md](RESOURCE_SCHEDULING.md).


Configured resource admission is now also an original execution-setting boundary:
continuation rejects it even if no `resource_intent` was recorded before a crash.
This prevents a pre-inference checkpoint from omitting its configured pool.
Separate `resource-status` / `resource-release ID --human-approve` commands inspect
and reconcile abandoned reservation metadata under exclusive leases; they neither
resume reasoning nor recreate a task budget. Configured startup refuses unresolved
dispatches, and reviewed retention preserves that evidence until completion/release.
See RESOURCE_SCHEDULING.md for the exact bounded operator workflow and limitations.


### Configured resource-pool restoration

The earlier blanket configured-pool refusal is superseded for supported ready
checkpoints. Preview and approved resume now validate original resource call chains
and resident coverage, preserve the original configuration/charges/deadline, and
restore or reuse the matching compiled pool under runtime/resource ownership.
Restoration occurs outside Store locks; admission revalidates in a transaction
before any provider or worker is constructed. Unresolved calls, malformed/missing
records, changed bindings, closed/live conflicting pools and unavailable native
configurations fail closed. No executable, new task budget, model load or provider
stop proof is inferred. A separately approved resource release cannot reconstruct
an uncertain answer. See RESOURCE_SCHEDULING.md and tests/resource_resume.rs for
exact ordering, legacy-empty handling and direct/managed/graph integration scope.

## Lease-acquisition diagnostics and fork inheritance

Every recovery entry point retains the original acquisition error under an
operation-specific context. Callers can inspect the underlying `std::io::Error`
for actual contention (`WouldBlock`), an OS failure such as a refused symlink, or
the deterministic private-file validation failure. A failed open or unsafe lease
file must not be diagnosed as proof that an engine is running. All such failures
still stop before recovery writes, backend construction or inference replay.

Close-on-exec descriptors are inherited by `fork` until the child executes another
image or exits. A forked child can therefore briefly retain a shared lease after
the parent closes its copy. Preview is not an ownership reservation: a later resume
must acquire its own exclusive lock and may independently refuse. Do not unlink
the lease, explicitly unlock a surviving shared description, bypass the refusal or
infer provider termination from it.

`tests/lease_inheritance.rs` deterministically holds a forked child's inherited
close-on-exec lease. Recovery must refuse without changing the task, then succeed
only after the child exits and is reaped. The child uses only async-signal-safe
syscalls after fork; the test has its own integration process to avoid inheriting
other parallel fixtures' descriptors. The test does not prove external inference
has stopped.

Resource-resume tests exclude their CLI-launch case from concurrent in-process
fixtures asserting immediate acquisition. Those fixtures remain parallel with each
other. This test-only launch gate removes cross-fixture fork inheritance without
changing production locking or adding retries to assertions. A prior intermittent
failure was captured as `EWOULDBLOCK` with no holder visible by the time descriptors
were inspected. The controlled test establishes the inheritance mechanism; the
historical child PID was not established.

## Candidate recurrence and rejected continuations

Continuation reconstructs first-per-cycle whole-goal candidate history from the
already validated durable nodes. No new checkpoint field or migration is needed;
records lacking the new `candidate_progress` event remain usable if their original
history and all other admission checks pass. An already recurrent history cannot
restart a no-progress search. Revision-result reconciliation runs this same guard
inside its transaction; refusal rolls back the prospective boundary and audit.

Successful resumed independent verification remains `completed`. A failed
independent criterion now remains `rejected`, rather than becoming `unverified`;
continuation reports expose `verification_attempted` and `stop_reason`. General
prose remains unverified. Original deadlines, resource charges, model/context pins
and permissions still apply. Unknown outcomes and unsupported crash windows are
not automatically finalized or replayed. These checks do not prove provider stop.

`tests/candidate_convergence.rs` covers actual local HTTP continuation across a
nonadjacent recurrence and transactional refusal to restart a recurrent prose
search, including legacy records without progress events.
