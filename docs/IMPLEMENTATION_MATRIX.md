# Implementation matrix — 1.1.0-alpha.1

Direct and task-pinned inference broker dispatch both preserve caller task UUIDs.
Concurrent distinct-task and malformed-task regressions cover this boundary in
`src/fabric/tests.rs`; general independent actor provisioning remains incomplete.

`src/fabric/unix.rs` adds host-connected inference transport authenticated against
approved OS peer UID/PID, validated correlated COGM frames and cancellation-safe
connection disposal. A separate test process verifies real process communication.
General actor routing remains incomplete; this adapter is not an executable
sandbox or attestation.

`src/fabric/unix/worker.rs` now implements worker-side authorization with immutable
host/task grants, cumulative resource ceilings and disconnect/shutdown cancellation.
Tests cover the complete Rust adapter/worker exchange and negative grants/budgets.
Worker sessions require bounded SQLite audit attachment and fail closed on
authorization-write failure; reopening/privacy/failure tests verify this path.
Opt-in same-binary task workers now launch through `src/fabric/unix/process.rs`,
receive authenticated grants, reuse pinned settings and are closed/reaped at task
termination. CLI integration tests cover demo execution, zero-process arithmetic
and pending HTTP cancellation. Worker audit reconciliation is operator-only under
exclusive runtime leases; real-crash, live-lease and rollback tests cover it.
Generalized executable module lifecycle, stale-resource cleanup and general actor
routing remain incomplete. See `WORKERS.md` and `RECOVERY.md`.

Managed worker lifetime now follows the task's approved wall ceiling rather than
an unrelated fixed timeout. Virtual-time regressions cover idle gaps, partial
frames and session expiry; legacy bootstrap lifetimes remain compatible.

`protocol/bootstrap.fbs` and `src/fabric/unix/bootstrap.rs` provide validated typed
single-task setup and pinned generation settings. Codec/negative tests and the
worker round-trip cover grant conversion. Authenticated bounded bootstrap delivery
has a real socket fixture and is used by the managed launcher.

The complete French `ARCHITECTURE.md` remains canonical. This matrix describes
current evidence on `develop/v1.0`, not a reduced target architecture. The project
remains a pre-release. `QUALITY_REVIEW.md` preserves verification history.

| Architecture sections | Requirement | Status | Current implementation and tests | Remaining release work |
|---|---|---|---|---|
| 7, 12 | Typed local IPC | Partial | `protocol/*.fbs`, `src/fabric.rs`, `src/events.rs`, `src/wire.rs`, `src/service.rs`: bounded verified IPC and module RPC, content-minimized typed progress subscriptions/catch-up, version-pinned inference registration/discovery, correlation/actor/destination/expiry checks, queues and cancellation; socket, spoofing, overflow, replacement and compatibility tests | General module event routing, separate-process actor authentication and broader compatibility fixtures |
| 3 | Language interface | Partial | `src/language.rs`, `src/backend.rs`, `src/config.rs`: strict advisory intent, clarification, original-goal preservation, explicit conversation references, optional synthesis, persistent role profiles; scripted and HTTP tests | Live local-model validation and measured interpretation quality |
| 4, 9 | Jev+ coordination | Partial | `src/engine.rs`, `src/planning.rs`, `src/store.rs::route`: capability routing/gaps, bounded topological subtask execution, admission budgets; dependency and budget tests | General module scheduling, measured escalation policy, automatic subtask resumption and broader scheduling telemetry |
| 5 | Jev-R reasoning | Partial | `src/reasoning.rs`: planner/solver/explorer/critic/reflector, strict recursive revisions, no-progress stopping, unverified subtask results and original-goal synthesis; order, revision and cancellation tests | General independent executable verifiers and live-model effectiveness measurements |
| 6 | Expert network | Partial | `src/registry.rs`: immutable semantic-versioned reference manifests, dependency/reverse-dependency checks, health, activate/disable/metadata rollback, bounded registry; lifecycle/tampering tests | Dynamic executable tool experts, measured workflow usefulness; backend loading/unloading; neural adapters remain experimental |
| 7 | Persistent memory | Partial | `src/store.rs`: SQLite WAL/FTS5, schema migrations through 6, durable task/events, isolated explicit history, pinned registry/document snapshots, explicit lease-protected interrupted-task recovery, immutable versioned local model manifests and task selection snapshots, operator-published episodic snapshots, recomputed arithmetic memory, revocation and payload quotas, SQLite page ceilings, global task admission and reviewed retention; migration/isolation/tampering/deletion-rollback tests | Broader independent fact verifiers, backend model residency, physical WAL/filesystem bounds, general external/neural artifact portability and automatic checkpoint resumption |
| 8 A/B | RAG evolution | Partial | `src/evolution.rs`, `src/evaluation.rs`: content-addressed quarantine, hashes/provenance/deduplication/quotas, isolated held-out retrieval, bounded cumulative approved-version regression gates and immutable accepted suites, atomic paired review records, transactional human promotion/rollback; integrity, regression, concurrency and rollback tests | Stronger poisoning/security evaluation, automatic source discovery and cleaning; dataset independence remains operator-attested |
| 8 C | Offline neural adaptation | Experimental | `training/train_lora.py`, inactive artifact registration; neural evaluation/promotion denied | Base-model provenance and independent neural evaluation manifests, actual training and externally isolated pipeline; no hot-swap claim |
| 8 D, 14 | Novel neural reconstruction | Not implemented | Research only; no trained Jev networks or superiority claims | Separate research and independent evaluations, beyond proven v1.0 infrastructure |
| 9 | Resource enforcement | Partial | `src/config.rs`, `src/reasoning.rs`: FAST/STANDARD/DEEP/RESEARCH ceilings, calls/cycles/reserved outputs/prompt bytes/tools/time, cancellation; exhaustion tests; optional provider usage and measured inference duration | Exact input-token preflight, model-load cost and retry metrics, process-isolated inference/GPU memory and optional energy telemetry |
| 10, 11 | Security kernel | Partial | `src/security.rs`: immutable task/actor scope intersected with global policy, per-dispatch audits, deny-default capabilities, bounded no-follow reads; replay/actor/promotion denials and artifact tampering tests | Authenticated independent module identities and isolation before executable experts; signatures/attestations and neural behavioral evaluator absent |
| 13 | Reproducible evaluation | Partial | `src/evaluation.rs`, `src/benchmarking.rs`, `src/corpus.rs`: expected exact-match benchmarks, six isolated same-model ablation profiles plus optional larger baseline, per-arm resources, reviewed dataset hashes, bounded read-only corpus snapshots and durable run cancellation; profile/isolation/deadline tests | Independent review of the provisional 100-case corpus, unseen technical tasks, robustness/security studies and measured live-model comparisons |
| 12, 14 | CLI/service | Partial | `src/main.rs`: initialization/configuration, ask, service controls, task-specific socket progress/cancellation, skill lifecycle, ingestion/search, task traces, candidates/rollback, benchmark, self-test; real socket fixtures | Live residency lifecycle |
| 12, 13 | Hardware/performance claims | Partial | Optional whole-device GPU diagnostics sampled real host memory/utilization/power; no energy or large-model advantage claimed | Authenticated live inference, task-attributed telemetry and equal-budget comparisons |

All 227 current tests pass, including actual socket/loopback HTTP fixtures; formatting,
all-target build checks and Clippy with denied warnings pass. Fixtures do not prove
GPU integration, neural quality, immutable backend weights or live model switching.

The latest host validation loaded an installed LM Studio model, but inference
returned HTTP 401. Scoped authentication is implemented and fixture-tested;
successful live generation remains unverified without an operator credential. It is not a blocker for implementing independent infrastructure,
but it is a release gate. No stable release tag or remote publication is authorized.

Independent verification now additionally supports explicit `order:` dependency
criteria through `src/verification.rs` and `src/reasoning.rs`; see
`VERIFICATION.md`. Failed checks drive bounded solver revisions; success preserves
the checked answer and records hashes. This advances sections 5/9/10 but does not
complete the requirement for broader independent executable verifiers.

Verification failures now preserve checked candidates as rejected outcomes through
progress IPC, episodic history and reviewed retention. Regression tests additionally
cover division-by-zero progress and single-call ordering rejection.

Comparison evaluation additionally supports versioned dependency-order criteria,
accepting every valid permutation across all arms rather than one exact answer.
Strict criterion preflight and a real loopback HTTP six-arm fixture cover this path;
this does not supply the remaining diverse held-out dataset or live-model results.

CLI diagnostics now checks storage/schema and registered selection, with optional
operator-only bounded loopback model discovery and explicit failure exit codes.
`src/diagnostics.rs` tests malformed discovery, redirects, bounds and missing IDs.
The actual CLI probe still finds no server at 127.0.0.1:1234; live generation remains
unverified and is a release gate.

`src/gaps.rs` and `src/evolution.rs::create_from_gap` now support bounded recorded-gap
inspection and reviewed RAG preparation. Candidate/evidence links commit atomically;
changed evidence, missing gaps and audit faults fail closed. Provenance source tasks
are protected from retention. No automatic source acquisition or poisoning/safety
approval is claimed.

`src/fabric.rs` now measures actual broker queue wait; append-only optional fields in
both RPC/socket schemas preserve missing legacy metrics. Per-call audits and task/
comparison totals carry measured queue waits, with cancellation invalidation.
Controlled contention and malformed/legacy metric tests cover the implementation.

Prompt/workflow experts now use immutable bounded checklist manifests and one
separately audited shared-backend advice call in full/experts profiles. Budget and
zero-cycle admission, profile isolation, manifest replacement/disable and invalid
workflow tests cover this path. Measured utility remains partial; structured
multi-step advice is described below. No extra model residency or tool authority
is introduced.

`src/backups.rs` now creates consistent checked SQLite snapshots with owner-only
storage, checksums, bounded admission and nonblocking backup leases. WAL-content,
read-only restore, symlink/capacity/contention tests and an actual CLI copy/reopen
smoke cover this path. Optional configuration and RAG artifact restoration are now implemented as
described below; arbitrary external artifacts remain outside automated restore.

Owner-authenticated independent-process progress streaming is now implemented by
`src/service.rs`, `src/wire.rs` and `watch-task`. Tests cover catch-up/live ordering,
task isolation, two-watcher admission, overflow requiring durable replay, terminal
completion and shutdown. This closes the streaming item in the IPC and CLI rows
above; general module routing and separate-process actor authentication remain
partial. See `EVENTS.md` for limits and Store-instance publication boundaries.

`src/telemetry.rs` now samples bounded optional Linux process RSS and lifetime
high-water RSS at task completion and in diagnostics. Task audits, append-only
socket fields and comparison maxima preserve unavailable measurements. Parser,
legacy-frame and aggregation tests cover this path. These are whole Rust-process
observations, not isolated task peaks, backend residency or GPU measurements.

The optional offline trainer now freezes bounded dataset input, rejects ambiguous/
conflicting provenance, fails on passes with no supervised tokens, reserves separate
output and records seeded configuration, dependency versions and hashed adapter
inventory. Nine standard-library preflight tests pass in addition to the 164 Rust
tests. Level C stays Experimental: ML dependencies/weights are unavailable here,
and base provenance manifests, independent neural evaluation, external isolation,
actual training and adapter switching remain unverified. Promotion remains denied.

`src/sql.rs` adds a built-in bounded data.sql.s1 tool for explicit SELECT tasks over
inline tables, with task-scoped authorization, two-worker admission, fresh in-memory
connections, deny-default SQLite authorizers, cancellation/progress limits and
hashed results. Aggregation, isolation, recursive exhaustion, mutation/file/schema
denials, baseline separation and durable status tests cover this path. Tool-backed
domain functionality advances; dynamic executable manifests, external connectors
and broader independent verifiers remain incomplete. See `SQL.md`.

Workflow manifests additionally support one to four dependency-ordered advice
steps, strict DAG validation, pinned versions, shared inference and conservative
whole-workflow admission. Tests cover legacy hash compatibility, malformed graphs,
execution order, dependency results, profile isolation, version replacement and
budget refusals. This implements bounded multi-step advice; executable/conditional
workflow interpreters, measured utility remain incomplete; registered-model advice is described below. See `EXPERTS.md`.

`src/restore.rs` and `restore-database` now automate current-schema database-only
restoration into an exclusively new private directory, with saved-hash verification,
SQLite validation, no-overwrite publication and a private versioned restore record.
Tests cover preserved running state without replay, existing paths, checksum/
symlink/corruption/schema/WAL denials and model permission denial. This does not
restore configuration/artifacts or automatically migrate older snapshots; full
workspace restoration remains partial. See `STORAGE.md`.


Workflow experts may now pin an enabled immutable local-model registration and
invoke its actual endpoint through the bounded broker. Tests cover activation,
in-flight pinning, live disable overriding frozen knowledge snapshots, budget
refusal, failed-call accounting and managed two-child cleanup. See `EXPERTS.md`.
Model configuration snapshots are implemented; weight attestation, automatic model
residency and live domain-quality measurements remain incomplete.

Comparison reports now expose bound expert configurations and invalidate the
same-model configuration-control flag when observed, including provider failures.
Regression coverage checks experts-only disclosure and ordinary single-model arms.

Expert selection now preserves the router's relevance ranking and skips disabled
model bindings before dispatch, permitting another already routed workflow without
spending calls on the unavailable candidate. Tests cover score/ID disagreement,
version replacement, disable fallback and audited admission ordering. The router
still returns at most two candidates; general utility-based scheduling is partial.

Offline training now requires a bounded versioned dataset split manifest with
provenance, exact file hashes and normalized prompt disjointness across training,
validation and test. Only training rows reach optimization; tests cover changed
inputs, overlap, path/version/field denials and dry-run behavior. Semantic split
independence, base-weight provenance, real training, external isolation and an
independent neural evaluator remain unproven. Level C remains Experimental.

Comparison evaluation now supports sql_result_json_v1: strict typed result-shape
validation and structural equality against separately supplied expected labels.
A six-arm regression covers actual inline SQLite execution, no-model tool calls,
JSON formatting independence, type/value errors and runtime-status exclusions.
Operator label correctness and the diverse live-model study remain outstanding.

A provisional 100-case synthetic evaluation corpus now covers ten categories in
`evaluation/v1_cases.jsonl`, with hashed inventory and explicit incomplete review
flags. Regression coverage checks all shapes/identities and recomputes thirty
deterministic tool cases. Independent label review, contamination analysis, harder
unseen technical tasks and live equal-budget study results remain release work;
the case count alone does not establish the diverse held-out study requirement.

Decomposition admission now includes conservative predecessor JSON/prompt-byte
bounds and reserves the first whole-goal prompt as well as calls/output tokens.
Research exploration preserves that solver's remaining calls/tokens/input bytes.
Four constrained-budget regressions verify whole-goal fallback and audited skips;
this is bounded scheduling, not measured utility estimation or resumable graphs.

Optional operator GPU observations are implemented in `src/gpu.rs` and
`diagnostics --probe-gpu`: fixed read-only NVIDIA query, bounded child/output, strict
units/ranges, unsupported values preserved as unavailable and explicit whole-device
scope. Parser and timeout/overflow/reaping fixtures pass. The actual driver probe
failed here, so live GPU integration, task attribution, energy and load-cost
measurement remain unverified. No GPU data is fabricated or added to task totals.

Registered local model endpoints now have an explicit operator diagnostic covering
expert registrations as well as the primary configured backend. The bounded
deduplicated GET probe reports missing IDs and exact configuration checksums,
skips disabled registrations and refuses more than eight distinct endpoints before
network access. Fixtures cover missing advertisements, deduplication and admission.
Live generation/residency remains unverified; local LM Studio CLI startup timed out
despite existing GGUF files, so absence of weights is not assumed.

Comparison runs can now pin an operator-reviewed dataset SHA-256 before task
creation. Reports distinguish prior operator pinning from observed byte hashes.
Regression coverage confirms malformed/mismatching pins and even whitespace-only
changes create no additional tasks. This proves byte binding, not independent
review, contamination resistance or the still-outstanding live-model study.

Host-access validation refined earlier sandbox observations: the framework GPU
diagnostic sampled a real 12 GiB device, LM Studio API access succeeded, and an
existing local model loaded with 35% GPU offload / 2,048 context / one prediction.
Discovery and inference then returned HTTP 401 because the server requires
authentication. No credentials were searched or authentication disabled. The
temporary model was unloaded and the previously stopped server was stopped again;
loaded-model inspection returned empty. Successful generation remains unverified.
Scoped operator-supplied API authentication is now implemented and fixture-tested;
an operator credential is still needed for live generation. External model loading
is not automatic framework residency control.

Scoped local API authentication: **Implemented** for direct HTTP generation,
primary and registered discovery, and managed-worker forwarding. See
`src/credentials.rs`, `src/backend.rs`, `src/diagnostics.rs`, typed bootstrap and
`tests/inference_auth.rs`. Operator configuration persists scope only; endpoint
changes cannot forward the primary token. Synthetic direct/managed tests reject
escaped credential echoes without writing credentials to task traces or config.
Real authenticated LM Studio generation remains **Not verified**. See
`docs/AUTHENTICATION.md` for setup and residual same-user/provider risks.

Authentication boundary follow-up: CLI integration verifies scoped and non-scoped
primary/registered discovery, escaped echo rejection, and missing-credential
failures before connecting for direct and managed tasks. `tests/inference_auth.rs`
contains the evidence; full Rust regression count is 168. Actual server credentials
and live successful generation remain external validation requirements.

Paired comparison reporting: **Implemented** in `src/benchmarking.rs` against the
small-model-alone arm. Reports include improved/regressed case indices, paired and
unpaired coverage, explicit denominators and null unavailable rates. Unit tests
cover incomplete/zero-denominator outcomes; the real six-arm SQLite fixture checks
aggregation from executed tasks. This supplies descriptive dataset regression
metrics, not the outstanding live held-out robustness or security study.

Configuration recovery: **Implemented** for new checked database snapshots via
embedded versioned effective settings and explicit `--restore-configuration`.
`src/backups.rs`, `src/restore.rs` and CLI integration cover preserved settings,
private new-path publication, invalid/missing metadata rejection, legacy default
restoration and no task replay. External artifacts/weights and older-schema
migration remain **Not implemented**; this does not complete full workspace restore.

RAG candidate artifact recovery: **Implemented** as explicit bounded snapshot and
restore options in `src/rag_archive.rs`, `src/backups.rs`, `src/restore.rs` and CLI.
Tests verify checksummed read-only payload roundtrip, canonical path rebasing without
status changes, malformed/missing/oversized payload and trigger rejection, opt-in
re-backup behavior and actual CLI flags. Neural/unrelated external files remain
excluded; general full external artifact restoration stays **Partial**.

Older snapshot migration: **Implemented** for explicit `--migrate-schema` on versions
1–5. `src/store.rs::migrate_snapshot` applies existing transactional migrations to
checked staging only, with limits/progress interruption and standalone publication.
`src/restore.rs` tests all recognized versions, source preservation, no task replay
and failure before publication; CLI integration exercises the option. Future and
unversioned schemas remain rejected. This supersedes prior older-schema blockers,
while arbitrary external/neural artifacts remain outside the recovery implementation.

Versioned task-progress module routing: **Implemented** for trusted embedding hosts
in `src/events/modules.rs` and `src/events.rs`. Registration/discovery, immutable
versions, pinned task/stage subscriptions and shared bounded queues work over COGE.
Tests cover actual durable events, task/stage filtering before admission, replacement,
unregistration, malformed descriptors, registry/subscriber bounds and overflow.
General independent-process actor identities, executable module event handling and
command routing remain **Partial / Not implemented**; no model permissions expand.

Event observer recovery: **Implemented** via explicit cancellation/receive-until
and module-bound durable replay. Tests verify pending/buffered cancellation,
capacity release without cancelling tasks, overflow replay, preserved pinned stages
and cursor advancement through empty filtered pages. This strengthens host event
lifecycle; external module provisioning and execution remain incomplete.

Bounded audit inspection: **Implemented** in `Store::task_events_since` and CLI
`task-events`, with stable indexed task-scoped pages. Full trace reads now fail
explicitly on event/raw-byte ceilings and malformed/oversized history. Comparison
reports select bounded identity events instead of materializing full traces.
Tests cover page order/isolation/cursors, full count/byte bounds, malformed records
and actual CLI pagination. This supports recovery inspection; automatic graph
resumption and restoration of unknown interrupted inference usage remain incomplete.

Inference reservation recovery evidence: **Implemented** in
`src/recovery/inference.rs` and `inference-audit`. New reasoning events correlate
results to charged calls; bounded transactional inspection validates identities,
hashes and allocations while preserving unknown legacy/interrupted usage. Tests
cover runtime/CLI production of call IDs, incomplete/legacy reports and malformed,
duplicate/unmatched/tampered records. This does not authorize replay or complete
automatic graph resumption; remaining-state/budget/configuration checks still apply.

Durable execution settings: **Implemented** in Engine/Session before inference,
with runtime configuration, effective call/cycle limits and execution options.
Embedding validation now rejects invalid configuration before task creation.
Tests cover durable original settings, ordering before dispatch, private path
exclusion and snapshot write failure containment. Backend attestation and restored
permissions are explicitly false; automatic checkpoint continuation remains partial.

Task recovery execution bounds: **Implemented** in `src/recovery.rs`. Exclusive
lease-protected reconciliation validates bounded UUIDs before updates and applies
a five-second SQLite progress deadline. Regression tests cover malformed/oversized
identities, oversized batches, expensive-trigger interruption, atomic rollback and
callback cleanup. This improves reconciliation only; automatic resumption remains
partial and external inference completion is still unknown.

Durable advisory graph reconstruction: **Implemented** in `src/recovery/graph.rs`
and `graph-audit`. Bounded transactional inspection validates pinned graph
structure, per-node transitions and dependency ordering without upgrading results
to evidence or revealing result bodies. Tests cover partial histories, isolation,
malicious transitions, changed definitions, duplicate plans, real scheduler events
and CLI inspection. Automatic checkpoint execution remains **Partial**: this is
validated state reconstruction, not remaining-budget admission or replay authority.

Saved inference admission reconstruction: **Implemented** in
`src/recovery/inference.rs`. Original validated runtime settings and deterministic
mode ceilings bound remaining calls, reserved output and prompt-byte charges;
unrecorded results never refund charges. Tests cover runtime/CLI reports, missing
legacy snapshots, invalid/late/duplicate snapshots, mode mismatch, uncorrelated
results and overdrawn budgets. Wall-time/cycle/tool reconstruction and actual
checkpoint continuation remain **Partial**, with no resumed execution authority.

Durable tool/cycle admission: **Implemented** in `src/reasoning.rs` and recovery
inference auditing. New settings version the resource ledger; reservations are
written before tool admission/cycle execution, with failure containment and
conservative reconstruction against original limits. Tests cover injected write
failure, denied-tool behavior, actual runtime cycles, missing version markers,
forged sources/capabilities, noncontiguous charges and cycle exhaustion. Older
histories retain unknown tool/cycle allowances. Wall-time recovery, immutable
reference admission and actual continuation remain **Partial**.
Repeated Session solve calls now share the original cycle ceiling rather than
restarting it; a regression test checks one exhausted FAST session remains at
one cycle and one inference call after another solve request.

Shared monotonic execution deadline: **Implemented** in Engine/Session. Setup time
consumes the original wall budget; sessions reject expired tool/cycle/model
admission and cap request timeout by remaining time. Tests exercise slow setup,
zero post-expiry dispatch and refusal to extend a restricted deadline. Blocking
callbacks/I/O are not preemptible; cross-process wall-time recovery remains
**Partial** and no fresh allowance is granted by recovery inspection.

Model registry read integrity: **Implemented** in `src/models.rs`. Listing now
checks embedded identity against row identity as exact lookup already did, and
both paths exclude oversized payload/checksum fields before Rust allocation.
Regression tests cover changed IDs/versions with recomputed valid hashes and
oversized manifests, checksums, row IDs and versions. This strengthens discovery
and version references; server-loaded weight identity remains unattested.

Independent integer sorting verification: **Implemented** in `src/verification.rs`
and Jev-R/Engine criterion dispatch. Explicit `sort:` goals preserve signed 64-bit
integer multiplicity and ascending/descending order, with bounded input/output and
strict metadata parsing. The checker uses an individually charged `sorting.verify`
capability and records `integer_sort/v1` rather than ordering evidence. Tests cover
invalid/overflow/duplicate-loss answers, malformed criteria, bounded recursive
revision, FAST rejection, denied verification budget and criterion-specific evidence.
General independent task verification and live effectiveness remain **Partial**.

Freshly verified structured-memory reuse: **Implemented** in `src/engine.rs`.
Full/Memory profiles may complete exact `sort:`/`order:` goals from explicitly
published episodes only after a new individually charged independent check and
audit. No memory kind or historical status grants verification. Tests cover
zero-inference success, wrong answers, revocation, changed goals, exhausted tool
budget and actual CLI reuse. General fact verification and broad live memory
benefit remain **Partial**; this claims no learned weights or benchmark advantage.

Immutable assembled reasoning inputs: **Implemented** in `src/reasoning.rs`.
Versioned context snapshots preserve actual objective/reference/advice bytes and
hashes before planning/solving; raw and serialized bounds and audit-write failures
prevent later dispatch. Tests mutate a retrieved document after execution and
verify retained original context/hash/order, inject snapshot failure and reject
oversized/escaped inputs. This preserves recovery evidence; validated replay,
remaining-time admission and actual continuation remain **Partial**.

Checked saved-result recovery: **Implemented** in `src/recovery/result.rs` and
`recover-verified-result`. Under exclusive runtime ownership, one immediate
transaction validates the original bounded inference ledger and independently
checks the latest solver answer against an explicit original sort/order criterion.
Remaining verification allowance is charged; valid answers complete without new
inference, failed checks preserve task state with durable charges, and SQL/audit
failure rolls back. Tests cover leases, reopen durability, terminal/actor denials,
wrong/tampered/unresolved answers, exhausted budgets, atomic failure and real CLI
use. This completes a concrete result-reconciliation path; arbitrary graph
continuation, missing outcomes and recovered wall-time admission remain **Partial**.

Compiled checker expert bindings: **Implemented** in `src/registry.rs`, Engine and
Session verification. Reviewed checker manifests use existing immutable lifecycle
and dependency checks, route by capability tags and bind only supported compiled
criteria. Health and actual memory/reasoning proof events disclose manifest and
checker identities; tests cover malformed bindings, legacy hashes, replacement,
disabling, pinned historical proofs and independent recursive verification.
Registration grants no capabilities; arbitrary executable experts and dynamic
implementation loading remain **Partial / Not implemented**.

### Offline security boundary evaluation — Implemented (bounded scope)

`src/security_evaluation.rs` and the `security-eval` CLI provide 24 reproducible
permission, task-isolation, FlatBuffers and manifest probes with positive controls,
fixture hashes and failure exit status. Unit tests verify deterministic reports;
`tests/security_evaluation.rs` checks the actual CLI. Broad adversarial evaluation,
live injection/poisoning, sandbox escapes and module peer authentication remain
**Not implemented** by this suite. It cannot approve neural candidates.

Host inference broker shutdown: **Implemented** in `src/fabric.rs`. An explicit
permanent stop signal reaches current and retired task-pinned versions, cancels
active futures, discards queued work and prevents subsequent registration/pinning
or dispatch. Tests in `src/fabric/tests.rs` cover retained versions after replace/
unregister, buffered work, cancellation, idempotence and pre-initialization stop.
Worker-loop exit is asynchronous; general executable lifecycle and external
inference termination remain unproven.

Service broker lifecycle integration: **Implemented** in `src/service.rs`. Normal
shutdown joins task handlers before stopping the shared broker, so retained pins
cannot continue inference after service completion. Socket tests cover typed STOP
with an incomplete client and host-triggered shutdown during pending HTTP work,
while retaining durable cancelled task status. General module routing is still
incomplete; this adds no model capability.

Native residency inspection: **Implemented** in `src/residency.rs` and
`residency-status`. Bounded LM Studio native inventory distinguishes installed
models from loaded instances, with dedicated opt-in credentials and no mutations.
Unit and actual CLI HTTP tests cover identities, native endpoint constraints,
separate authentication, escaped token echo rejection, redirects, malformed and
oversized responses. Physical load/unload ownership and live validation remain
**Not implemented / Not verified**. See `MODELS.md`.

Explicit native model load/unload: **Implemented** in `src/residency.rs` and
`residency-load` / `residency-unload`. Operator approval, exclusive workspace
leases, installed/exact-instance preflight, durable pre-effect intent, bounded
requests and fresh inventory confirmation are tested through the CLI. Unknown
outcomes remain interrupted without retries or implicit cleanup. Cross-client
ownership cannot be established by this API; automatic lifecycle scheduling,
weight identity, generation after switching and real host validation stay
**Partial / Not verified**. See `MODELS.md` for operator responsibilities.

Residency audit/status consistency: **Implemented** in `src/residency.rs`.
Terminal confirmation/unknown-outcome metadata and task status commit atomically;
injected audit/status failures leave the prior running state without partial
completion evidence. Missing and terminal anchors are rejected. External model
server effects remain outside the database transaction and are never retried
merely because local persistence failed.

Installed binary workflow: **Implemented and locally verified** through
`scripts/installed-smoke.sh`. An isolated optimized offline source installation
passes tool tasks, skill registration, reference retrieval, example RAG promotion/
rollback, configuration/artifact restore and real socket ask/stop in fresh state.
README documents installation prerequisites and reproduction; CI includes the
workflow. Clean-OS package setup, remote CI and live inference remain unverified.
See `RELEASE_AUDIT.md` for source/binary identity and evidence limits.

Per-subtask coordination costs: **Implemented** in `src/reasoning.rs`. Scheduler
terminal nodes now disclose actual reserved calls/output/prompt bytes, measured
timing and optional provider/queue deltas without result bodies or verification
claims. Tests cover dependency execution, skipped/cancelled nodes, unavailable
usage and audit failure stopping subsequent dispatch. Expected-utility estimation,
parallel/general module scheduling and automatic resumption remain **Partial**.

Same-boot execution deadline evidence: **Implemented** in `src/execution_clock.rs`,
Engine/Session settings and recovery inference audits. Original pre-setup Linux
boot-time snapshots reconstruct decreasing/expired nanosecond allowance without
civil timestamps. Tests cover actual kernel observation, separate CLI processes,
setup exhaustion, reboot mismatch, backwards time and malformed snapshots. Legacy,
other-boot and unavailable clocks do not receive a fresh allowance. Actual graph
continuation remains **Partial / Not implemented**; inspection grants no authority.


Explicit pre-solver advisory continuation: **Implemented (bounded scope)** in
`src/recovery/continuation.rs`, `src/reasoning.rs` and `resume-task`. Original
same-boot deadlines and resource charges survive restoration; completed nodes
are correlated to saved planner/solver output and reused as unverified advice.
Five regression tests include actual HTTP, corrupted checkpoints, lease/approval
refusals, cancellation and terminal transaction failure. Default direct module
Full/Collective tasks are supported; managed/registered/custom backends,
post-solver phases and general repeated recovery remain **Partial / Not implemented**.
See `RECOVERY.md` for exact admission, timing and provider-identity limitations.


Registered direct backend continuation: **Implemented** in
`src/recovery/continuation.rs`. Admission validates the original immutable manifest
against both its task snapshot and current enabled registry entry, rejects changed
endpoint/model/limits and never selects a newer version. Tests exercise real HTTP
model selection and six missing/disabled/altered registration cases. This extends
the preceding bounded recovery entry; managed/custom backends and later phases
remain **Partial / Not implemented**. Provider weight identity remains unattested.


Managed-worker advisory continuation: **Implemented** in `src/recovery.rs`,
`src/recovery/continuation.rs` and `tests/resume_worker.rs`. Transaction-protected
lease conversion permits a fresh same-binary worker with only remaining resource
grants while preserving exact host deadline enforcement. Actual CLI success and
in-flight HTTP cancellation tests assert reaping and cumulative unresolved charges;
a lease test verifies shared worker access and exclusive recovery refusal.
Canonical private workspace storage is required. This supersedes the managed-mode
limitation above; custom modules, later phases, repeated recovery and external GPU
termination remain **Partial / Not implemented / Not verified**.


Durable reasoning boundaries and revision continuation: **Implemented (bounded
scope)** in `src/reasoning.rs` and `src/recovery/continuation.rs`. Before charging a
new solver cycle, the runtime persists bounded strategy/previous-answer/repeat
state and ledger anchors. Unconsumed boundaries following a completed advisory
graph can resume with original cycles, budgets and independent verification;
stale or altered boundaries fail closed. Tests cover write failure, preservation
of repeat stopping, tampering/consumption and actual resumed HTTP verification.
This supersedes the blanket later-phase limitation above. Arbitrary within-cycle,
no-graph and general repeated recovery remain **Partial / Not implemented**.


Whole-goal and skipped-plan boundary recovery: **Implemented** in
`src/recovery/continuation.rs`. Validated unconsumed boundaries admit FAST tasks,
malformed-planner fallback and explicit budget-skipped graphs while preserving
original resources and stopping state. A valid planner result with missing graph
history fails closed. Tests include actual HTTP no-graph revision, FAST and skipped
budgets plus missing-plan/consumed-boundary denials. This supersedes the preceding
no-graph limitation; within-cycle and repeated recovery remain **Partial**.


Repeated solver-boundary recovery: **Implemented** in
`src/recovery/continuation.rs`. Up to 17 immutable context assemblies are validated
against original bytes and chronological call/cycle reservations. Two-boundary
regression execution retains one settings record, cumulative charges and the
original cycle cap. Tampered counters/context, consumed/exhausted boundaries and
excess history fail closed. This supersedes the general repeated-boundary limit;
arbitrary within-cycle recovery remains **Partial / Not implemented**.


Failed saved-answer revision reconciliation: **Implemented (structured criteria)**
in `src/recovery/result.rs` and `src/recovery/continuation.rs`. A fresh independent
sort/order rejection may save an admitted next-solver boundary without repeating
inference. Original budgets/context/deadline and repeat stopping remain enforced;
write failure rolls back proof, tool charge and boundary together. Tests cover
actual HTTP continuation, repeated-answer refusal and fault rollback. Unknown
outcomes and arbitrary within-cycle phases remain **Partial / Not implemented**.
