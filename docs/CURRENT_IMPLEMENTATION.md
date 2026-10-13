# Current Rust implementation

This is the implementation snapshot, not the target specification. The complete
French vision is preserved in `ARCHITECTURE.md`. The branch is `develop/v1.0`,
package version `1.1.0-alpha.1`; it remains a pre-release despite its historical
version numbering. Rust 2021 and Tokio own the runtime; Python is offline only.

The seven responsibilities currently map to these modules:

| Layer | Implementation |
|---|---|
| Language Interface | `language.rs`, `backend.rs`, `config.rs`, `agent/`: advisory intent, explicit history, synthesis, loopback inference and bounded native conversation turns |
| Jev+ | `engine.rs`, `planning.rs`, `routing.rs`: task lifecycle, replaceable trusted expert selection, capability gaps, bounded dependency scheduling |
| Jev-R | `reasoning.rs`: shared-budget model roles, revisions, stagnation detection and independent verification status |
| Expert Network | `registry.rs`: immutable reference manifests, dependencies, lifecycle and task version snapshots |
| Cognitive Fabric | `wire.rs`, `service.rs`, `store.rs`, `agent/protocol.rs`: typed owner-local IPC, durable state/evidence and SQLite/FTS5; optional `crates/cogfabric-server` HTTP transport builds on the core |
| Evolution Engine | `evolution.rs`, candidate storage: quarantined immutable artifacts and explicit lifecycle |
| Security & Evaluation Kernel | `security.rs`, `evaluation.rs`: task-bound dispatch authorization and independent deterministic gates |

Inference now traverses a bounded FlatBuffers Tokio broker with runtime-owned
registration, discovery, actor/task validation and pinned adapter versions. Other
layer interactions use direct Rust interfaces. Typed task progress now supports bounded local subscriptions and durable catch-up.
General module events and independent process registration/authentication remain incomplete.
Reference experts are functional; they are not loaded neural specialist models.

A task snapshots active skill metadata and retrieved content before inference.
Planner JSON may describe up to four acyclic dependent subgoals; ready nodes run
sequentially under shared ceilings, followed by an original-goal solver. Invalid
or unaffordable plans fall back to whole-goal execution. Critic advice can trigger
bounded reflection/revision. Model claims and subtask answers remain unverified.
The arithmetic parser provides an independent verified execution criterion.

The local service authenticates the owner UID, caps pending connections at eight,
and permits one active task. Typed controls can query status, cancel current work,
or stop/join the service. Active task UUIDs enable targeted cancellation and
bounded progress pages over the socket. Socket disconnects also cancel work durably. It is a
single-user local service, not a multi-tenant sandbox.

SQLite migrations preserve alpha data through schema 6. RAG candidate imports use
content-addressed quarantine, explicit provenance, quotas and integrity checks.
Quality requires operator-attested held-out data. Promotion checks quality,
cumulative approved-version retrieval regressions and unchanged versions transactionally.
Rollback restores approved stored snapshots. Neural promotion stays disabled.

Verification: 164 tests, formatting, all-target check and Clippy pass using exact
flatc/Rust FlatBuffers 25.12.19. HTTP fixtures and socket tests ran locally; no
compatible live model server was available at the default endpoint. No neural
quality, GPU integration, hot-swap, energy or model-superiority result is claimed.
See `IMPLEMENTATION_MATRIX.md` for every remaining engineering requirement and
`QUALITY_REVIEW.md` for actual command evidence.

Optional managed worker mode now launches the same Rust executable lazily per
task, provisions authenticated typed settings and closes/reaps it on completion
or cancellation. Three actual CLI integration tests cover demo role execution,
zero-process arithmetic and Ctrl-C during pending HTTP inference. This is process
coordination, not an executable sandbox or model-loading proof. See `WORKERS.md`.

Persistent workers also hold shared runtime leases. Operator-only recover-workers
marks open audit sessions interrupted under exclusive ownership, without task
result changes, PID killing or inference replay. Tests include an actual child
crash and transactional rollback; stale temporary-directory cleanup remains open.

`benchmarking.rs` now runs explicit execution ablations under common operator
ceilings, rotates arm order, excludes expected answers from inference, records
actual resource totals and cancels durably on a run deadline. The diverse study
dataset and live-model comparison remain outstanding.

Operator-published episodic memory and independently recomputed arithmetic snapshots
are retrievable with provenance, hashes, revocation and bounded payload storage.
See `MEMORY.md`; arbitrary model output remains unverified.

Persistent Engine tasks hold kernel execution leases. Operator `recover-tasks`
marks abandoned running tasks interrupted in an audited transaction; it refuses
live leases and never automatically replays work. See `RECOVERY.md`.

Immutable local model registrations now record source/license, connection identity,
declared limits and optional unverified weight hashes. Operator selection persists
an exact version reference; task traces pin it and disabled registrations reject
new tasks. Physical model residency remains server-owned; see `MODELS.md`.

Typed progress events use a dedicated FlatBuffers schema, with bounded subscriber
queues, explicit gap reporting and cursor-based durable catch-up (`EVENTS.md`).

SQLite page limits, transactional global task admission and explicit preview/apply
retention bound logical storage growth. Memory/conversation sources are protected;
WAL physical size remains outside a hard runtime quota (`STORAGE.md`).

Comparison arms now share a consistent read-only SQLite retrieval snapshot, with
size/hash/timing recorded in reports and durable task traces (`corpus.rs`). Live
results persist separately; live model availability/weights remain unfrozen.

Explicit `order:` objectives now use an independent bounded Rust constraint checker.
Each check is individually authorized and charged against the tool budget; failed
checks inform subsequent solver cycles. Passing checks produce completed outcomes
and suppress answer rewriting. See `VERIFICATION.md` for syntax and exact scope.

Operator `diagnostics` reports storage/schema/registered-selection checks without
creating a task. Optional bounded local model discovery reports unavailable or
missing model IDs and produces a nonzero CLI exit; it does not perform inference
or attest weights. See `DIAGNOSTICS.md`.

Recorded capability gaps now support bounded operator inspection/preparation and
quarantined gap-guided RAG imports. Candidate metadata and its evidence link commit
atomically after revalidating the recorded gap hash; source tasks are protected from
retention. Automatic source discovery/cleaning remains absent. See `EVOLUTION.md`.

Module brokers measure per-call queue waiting independently of backend reports.
Optional queue timing crosses module/owner IPC and accumulates in task/comparison
resources; cancelled partial totals remain unavailable. GPU/load timing is absent.

Expert manifests now include bounded prompt/workflow checklists. Full/experts
execution can invoke one routed, version-pinned workflow through the shared solver
backend, reserving budget for whole-goal reasoning. Advice is unverified and grants
no permissions. Bounded multi-step and registered-model advice are implemented; executable
experts and measured domain usefulness remain incomplete.

Operator SQLite backups now use a pinned read view, bounded copy/hash loops,
private files and serialized admission. They preserve committed WAL state and
publish a checksummed read-only standalone database. Full artifact/configuration
restore and automatic replay are not implemented. See `STORAGE.md`.

Offline training preflight now has nine standard-library tests, frozen input hashes,
strict provenance/deduplication, a no-supervised-token termination guard, disjoint
new output and bounded adapter inventories. This improves the experimental script;
no actual ML training, isolation, neural evaluation or promotion is proven.

Explicit `sql:` tasks now execute bounded read-only queries over fresh task-local
inline tables without model calls. Task permissions, worker/VM/data/output bounds,
denied external access and hashed verification evidence are covered by tests.
This built-in capability is distinct from dynamically loaded external experts;
see `SQL.md` for its limited verification scope.

Workflow experts now accept bounded acyclic advice-step graphs as an alternative
to legacy text checklists. They execute dependencies with the shared backend and
task budgets, record step identities/hashes and preserve pinned versions. Results
remain unverified advice; measured model usefulness and executable workflows are
not proven.

`restore-database` now restores a saved-hash-verified schema-6 SQLite snapshot into
a new private directory and records provenance. Existing workspaces, corrupt or
live-WAL inputs and incompatible schemas are refused. Running tasks are preserved
without replay; external artifacts/configuration and older-schema migration are
not included.


Comparison reports disclose per-case bound expert model snapshots and flag the
loss of the same-model configuration control, including refused/failed expert
advice. This prevents separately configured experts being described as ordinary
same-model ablations; backend weights remain unattested.

Offline training also requires reviewed hashed train/validation/test split
metadata and rejects normalized prompt overlap before ML imports. Split identity
is recorded, but semantic independence and neural acceptance remain unverified.

SQL comparison cases can use structural JSON result scoring against reviewed
expected labels, preserving cell types and row/column order while ignoring object
field order and whitespace. Status alone cannot make a wrong answer pass.

The evaluation directory now supplies a provisional 100-case authored corpus and
hashed inventory. Ten categories include deterministic tools and ambiguity/policy
reasoning. Independent review, held-out status and live performance remain unproven;
no benchmark accuracy or security result is inferred from format/oracle tests.

Optional decomposition/exploration now preserves the first whole-goal solver's
call, output-token and prompt-byte capacity. Conservative dependency escaping
bounds and audited skips prevent advisory stages consuming its reserved budget.

Optional operator GPU diagnostics use one fixed bounded NVIDIA query and report
whole-device observations separately from task metrics. The actual driver is
unavailable here; live GPU values, energy and task attribution remain unverified.

Operator diagnostics can inspect enabled registered model endpoints with bounded
deduplicated discovery, including potential workflow experts. Advertised IDs are
not loaded-weight or successful-generation evidence. Local model files were found,
but LM Studio's local API startup timed out; real inference remains unverified.

Comparison CLI supports an optional reviewed dataset hash pin, rejecting changed
bytes before execution and recording the operator pin separately from actual
content identity. This does not establish held-out independence or label quality.

A later host-access check succeeded where sandboxed checks failed: actual framework
GPU telemetry sampled a 12 GiB device, and LM Studio loaded an existing quantized
model for a bounded test. Its authenticated HTTP server returned 401 for discovery
and generation; no successful model answer is claimed. The owned test model was
unloaded and the server returned to its prior stopped state. Protected explicit
operator API credentials remain an integration gap; none were accessed or stored.

Opt-in local bearer authentication now binds the dedicated operator environment
credential to one local API scope, including managed workers and discovery.
Synthetic direct/managed generation and escaped-echo regression tests pass.
Live authenticated generation remains unverified; see `AUTHENTICATION.md`.

New database backups include bounded effective configuration under the same
checksum. Restore defaults to database-only; `--restore-configuration` explicitly
validates and creates private settings for the new workspace. External artifact
files and older-schema migration remain separate recovery work. See `STORAGE.md`.

Optional `backup --include-rag-artifacts` and restore `--restore-rag-artifacts` now
preserve checked RAG candidate files with private quarantine and rebased paths.
Candidate status and promotion boundaries are preserved. Neural and unrelated
external artifacts remain excluded; see `STORAGE.md` for scope and failure handling.

Recognized schema-1–5 snapshots now support explicit `--migrate-schema` restoration
through bounded existing migrations on a new staged copy. Source and destination
schema versions are recorded; original files and task state are preserved. Unknown
and unversioned snapshots remain rejected. See `STORAGE.md`.

Trusted embedding hosts can register versioned event-module descriptors and
subscribe to selected progress stages for one task. Handles pin filters/versions
and share bounded FlatBuffers queues with owner observers. Registry state is
process-local; cross-process actor provisioning remains incomplete. See `EVENTS.md`.

Full task trace reads now have event/raw-byte ceilings and strict JSON validation.
`task-events` exposes bounded complete audit pages with durable cursors for larger
histories. Comparison identity reporting avoids full-trace allocation. These changes
support safe recovery inspection without replay; see `RECOVERY.md`.

New reasoning results carry their charged call identity. `inference-audit` reads
bounded durable reservations/results without exposing prompts/answers, and keeps
legacy or unrecorded usage unavailable. It grants no resumption authority; see
`RECOVERY.md` for limits and remaining checkpoint requirements.

Independent structured verification now also supports integer sorting, in addition
to dependency ordering, arithmetic and inline read-only SQL. `sort:` goals declare
one to 256 signed 64-bit integers and optional descending order; Rust checks exact
multiplicity and ordering before verified completion. Failed candidates can be
revised within the existing cycles/tool/call ceilings. This is a bounded domain
checker, not a general correctness evaluator or a measured live-model improvement.

For exact explicit `sort:` and `order:` goals, Full/Memory execution can reuse an
operator-published episode only after a fresh independently charged Rust check.
A valid retrieved answer completes without inference; rejected answers remain
advisory and normal reasoning continues within remaining limits. Unrelated goals,
revoked entries, explicit conversation references and other ablation profiles do
not gain this shortcut. A memory's prior status is never accepted as proof.

`recover-verified-result` now reconciles abandoned explicit sort/order tasks whose
saved solver answer passes a fresh independent check. It requires exclusive
runtime ownership, a fully correlated bounded ledger and remaining verification
allowance, then atomically commits proof/status without repeating inference.
Unsupported or unresolved tasks remain interrupted; general continuation is still
incomplete. See RECOVERY.md for failed-check charges and transactional behavior.

The Expert Network now supports checker manifests binding reviewed expert metadata
to compiled integer-sort/dependency-order verifiers. Existing immutable lifecycle,
dependency validation and task pinning apply; actual proof events record the
selected manifest version separately from the compiled criterion version. No
imported code or new permissions are accepted. See EXPERTS.md.


Explicit `resume-task --human-approve` now resumes validated pre-solver advisory
work using original resource charges and same-boot deadline evidence. Completed
nodes remain advisory; pending nodes and whole-goal verification use fresh kernel
authorization. Default direct Full/Collective tasks are tested through local HTTP.
Managed/registered/custom backends and general multi-phase/repeated resumption
remain unsupported. See `RECOVERY.md`; current mandatory suite has 232 tests.


Registered direct models are now supported by advisory continuation when the
original enabled registry version matches its saved manifest and configuration.
Changed/missing/disabled versions are refused without substituting a newer model.
HTTP fixtures verify the requested model identity; loaded weights remain unattested.


Managed same-binary inference workers now support advisory continuation. Admission
converts exclusive ownership to shared execution inside its write transaction,
then provisions only remaining worker budgets and reaps the fresh child on success
or cancellation. Actual CLI/process/HTTP tests cover both paths. Host enforcement
retains the precise original deadline; worker shutdown has a bounded cleanup grace.
Custom backend versions and later-phase recovery remain incomplete.


Solver cycles now persist bounded reasoning boundaries with previous answer,
revision strategy, repeat hash and cumulative reservation anchors. An unconsumed
boundary following a completed advisory graph can resume without resetting cycles
or repeat detection. Tampered/consumed boundaries and unresolved calls are refused.
No-graph, within-cycle and general repeated resumption remain incomplete.


Unconsumed solver-boundary recovery now also covers FAST tasks, malformed-plan
whole-goal fallback and explicitly budget-skipped graphs. Valid planner output
without its graph record is refused. Actual HTTP and budget/stopping regression
tests pass; within-cycle and repeated recovery remain incomplete.


Repeated recovery at later unconsumed solver boundaries now preserves the original
context/settings/deadline and cumulative counters. Each new context assembly must
match the original bytes and its chronological call/cycle anchors; history is
bounded to 17 assemblies. Tests cover two resumptions, unchanged settings, exhausted
cycles and tampering. Arbitrary within-cycle repair remains incomplete.


Failed structured saved-answer recovery can now report `revision_ready` and retain
an admitted next-solver boundary after a fresh charged independent check. It never
repeats the saved inference or treats rejection as success. Repeated failed answers,
unsupported state and unavailable budgets do not receive a revision; checkpoint
write failures roll back verification charges. Arbitrary within-cycle recovery
and unknown inference outcomes remain unsupported.


Host embedding APIs now deliver task-scoped progress to authenticated separate
processes using versioned COGS/COGE FlatBuffers, pinned module/stage grants and
bounded queues/timeouts. Actual child-process and negative transport tests pass.
Socket/child provisioning remains host-owned; general executable registration,
command routing and automatic IPC replay are still incomplete.


Authenticated process module streams now support COGS v2 durable replay followed
by deduplicated live delivery. Saved cursors advance through filtered pages and
host-retained grants restore task/version/stage scope after registry changes.
Version-one live output is preserved. Receiver frame/time limits and version/cursor
validation are tested; provisioning and general command routing remain incomplete.


### Typed arithmetic module dispatch

`src/fabric/tools.rs` and `protocol/tool.fbs` implement replaceable host-registered
arithmetic modules over verified COGT FlatBuffers request/response frames. Tasks
pin immutable module versions before execution. Registration/discovery, conflict
rejection and unregistration preserve existing pins; shutdown revokes retired pins
and cancels active/queued futures. Four queued requests, 8 KiB frames, 120-byte
expressions, 256-byte answers and a 30-second dispatch ceiling bound this adapter;
the Engine's original task deadline and charged tool allowance still apply.

Only `math.evaluate` from task-scoped `jev-plus` is supported. Registration is a
trusted Rust host API, not an IPC permission grant. The Engine independently
recomputes arithmetic before marking evidence verified; module output alone is
never proof. This adds no inference calls. Backend errors become generic failures.
Service shutdown also stops this broker. These are in-process typed queues, not
an authenticated process tool transport, executable sandbox or general command
registry. SQL and other capabilities remain outside this adapter. General module
command routing/provisioning and executable lifecycle remain **Partial**.


### Authenticated arithmetic process transport

`src/fabric/tools/unix.rs` extends COGT command dispatch to an already connected
Unix socket. Both sides authenticate trusted UID/PID expectations. Immutable,
non-deserializable host grants restrict one task/module/version, 1–64 cumulative
calls and a 1–300 second session. The adapter carries actual broker task identity;
unscoped dispatch is denied. Four-byte big-endian lengths bound verified frames
to 8 KiB; partial frames and writes have two-second deadlines. Each exchange has
at most 30 seconds and both sides enforce their fixed session deadline.

Worker authorization is audited before execution; audit failure prevents dispatch.
Correlation replay, changed tasks/versions, exceeded calls, malformed frames and
pipelining fail closed. Cancellation discards the client stream permanently;
worker disconnect/shutdown/expiry drops pending backend futures. Backend failures
are generic and audits exclude expressions, answers and backend error bodies.
A separate child-process regression executes actual arithmetic. This is a trusted
embedding API: automatic task-grant provisioning, executable launch/reaping,
service/CLI tool-worker setup and other tool capabilities remain incomplete.
Dropping a future is not proof that arbitrary external side effects stopped;
this transport provides no executable sandbox or model attestation. Abrupt worker
termination may omit the terminal audit record. Original Engine tool allowances
and independent arithmetic verification still apply; grants cannot expand policy.


### Managed built-in arithmetic workers

`configure-tool-worker --enabled true` persists `arithmetic_process` (default
false); restart a running service to apply it. Disable with the same command and
`--enabled false`. Direct and socket `ask 'calc: 6*7' --mode fast` now lazily launch
the same Rust binary as an arithmetic worker, using the existing bounded broker
and COGT commands. No inference worker/model call is needed for arithmetic.
The default arithmetic registry is required; custom versions are rejected before
task creation rather than silently replaced. Other capabilities remain in-process.

`protocol/tool_bootstrap.fbs` defines verified COTB v1, bounded to 2 KiB with
two-second read/write deadlines. Both peers authenticate approved UID/PID before
bootstrap. Trusted setup grants exactly one task, built-in module/version and one
arithmetic call; its lifetime is at most 30 seconds and conservatively rounds up
remaining task seconds. The host still enforces its original precise deadline.
The worker opens the matching private workspace database with the pinned quota;
no mutable generation settings, model credentials or inherited environment are used.
Permission/tool-budget admission precedes lazy launch and the host independently
verifies the returned arithmetic answer.

Launch intent, owned PID, bootstrap delivery and confirmed reaping are distinct
audit records. Ownership begins at spawn and survives cancelled/failed handoff;
normal task cleanup closes the connection, waits two seconds, then kills/reaps
with a further two-second limit. Directory/socket cleanup checks inode/device
identity. Cleanup errors fail the task. This also applies after execution errors
and cancellation. Process/runtime crashes can still leave stale files or unknown
cleanup outcomes; no automatic crash cleanup or executable sandbox is claimed.
This supersedes the arithmetic-specific provisioning/CLI lifecycle limitations
above. General executable provisioning and non-arithmetic command routing remain
**Partial / Not implemented**. Process overhead is not a measured speedup.


### Typed SQL capability routing

`src/fabric/tools.rs`, `src/fabric/tools/sql.rs` and `protocol/sql_tool.fbs` extend
the shared tool registry beyond arithmetic. Default `sql.local` 1.0.0 implements
`sql.query`; capability lookup pins SQL and arithmetic separately before execution.
Trusted Rust hosts may register immutable SQL replacements. Active capability
conflicts and changing an active module's capability are rejected. Unregistration
preserves existing pins, while broker shutdown cancels current/retired work.
Registry history remains bounded to 128 versions, with one active module per
supported capability; each module has four queue slots and a 30-second dispatch
ceiling under the original task deadline. Arbitrary capabilities are not accepted.

COGQ v1 carries structured tables/rows/typed cells and structured results, not JSON
strings inside IPC. Arithmetic COGT v1 remains unchanged. SQL frames are bounded
to 256 KiB; verification caps depth at 16, table visits at 25,000 and apparent
bytes at 1 MiB. Decoded cell budgets (32,000 request bytes / 16,384 response bytes)
stop aliased FlatBuffers references from amplifying allocations. Existing input,
identifier, table/row/column, cell and canonical result limits also apply.

The Engine charges `sql.query` before dispatch, records the selected version and
reruns the constrained SQLite checker before publishing verified evidence. This
uses a fresh execution of the same compiled SQLite checker, not an independently
implemented SQL engine. Wrong/malformed module output cannot establish proof.
One authorized command includes both execution and checking, potentially two
bounded SQLite runs (100 ms each), sharing the existing two blocking-worker slots.
No model call or new permission is added. The default backend still uses only
inline task-local data and denies host databases, mutations and unsafe functions.
Registered Rust backend code is trusted host code and is not automatically sandboxed.
SQL process transport/provisioning and broad executable tool loading remain
**Partial / Not implemented**; arithmetic worker setup remains arithmetic-only.

SQL Unix RPC is now implemented as a trusted-host embedding API; see
`PROTOCOL.md` and `src/fabric/tools/sql/unix.rs`. Structured COGQ exchanges enforce
UID/PID, task/version/call/session grants, replay rejection and audit-before-work.
An actual child-process test executes SQLite. Cancelled/malformed exchanges close
permanently and metadata excludes query/result bodies. Automatic SQL process
launch/bootstrap/reaping from CLI/service is still not implemented; the installed
smoke evidence above predates this transport. Current mandatory checks pass with
301 tests (281 library and 20 integration); real inference remains unverified.

Managed SQL launch/cleanup is now implemented for direct CLI and socket service
through opt-in `sql_process` / `configure-sql-worker`. It reuses owned startup and
bounded reaping, adds SQL-only COTB v2 setup, and keeps arithmetic-only v1 intact.
Custom SQL registries and exhausted tool budgets cannot silently launch a default
worker. Actual CLI/service tests verify SQL execution and unsafe-query cleanup;
independent host SQLite verification remains mandatory. This supersedes the SQL
provisioning limitation above. Crash cleanup and executable sandboxing remain open.

Inference worker startup now owns the child/socket/lease before handoff awaits,
records launch intent and owned PID separately, and reaps failed/cancelled startup
through normal bounded cleanup. Retry of an unfinished startup is refused. A real
child regression covers cancellation, ownership-audit failure, retry refusal and
socket-directory removal. Crash cleanup and external GPU cancellation remain unproven.

Worker audit recovery now includes arithmetic and SQL sessions in the same
exclusive-lease transaction as inference. Mixed batches preserve task results and
charges, validate bounded metadata and record interruption without replay/PID
killing. Live tool sessions block recovery; malformed metadata and audit failures
roll back the batch. Legacy unversioned tool audit starts remain supported.
Filesystem cleanup and broader module lifecycle remain incomplete.

Journaled managed-worker socket cleanup now has verified operator preview/apply
commands and actual host-death coverage. It removes only identity-matched sockets
and empty private directories under exclusive runtime ownership; extra files,
symlinks, stale/tampered plans and malformed metadata fail closed. All 318 tests
and mandatory checks pass. See `RECOVERY.md` for pagination, partial audit/filesystem
failures, Linux requirements and unjournaled/hostile-owner limitations. Automatic
crash recovery and broader module lifecycle remain incomplete.

Current verification: 321 Rust tests (300 library, 21 integration), formatting,
all-target checking and Clippy pass. Trusted tool registries now support immutable
version rollback, including SQL, without changing existing task pins. Capability
identity survives unregistration; inactive backend references remain count-bounded
until shutdown. Real direct and managed-worker Ollama generation succeeded, with
independent Rust sorting verification and confirmed child reaping. See
`LIVE_INFERENCE.md` for measured limits; broader live studies and module lifecycle
remain incomplete. Earlier unavailable-inference statements are historical.

Inference brokers now also support host-only rollback to retained immutable
backend versions. Original queues and in-flight role snapshots are preserved;
active role conflicts fail before mutation. All mandatory checks pass with 324
Rust tests (303 library, 21 integration). See `MODELS.md` for retained-resource,
expired-transport and external weight identity limitations.

Transport-bound inference descriptors are now preserved by initial broker
construction and validated during registration. Changed module IDs, versions or
roles fail before inference; actual child-process discovery now reports its fixed
connected scope. All mandatory checks pass with 325 tests (304 library, 21
integration). This is trusted metadata consistency, not loaded-weight attestation.

Event module registrations now retain immutable metadata for host rollback and
reactivation. Future observers use the selected filter; existing live/replay and
process grants retain pinned versions. Active/history ceilings remain enforced.
All mandatory quality checks pass with 327 tests (306 library, 21 integration).
This adds process-local metadata lifecycle, not executable loading or persistence.

Structured goal interpretation now has an operator-selected bounded output
reservation (`interface_tokens`, default 128, 1–4096), configurable through
`configure-local --interface-tokens`. The original cumulative call/token/time
limits still govern admission. All mandatory checks pass with 329 tests. Live
interpretation quality remains separately evaluated; this option does not turn
empty/invalid model responses into success or grant permissions.

Local inference now supports an optional strict provider `reasoning_effort`
override, configurable with `configure-local --reasoning-effort`; default requests
are unchanged. Managed workers preserve it through explicitly versioned COGB v2,
with legacy v1 and malformed pairing tests. All mandatory checks pass with 330
tests. This requests provider behavior; it does not attest hidden reasoning or
prove successful structured interpretation.

Planner admission now preserves the first whole-goal solver against prior
interface/expert charges, checking remaining calls, output reservations and prompt
bytes and auditing skipped planning. All mandatory checks pass with 331 tests.
This corrects the two-call live trial's scheduler defect; valid real-model
structured interpretation is still not established by that correction.

The language interface accepts one exact Markdown `json` fence around the same
strict bounded object, preserving unknown-field rejection and the user's original
goal. Multiple objects, surrounding prose and malformed wrappers remain rejected.
All mandatory checks pass with 333 tests. This handles an observed provider format,
without making advisory intent a permission or proof of an answer.

The corrected bounded real language/solver path now also succeeds in a recorded
STANDARD-mode trial: two calls, accepted advisory structured intent, planner skip,
whole-goal explanation and confirmed managed-child reaping. The explanation stays
unverified. Optimized installation/smoke also pass for this source. See
`LIVE_INFERENCE.md` and `RELEASE_AUDIT.md`; robustness and benchmark gates remain open.

The six-arm comparison runner now supports structural `integer_sort_v1` scoring,
checking exact integer multiplicity/direction without trusting runtime verdicts
or answer whitespace. All mandatory checks pass with 334 tests (313 library,
21 integration). Public sorting examples are demonstration data; live reviewed
comparison remains outstanding. See `BENCHMARKING.md` for shared-verifier limits.

Interface/planner/critic JSON now shares one bounded exact-fence parser, preserving
strict schemas, graph checks and advisory status. Planner graphs and critic revisions
accept the same observed model wrapper as interpretation. All mandatory checks
pass with 335 tests; live recursive quality and gain remain separate research evidence.

Public comparison workflow checks now use the explicit `--workflow-check` flag.
The runner rejects missing/conflicting purposes and reports public results without
held-out review claims. All 336 tests and mandatory quality checks pass. The
existing held-out benchmark gate is preserved; independent datasets and intended
small-model quality measurements remain missing.

Inference version inventories now expose local adapter availability without remote
probes. Permanently closed Unix connections cannot be reactivated or routed;
pinned requests fail before dispatch. Unknown/connected/in-use observations are
not remote health guarantees. All 337 tests and mandatory quality checks pass;
optimized installation must be refreshed after this runtime change.

Arithmetic and SQL brokers now mirror inference local availability handling:
closed Unix streams remain diagnosable but cannot be pinned, reactivated or
queued. Actual malformed-response regressions cover both capabilities. All 337
tests and mandatory checks pass; no remote-health, grant-renewal or reconnection
claim is introduced.

Optimized installation and the full installed smoke workflow passed for source
`bb3c987`, closing the pending installed-source gap for inference/arithmetic/SQL
availability handling. See `RELEASE_AUDIT.md` for binary hash and actual evidence.
Grant expiry and call allowances remain independently enforced; present streams
are not health or renewable-authority attestations.

Unix arithmetic/SQL availability now includes expired/exhausted grants, excluding
these adapters from pinning, activation and pinned dispatch without refund or
renewal. Both actual two-call and paused-clock zero-call regressions pass. All
339 tests and mandatory quality checks pass; optimized installation proof awaits
refresh after this runtime change.

Managed worker wrappers now propagate initialized adapter availability; expired
unlaunched tool deadlines are visible without starting children. Actual socket and
same-binary cleanup regressions cover inference, arithmetic and SQL. All 341 tests
and mandatory quality checks pass; optimized installation refresh remains pending.

Optimized installation and installed smoke passed for source `aa95552`, refreshing
proof after grant-aware routing and managed availability propagation. Exact build
and evidence identities are recorded in `RELEASE_AUDIT.md`. Automatic residency
and general plugin provisioning remain partial; no stable release is declared.

Managed shutdown is now terminal even before first use, with initialization/handoff
coordination before cleanup inspection. Later valid requests cannot launch stopped
inference/arithmetic/SQL workers. All 343 tests and mandatory checks pass; the
optimized installation evidence needs refreshing for this runtime change.

Optimized installation and installed smoke passed for source `99c939f`, refreshing
proof for terminal managed shutdown and its concurrency assertions. See
`RELEASE_AUDIT.md` for exact binary/evidence identities. General lifecycle,
residency ownership and independent held-out evaluation remain unfinished.

Trusted hosts can now register explicitly ranked alternative inference modules for
the same role. New pins choose deterministically among available candidates;
existing pins never migrate or silently retry. Ordinary exclusive registrations
retain overlap denial. All 345 tests and mandatory checks pass; ranks are declared
policy, not learned cost estimates. General provisioning/residency remains partial,
and installed verification must be refreshed after this runtime change.

Optimized installation and installed smoke passed for source `d66dc07`, refreshing
proof after ranked inference routing; see `RELEASE_AUDIT.md` for exact identities.
Arithmetic/SQL capabilities still allow one active module each; explicit ranked
tool alternatives and their active-capacity policy remain to be implemented.

### Ranked tool alternatives (2026-10-12)

Trusted hosts can explicitly approve multiple arithmetic or SQL adapters with
bounded immutable ranks. New task pins select lowest rank then module ID; existing
pins retain original versions. Exclusive registration, transport identity checks,
authorization and rollback remain enforced. Active modules are capped at 32 and
historical versions at 128. Three new regressions pass; the full 349-test suite
passes sequentially. Two default-parallel runs exposed different existing failures
(SQL execution and recovery lease contention); see QUALITY_REVIEW.md. Automatic
plugin provisioning and adaptive cost scheduling remain incomplete.

### Reference instruction separation (2026-10-12)

Nonempty solver reference contexts are JSON-quoted and followed by the original
objective; all inference roles receive fixed untrusted-evidence advice. A live
Qwen attack contaminated answers before the change and with system advice alone;
the combined prompt resisted the same attack once. Answers remain unverified.
Empty-context cost/checkpoint hashes are preserved; earlier nonempty-context
checkpoint hashes fail closed under changed prompt serialization. Full tests now
pass 351 sequentially and in the final default-parallel run; see QUALITY_REVIEW.md
for intermediate failures and LIVE_INFERENCE.md for actual security trial scope.

### SQL concurrency admission (2026-10-12)

Inline SQL now provides bounded backpressure: six admitted requests per process,
two execution slots, one-second queue timeout and cancellation-safe waiting.
Blocking work retains permits after caller cancellation until actual completion.
Input bounds precede admission; read-only inline-data restrictions and existing
SQLite execution limits are unchanged. Managed workers have separate process-local
limits. Dedicated fixtures verify exhaustion, deadline, cancellation and queued
completion without relying on global parallel-test scheduling.

### Typed host tool discovery (2026-10-12)

Local and socket `tool-modules` expose immutable arithmetic/SQL versions, active
selection, routing ranks and availability through append-only typed FlatBuffers.
Legacy replies default to an empty tool inventory. Unknown states, conflicting
registries and excessive inventories fail closed. Borrowed string bounds precede
copying and envelope verification bounds shared-reference traversal. Discovery
starts no workers and grants no permissions. All 358 tests pass, including real
socket discovery; expanded CLI/service smoke passes on the debug binary.

### Inference lifecycle observation (2026-10-13)

`inference-modules` and its socket variant now expose retained immutable adapter
versions, active selection, local availability and ranks through typed, bounded
FlatBuffers. Existing active-only `modules` remains compatible. All 361 tests and
quality checks pass, including real socket and debug CLI/service smoke. Discovery
does not launch inference, grant permissions or attest physical model residency.

### Compiled prompt consistency on continuation (2026-10-13)

New execution settings capture the default compiled role/system prompt fingerprint.
Continuation requires it to match, in addition to the existing context, model,
budget and module checks. Legacy missing or changed fingerprints are not migrated
automatically; read-only inspection and independent result recovery remain
available. All 364 tests and mandatory checks pass; this is version consistency,
not provider or executable attestation.


Inactive inference backend retirement is implemented through trusted-host
`Broker::retire` in `src/fabric.rs`. Active retirement and identity reuse fail;
existing task pins remain usable, while future rollback/routes cannot select the
retired version. `src/fabric/tests.rs` verifies backend release after the last pin
and queue close. This improves runtime handle lifecycle without provider unload,
new grants or general executable provisioning. Mandatory formatting, all-target
check, Clippy with warnings denied and all 366 tests (344 library, 22 integration)
passed on 2026-10-13 with flatc 25.12.19. Test log:
`/tmp/cogfabric-inference-retirement-tests.log`. Optimized installation evidence
predates this new API and must be refreshed before final release.


Tool backend retirement is implemented through trusted-host
`fabric::tools::Broker::retire` in `src/fabric/tools.rs`. Existing arithmetic/SQL
pins continue after retirement, while future selection/rollback and identity reuse
are denied. Metadata tombstones preserve module capability identity even after
all retained backend versions disappear. A regression verifies both actual weak
reference releases, authorized execution, denied actor access, cross-capability
reuse rejection and registration of a fresh same-capability version.
Mandatory formatting, all-target check, Clippy with warnings denied and all 367
tests (345 library, 22 integration) passed on 2026-10-13 with flatc 25.12.19.
Local test log: `/tmp/cogfabric-tool-retirement-tests.log`. No new permissions,
provider unloading, owned-child cleanup guarantee or performance gain is claimed.
Optimized installation verification must be refreshed for these retirement APIs.

The optional `cogfabric-server` workspace crate now builds on the core library
for authenticated loopback OpenAI-compatible Chat Completions. `src/agent/`
provides a transport-independent bounded native solver turn; OpenCode owns its
file/tool loop and approvals. Collective profile aliases retain Engine execution.
Actual OpenCode v2.0.26 with Qwen3 8B passed a disposable read/edit/read workflow.
See `HTTP_API.md` for setup, buffered SSE semantics and remaining native worker
and API limitations. Current workspace verification: 391 Rust tests, formatting,
all-target checking and Clippy with warnings denied. Earlier counts above are
historical milestones, not the current suite.

Native conversations now support opt-in same-image managed workers, including
registered models, scoped upstream credentials and one-task FlatBuffers grants.
Authenticated Unix exchange, audit-before-dispatch, cancellation, worker capacity
and normal/drop-path child reaping are verified. Actual OpenCode + Qwen3 8B also
passed a managed read/edit/read workflow. This supersedes earlier native-worker
limitations; role overrides and general independent executable isolation remain
unsupported. See HTTP_API.md, WORKERS.md and the latest quality evidence.

### Collective dropped-future cleanup

Collective inference/arithmetic/SQL process owners now retain child/socket/lease
ownership through asynchronous reaping after a task future drops. A shared
collective ownership drain is integrated into HTTP shutdown, including expert
inference owners. Cleanup failures disable new collective process admission
and fail shutdown for that host lifetime. Native and collective drains remain
separate; no new tool permissions or third-party sandboxing are introduced.
WORKERS.md describes deadlines and host-termination limits.

### Current installation verification

Source `a9933b3` passes a fresh Arch setup with Rust/Cargo 1.99.0 and FlatBuffers
25.12.19: 417 tests, all quality checks, optimized installations of both packages
and offline installed CLI/socket/HTTP workflows. Hosted CI also passed the new
HTTP smoke. Host managed OpenCode/Qwen evidence is separate from this clean-OS
run, which exercised no native inference or GPU. See INSTALLATION_VALIDATION.md
and RELEASE_AUDIT.md for exact scope and reproduction.

### Residency operation observation

`residency-inspect TASK_UUID --api-base NATIVE_BASE` now compares recorded
operator intent with fresh native inventory through read-only startup. It
reports matching instance IDs and observed desired state without resolving the
unknown outcome, changing task state or authorizing cleanup. Malformed/ambiguous
intent and endpoint mismatches fail before HTTP. Ownership-aware automatic
residency scheduling remains incomplete; MODELS.md specifies the limits.

### Basic benchmark preflight

`bench` now validates every dataset row before any task executes, rejects unknown
fields and empty/oversized cases, and records dataset identity plus runtime status
and charged model/tool task counts. Zero-call exhaustion remains distinguishable
from legitimate zero-model deterministic tool execution. Expected labels stay
outside inference prompts; scoring and comparison behavior are unchanged.

### Basic benchmark run cancellation

`bench --max-run-seconds` now bounds the run as well as each task. Ctrl-C or expiry
returns a partial report, durably cancels active inference and stops later cases;
the CLI exits unsuccessfully. Executed/unexecuted counts and run stop reason
distinguish cancellation from score failure. Existing Rust `benchmark` calls
retain a default 1800-second ceiling; `benchmark_until` accepts cancellation.
Cleanup grace and external provider continuation remain outside strict GPU-stop
guarantees. BENCHMARKING.md documents partial accuracy semantics.

### Benchmark identity disclosure

Basic benchmark reports now retain declared configuration and its hash, package
version and compiled role-prompt hash. Both runners share a bounded identity
reader for backend labels, expert bindings and primary registration snapshots,
without copying reasoning text. Configuration/weight identity remains explicitly
unattested; hashes and registrations describe metadata, not neural proof.

### Replaceable Jev+ expert selection

`routing::ExpertRouter` now separates executive skill selection from Engine
orchestration. The default tag heuristic remains unchanged. Trusted replacements
receive pinned skill metadata outside the SQLite lock; at most two distinct
enabled snapshot IDs are accepted, with policy version/audit disclosure. Old
Engine clones and admitted decisions retain their original handles/snapshots.
This adds no model/tool permissions or model calls. General native isolation,
policy catalogs, scheduler replacement and calibrated utility remain incomplete.

### Skill inventory bounds

Legacy and manifest registration now share a 4,096 retained-ID capacity under
immediate transactions. Skill snapshot/inventory reads cap rows and SQL field
bytes before Rust copies, strictly parse tags and reject invalid state/versions.
Valid legacy empty tags remain readable; malformed metadata is not silently
converted or repaired. The router uses the same inventory ceiling. EXPERTS.md
documents compatibility and the independent active/version limits.

### Manifest read integrity

Manifest snapshot/activation/idempotent reads now guard SQL field sizes before
Rust copies and verify bounded rows incrementally. Duplicate active identities
fail instead of silently overwriting a graph entry. Existing-version registration
checks stored body integrity before returning idempotent success. Corrupt data
remains unchanged and cannot be implicitly repaired or promoted by these reads.


### Explicit atomic expert disable sets

`skill-disable-set ID...` / `registry::disable_set` accepts 1–256 distinct
canonical IDs under the existing human-only `skill.disable` permission. The
operator explicitly selects the damaged expert and any dependents to contain.
Remaining enabled manifests, projections and dependencies must validate. No
cascade is inferred; unknown/already-disabled targets and late audit failures
roll back every enable change and audit in the immediate transaction. Bodies,
hashes and retained inventory capacity are preserved. Normal activation stays
strict; this is not executable isolation or automatic recovery. Invalid retained
inventory metadata can still require reviewed repair/restoration.
