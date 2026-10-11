# Current Rust implementation

This is the implementation snapshot, not the target specification. The complete
French vision is preserved in `ARCHITECTURE.md`. The branch is `develop/v1.0`,
package version `1.1.0-alpha.1`; it remains a pre-release despite its historical
version numbering. Rust 2021 and Tokio own the runtime; Python is offline only.

The seven responsibilities currently map to these modules:

| Layer | Implementation |
|---|---|
| Language Interface | `language.rs`, `backend.rs`, `config.rs`: advisory intent, explicit history, synthesis and loopback inference |
| Jev+ | `engine.rs`, `planning.rs`: task lifecycle, reference routing, capability gaps, bounded dependency scheduling |
| Jev-R | `reasoning.rs`: shared-budget model roles, revisions, stagnation detection and independent verification status |
| Expert Network | `registry.rs`: immutable reference manifests, dependencies, lifecycle and task version snapshots |
| Cognitive Fabric | `wire.rs`, `service.rs`, `store.rs`: typed owner-local IPC, durable state/evidence and SQLite/FTS5 |
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
