# Implementation matrix — 1.1.0-alpha.1

Implemented on Linux: managed inference, arithmetic and SQL workers launch from
the running service image through `/proc/self/exe`, retaining version consistency
after an upgrade replaces its original executable pathname. See
`src/fabric/unix/process.rs`, `src/fabric/tools/unix/process.rs` and the real-service
replacement regression in `tests/worker_executable.rs`. Other Unix systems retain
pathname-based launching; executable attestation and hostile-host isolation remain
not implemented.

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
| 3, 7, 9, 10, 12 | External agent HTTP interoperability | Implemented / Partial | `src/agent/`, `crates/cogfabric-server/`: generation-only native solver turns and authenticated same-image workers, authenticated loopback Chat Completions/SSE, client-owned tool proposals/results, collective aliases and bounded admission; real OpenCode v2 + Qwen read/edit/read, HTTP and worker regression tests | Responses/multimodal API, independent executable provisioning and role selection, broader client compatibility and independent agentic-quality/security evaluation |
| 7, 12 | Typed local IPC | Partial | `protocol/*.fbs`, `src/fabric.rs`, `src/events.rs`, `src/wire.rs`, `src/service.rs`: bounded verified IPC and module RPC, content-minimized typed progress subscriptions/catch-up, version-pinned inference registration/discovery, correlation/actor/destination/expiry checks, queues and cancellation; socket, spoofing, overflow, replacement and compatibility tests | General command routing, automatic independent actor provisioning and broader compatibility fixtures |
| 3 | Language interface | Partial | `src/language.rs`, `src/backend.rs`, `src/config.rs`: strict advisory intent, clarification, original-goal preservation, explicit conversation references, optional synthesis, persistent role profiles; scripted and HTTP tests | Live interpretation and synthesis quality measurements; direct/managed/socket generation now succeeds |
| 4, 9 | Jev+ coordination | Partial | `src/engine.rs`, `src/planning.rs`, `src/store.rs::route`: capability routing/gaps, bounded topological subtask execution, admission budgets; dependency and budget tests | General module scheduling, measured escalation policy, automatic subtask resumption and broader scheduling telemetry |
| 5 | Jev-R reasoning | Partial | `src/reasoning.rs`: planner/solver/explorer/critic/reflector, strict recursive revisions, no-progress stopping, unverified subtask results and original-goal synthesis; order, revision and cancellation tests | General independent executable verifiers and live-model effectiveness measurements |
| 6 | Expert network | Partial | `src/registry.rs`: immutable semantic-versioned reference manifests, dependency/reverse-dependency checks, health, activate/disable/metadata rollback, bounded registry; lifecycle/tampering tests | General executable loading and health, measured workflow usefulness and real weight residency; trusted tool/inference rollback is implemented |
| 7 | Persistent memory | Partial | `src/store.rs`: SQLite WAL/FTS5, schema migrations through 6, durable task/events, isolated explicit history, pinned registry/document snapshots, explicit lease-protected interrupted-task recovery, immutable versioned local model manifests and task selection snapshots, operator-published episodic snapshots, recomputed arithmetic memory, revocation and payload quotas, SQLite page ceilings, global task admission and reviewed retention; migration/isolation/tampering/deletion-rollback tests | Broader independent fact verifiers, backend model residency, physical WAL/filesystem bounds, general external/neural artifact portability and automatic checkpoint resumption |
| 8 A/B | RAG evolution | Partial | `src/evolution.rs`, `src/evaluation.rs`: content-addressed quarantine, hashes/provenance/deduplication/quotas, isolated held-out retrieval, bounded cumulative approved-version regression gates and immutable accepted suites, atomic paired review records, transactional human promotion/rollback; integrity, regression, concurrency and rollback tests | Stronger poisoning/security evaluation, automatic source discovery and cleaning; dataset independence remains operator-attested |
| 8 C | Offline neural adaptation | Experimental | `training/train_lora.py`, inactive artifact registration; neural evaluation/promotion denied | Base-model provenance and independent neural evaluation manifests, actual training and externally isolated pipeline; no hot-swap claim |
| 8 D, 14 | Novel neural reconstruction | Not implemented | Research only; no trained Jev networks or superiority claims | Separate research and independent evaluations, beyond proven v1.0 infrastructure |
| 9 | Resource enforcement | Partial | `src/config.rs`, `src/reasoning.rs`: FAST/STANDARD/DEEP/RESEARCH ceilings, calls/cycles/reserved outputs/prompt bytes/tools/time, cancellation; exhaustion tests; optional provider usage and measured inference duration | Exact input-token preflight, model-load cost and retry metrics, process-isolated inference/GPU memory and optional energy telemetry |
| 10, 11 | Security kernel | Partial | `src/security.rs`: immutable task/actor scope intersected with global policy, per-dispatch audits, deny-default capabilities, bounded no-follow reads; replay/actor/promotion denials and artifact tampering tests | General executable attestation and isolation before untrusted experts; signatures/attestations and neural behavioral evaluator absent |
| 13 | Reproducible evaluation | Partial | `src/evaluation.rs`, `src/benchmarking.rs`, `src/corpus.rs`: expected exact-match benchmarks, six isolated same-model ablation profiles plus optional larger baseline, per-arm resources, reviewed dataset hashes, bounded read-only corpus snapshots and durable run cancellation; profile/isolation/deadline tests | Independent review of the provisional 100-case corpus, unseen technical tasks, robustness/security studies and measured live-model comparisons |
| 12, 14 | CLI/service | Partial | `src/main.rs`: initialization/configuration, ask, service controls, task-specific socket progress/cancellation, skill lifecycle, ingestion/search, task traces, candidates/rollback, benchmark, self-test; real socket fixtures | Live residency lifecycle |
| 12, 13 | Hardware/performance claims | Partial | Optional whole-device GPU diagnostics sampled real host memory/utilization/power; no energy or large-model advantage claimed | Task-attributed telemetry, real model switching and equal-budget comparisons |

All 355 current tests pass, including actual socket/loopback HTTP fixtures; formatting,
all-target build checks and Clippy with denied warnings pass. Fixtures do not prove
GPU integration, neural quality, immutable backend weights or live model switching.

Earlier LM Studio validation returned HTTP 401. Later direct and managed-worker
generation succeeded through the existing Ollama OpenAI-compatible endpoint;
the managed sorting answer passed independent Rust verification. See
`LIVE_INFERENCE.md` for actual measurements and limits. LM Studio authentication,
controlled native load/unload and reviewed equal-budget studies remain unverified.
LM Studio Qwen3 8B generation and native read-only inventory now pass real smoke
tests; see the later entries for actual scope and observed injection failures. Private
GitHub publication is authorized; no final stable release tag is authorized.

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


Authenticated process event-module delivery: **Implemented (host API)** in
`src/events/unix.rs`, `protocol/event_stream.fbs` and generator integration.
COGS wraps verified task-scoped progress with pinned module versions/stage grants;
both sides authenticate approved UID/PID peers. Bounded lifetimes/frames/queues,
cancellation and explicit overflow/protocol errors are tested, including a real
child process. Registry/filter replacement semantics persist. Automatic provisioning,
IPC replay, service/CLI listener integration and general module command routing
remain **Partial / Not implemented**; no executable attestation is claimed.


Durable authenticated module replay: **Implemented (host API)** in
`src/events/unix.rs`, `src/events/modules.rs` and COGS v2. Pinned filtered replay
advances empty-page cursors, deduplicates overlap with live queues and restores
host-retained grants after registry replacement/unregistration. Version-one live
output remains compatible; wrong versions/cursors and receiver frame/time
exhaustion fail closed. Five additional regression tests cover replay/reconnect,
pinned filtering and negative limits. This supersedes the IPC replay limitation;
automatic provisioning and general command routing remain **Partial**.


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


Installed tool workflow: **Implemented and locally verified** through the expanded
`scripts/installed-smoke.sh` against an optimized isolated source installation of
`6919087`. Actual CLI/service arithmetic workers are audited/reaped; rejected
arithmetic is cleaned up; SQL dispatch stays separate; restored configuration
launches a functioning worker. Binary/script hashes, task traces and exact build
identity are recorded in `RELEASE_AUDIT.md`. All 292 mandatory Rust tests and quality
checks pass. This supersedes the earlier installed binary evidence, while final
runtime changes, clean-OS setup and live inference still require verification.

SQL process transport: **Implemented** for trusted Rust embedding through
`src/fabric/tools/sql/unix.rs`; automatic CLI/service provisioning remains
**Not implemented**. Tests cover actual child-process SQLite, exact UID/PID and
task grants, cross-capability denials, immutable registration identity, cumulative
calls, replay, corrupt/oversized responses, partial-frame/session deadlines,
cancellation and failed authorization audit. Queries/results are excluded from
worker metadata. All 301 Rust tests and mandatory quality checks pass. This
supersedes the SQL transport limitation above, but does not establish sandboxing,
crash cleanup or installed verification of the new runtime source.

The SQL Unix source commit `af92168` subsequently passed a fresh optimized install
and existing installed CLI/service smoke workflow. See `RELEASE_AUDIT.md` for
hashes and retained evidence. Automatic SQL worker provisioning is still absent;
separate-process transport evidence comes from the Rust child-process regression.

Managed SQL workers: **Implemented** for the built-in `sql.local` 1.0.0 module in
CLI/service execution. Files: `src/fabric/tools/unix/process.rs`, `src/engine.rs`,
`src/config.rs`, `src/main.rs`, `src/fabric/tools.rs`, `protocol/tool_bootstrap.fbs`.
Tests: `tests/tool_worker.rs` real SQL CLI/service success, rejected unsafe query
with confirmed reaping, exhausted budget with no child; process-unit regressions
cover SQL COTB v2, arithmetic v1 compatibility, cross-capability/version refusal,
wrong-task scope and custom-registry refusal before task creation. Shared startup
ownership and cancellation/reaping paths remain covered. This supersedes the SQL
provisioning limitation above; arbitrary executables, crash cleanup, independent
SQL-engine implementations and live model measurements remain incomplete.

Managed SQL integration validation passes all mandatory quality checks with 304
Rust tests (283 library, 21 integration). The expanded smoke script also checks
SQL worker configuration, success/rejection reaping, backup/restored worker state
and actual socket SQL execution. Optimized installation verification is pending
for this new source; live inference and broader release blockers remain open.

Optimized installed validation of `04a3a03` now passes the expanded smoke workflow,
including actual managed SQL CLI/service execution, rejected-query cleanup and
restored SQL process configuration. Exact hashes/evidence appear in
`RELEASE_AUDIT.md`; the pending installed-source gap above is closed for this
commit. Crash cleanup, general executable integration and live measurements remain open.

Inference startup failure containment: **Implemented** in
`src/fabric/unix/process.rs`. Ownership of child/socket/lease survives cancelled
connection/setup and failed ownership audit; shutdown handles both initialized and
unfinished workers with bounded wait/kill/reap. Launch intent and owned PID are
persisted separately from completed handoff. Actual child tests cover cancellation,
audit failure, second-launch refusal and socket removal. Host crash/stale resources
and external GPU cancellation remain unresolved; no permission or wire change occurs.

Inference startup ownership validation: all mandatory quality checks pass with
306 tests (285 library, 21 integration). The expanded smoke workflow also checks
one demo inference child from launch intent through owned PID, handoff and matching
reap; debug execution passes. Optimized installed verification is pending for this
new source. This does not establish host-crash cleanup or real model behavior.

Optimized installed verification of `fcd6465` now passes the expanded CLI/service
workflow and demo inference ownership/reaping checks. Exact build identity and
hashes are recorded in `RELEASE_AUDIT.md`. The installed-source gap is closed for
this commit; host-crash cleanup, general module lifecycle and live-model gates
remain incomplete.

Tool-worker audit recovery: **Implemented** in `src/recovery.rs`, sharing registry
descriptor validation and the existing operator-only recovery command. New tool
audit records are explicitly versioned; original unversioned records remain
recoverable. `src/recovery/tool_tests.rs` covers mixed inference/math/SQL recovery,
terminal-session exclusion, idempotence, preserved answers/charges, malformed or
oversized metadata, audit-failure rollback and actual live socket worker lease
exclusion. Combined batches retain 10,000-session and five-second limits. No tool
replay, PID termination, external-effect proof or stale-file removal is implied.

Mixed worker recovery validation passes all mandatory checks with 309 Rust tests
(288 library, 21 integration). The expanded smoke workflow also confirms normal
worker completion leaves no recoverable sessions. Optimized installation evidence
must be refreshed after this source change. Stale-file cleanup, broader module
lifecycle and live-model evaluation remain open.

Optimized installed validation of `dbb9c47` now passes the expanded workflow,
including CLI recovery after clean completion. `RELEASE_AUDIT.md` records hashes
and evidence. The installed-source gap is closed for this commit; the mixed/live
recovery regressions establish abandoned-session behavior. Stale filesystem
resources and broader module lifecycle remain incomplete.

Worker-file cleanup: **Implemented** for journaled same-binary socket directories,
with operator-reviewed checksum-bound preview/apply under an exclusive runtime
lease. `src/recovery/worker_files.rs`, `src/recovery/worker_files_tests.rs` verify
actual host death, lease exclusion, approval, workspace/checksum binding,
symlink/type/mode/identity/extra-file refusal, audit failures, malformed/oversized
journals and pagination. All 318 Rust tests and mandatory quality checks pass.
Installed validation is pending. Older unjournaled resources, creation-to-journal
crash windows, hostile same-user races and automatic crash cleanup remain outside
the guarantee; filesystem deletion cannot roll back with SQLite audits.

The cleanup source `054505c` subsequently passed fresh optimized installation and
the expanded CLI/service smoke workflow, closing the pending installation gap.
`RELEASE_AUDIT.md` records source/binary/script hashes and retained evidence.
Actual host-death deletion is verified by the Rust child regression; the installed
workflow exercises empty reviewed plans. Broader module lifecycle and live model
gates remain incomplete.

Tool version rollback: **Implemented** for trusted host registrations in
`src/fabric/tools.rs`, covering arithmetic and SQL. Activation preserves original
backend identity and existing task pins; historical IDs cannot change capability.
Tests cover SQL/math rollback, active conflicts, shutdown, immutable redefinition
and rollback at the 128-version ceiling. All mandatory checks pass with 321 tests
(300 library, 21 integration). History retains inactive backends/queues until
shutdown; health checks, general executable lifecycle and residency quotas remain
partial. Real direct/managed Ollama generation now passes, including independent
sorting proof and confirmed child reaping; see `LIVE_INFERENCE.md`. This supersedes
older statements that all compatible real inference remains unavailable.

Inference backend rollback: **Implemented** in `src/fabric.rs`, with trusted host
activation/discovery of immutable retained versions. `src/fabric/tests.rs` covers
original backend/queue reuse, version-pinned role routes, conflict refusal,
shutdown/unknown versions and the 128-version bound. All mandatory quality checks
pass with 324 tests (303 library, 21 integration). Retention keeps inactive backend
references until shutdown; remote health/grant renewal, automatic executable
lifecycle and real weight residency/switching remain partial. See `MODELS.md`.

Optimized installation of `dec46fa` and the complete installed smoke workflow
passed. A separate installed socket service executed one real Ollama sorting task
with negative/duplicate values, independent Rust proof, confirmed worker reaping
and service/socket cleanup. See `RELEASE_AUDIT.md` / `LIVE_INFERENCE.md` for hashes,
times and scope. Live model switching and controlled performance studies remain
unverified; model advertisement is not loaded-weight attestation.

Transport-bound inference identity: **Implemented** in `src/backend.rs`,
`src/fabric.rs` and `src/fabric/unix.rs`. Initial brokers preserve actual connected
module metadata; registration rejects changed identity/version/roles before
inference. Regression tests cover no-dispatch rejection, invalid initial metadata
and actual child-process discovery/role restriction. This closes metadata
relabeling, not trusted-code compromise, executable attestation or weight identity.

The identity slice passes all mandatory checks with 325 tests (304 library and
21 integration). Optimized installed verification is pending for this source;
prior installation evidence remains tied to its recorded earlier source.

Optimized installation and installed CLI/service smoke passed for `8506352`;
`RELEASE_AUDIT.md` records hashes and evidence. The pending installed-source gap
above is closed. The leading summary table now reflects implemented authenticated
process transports and live generation without implying general actor provisioning,
loaded-weight attestation or model quality. Broader lifecycle work remains partial.

Event-module rollback: **Implemented** in `src/events/modules.rs`. Trusted hosts
can restore immutable registered metadata for future subscriptions; existing
observer/process grants and durable replay retain pinned filters. Regressions
cover rollback after unregistration, live/replay filtering, version forgery,
unknown selectors, 128-version history and 32-active-module reactivation bounds.
The registry remains process-local; executable health/provisioning and persistent
general module lifecycle remain partial. See `EVENTS.md`.

The event rollback source `2ac215b` also passed fresh optimized installation and
the installed CLI/service smoke workflow. See `RELEASE_AUDIT.md`; process-local
metadata rollback is verified, while automatic general executable lifecycle and
controlled benchmark studies remain incomplete.

A separate installed STANDARD-mode live interpretation attempt failed before
solver execution: the provider returned empty text under the interface's fixed
128-token reservation. The runtime returned visible failure and retained one
charged call; no structured-goal success is claimed. `LIVE_INFERENCE.md` records
actual settings and timing. This is a newly observed language integration gap,
not evidence against the independently verified direct/socket sorting cases.

Language-interface allocation: **Implemented** in `src/config.rs`, `src/engine.rs`
and `src/main.rs`. Operators can set `interface_tokens` / `configure-local
--interface-tokens` (default 128, bounded 1–4096) within original cumulative
ceilings. `src/config.rs` and `src/language.rs` regressions cover legacy settings,
invalid limits, exact backend allocation/charges and no dispatch when it cannot
fit. All mandatory checks pass with 329 tests (308 library, 21 integration).
Larger allocations do not guarantee valid provider output or permission changes.

A real follow-up using 384 interface tokens reached the unchanged 90-second
inference timeout before producing a structured goal. The reservation/timeout
were enforced and one call stayed charged, but live interpretation remains
unverified. `LIVE_INFERENCE.md` records both attempts; neither is a benchmark or
successful language-interface result.

Provider reasoning control: **Implemented** in `src/config.rs`, `src/backend.rs`,
`src/main.rs`, `protocol/bootstrap.fbs` and `src/fabric/unix/bootstrap.rs`.
Operator-only configuration accepts a fixed optional effort value; default HTTP
requests/configuration identities remain unchanged. Versioned COGB v2 preserves
explicit settings through workers and rejects incompatible shapes instead of
silently ignoring them. Direct/managed HTTP and codec regressions plus schema
conformance pass. All mandatory checks pass with 330 tests (309 library, 21
integration). Provider compliance and useful real interpretation remain separate
live evidence; no compute ceiling or model-quality claim follows from this option.

Initial planner admission now reserves a whole-goal solver using remaining
calls/tokens/prompt bytes after interface/expert work, rather than original totals.
`src/reasoning.rs` records `admit_planning` / `skip_planning`; its minimum prompt
reservation includes both planner and empty-strategy solver framing. Decomposition
and exploration retain their separate stricter reservations. A new language
regression covers independently constrained calls, tokens and prompt bytes after
malformed interpretation, preserving the original goal and reaching a solver.
All mandatory checks pass with 331 tests (310 library, 21 integration). This fixes
the scheduler gap observed in the effort trial; live structured interpretation
itself remains unverified.

Fenced intent compatibility: **Implemented** in `src/language.rs`. Only one exact
outer `json` fence is accepted in addition to raw JSON; byte/schema/permission
checks are retained. Tests reject surrounding instructions, multiple JSON values,
wrong/nested/incomplete fences and forged permission fields, and demonstrate a
fenced advisory objective cannot replace the user's goal before solver dispatch.
All mandatory checks pass with 333 tests (312 library, 21 integration). Earlier
effort-trial trace inspection confirmed its interface output was a fenced valid
object; the original strict parser correctly rejected that formerly unsupported
wrapper. Real final-path success still requires separate observation.

The corrected real structured-language path now succeeds on the same request:
`3b186cd` accepts the exact fenced intent, records advisory structured metadata,
skips unaffordable planning and dispatches the whole-goal solver within two calls
and 768 output reservations. CLI exit is zero; prose remains unverified. The
managed child is confirmed reaped. `LIVE_INFERENCE.md` records timing and limits.
This closes the single-case live integration failure, not robust interpretation
quality, synthesis or equal-budget reasoning evaluation.

Structural sorting comparisons: **Implemented** in `src/benchmarking.rs` as
`integer_sort_v1`, reusing fresh compiled checks over explicit `sort:` objectives
and omitting expected strings. Unit regressions cover duplicates, direction,
integer extremes/types, wrong answers and cross-criterion rejection. The actual
HTTP comparison regression covers sorting and ordering through all six arms.
`examples/sorting_comparison.jsonl` is demonstration data, not a reviewed held-out
study. Reports disclose shared verifier implementation; live comparative quality
and independent corpus review remain incomplete.

Runtime `3eb870c` passes optimized installation and installed smoke. Expanded
`tests/resume_worker.rs` now verifies saved provider controls survive mutable host
configuration changes for both normal resumed worker completion and cancellation;
all 334 tests and mandatory checks pass. Current operator settings remain unchanged.
This strengthens existing task pinning evidence, not general arbitrary-phase replay
or provider hidden-compute attestation. See `RELEASE_AUDIT.md` / `QUALITY_REVIEW.md`.

Bounded structured role JSON: **Implemented** via `src/structured.rs`, reused by
language interpretation, planner graphs and critic advice. Existing strict schemas,
graph validation and recursion limits remain enforced. Tests execute dependent
subtasks from a fenced graph (including durable graph validation), perform a
bounded fenced-critic revision, and reject forged permissions/surrounding prose/
multiple objects. All mandatory checks pass with 335 tests (314 library, 21
integration). This adds compatibility with observed model formatting; useful
live recursive gains still require measurement.

### Explicit public comparison workflow checks — 2026-10-12

Implemented: `compare --workflow-check` runs the existing six profiles without
falsely declaring public examples held out. `src/main.rs` and
`src/benchmarking.rs` require exactly one purpose; report fields distinguish
workflow checks from operator-attested held-out runs. Dataset integrity pins,
resource bounds and cancellation remain enforced. Regression:
`workflow_checks_are_explicit_and_never_attest_held_out_review` verifies all six
results and rejects missing/conflicting purposes. Independent held-out datasets
and live small-model comparative measurements remain incomplete.

### Observed inference adapter availability — 2026-10-12

Implemented: `InferenceBackend::availability` defaults to unknown; Unix adapters
report connected, in-use or permanently closed from their local stream slot.
`Broker::statuses` exposes bounded immutable-version metadata, active selection
and observed state without provider probes. Closed transports cannot be
reactivated, enter new task routes or dispatch from already pinned routes.
Metadata discovery/history retains registered versions for diagnosis. Regression
`closed_transport_is_visible_but_cannot_be_reactivated_or_routed` uses an actual
forged Unix response to prove closure, route refusal, inventory and shutdown.
Remote health, weight identity, automatic reconnection and residency scheduling
remain incomplete. In-use is not failed; connected is not provider health.

### Tool transport availability — 2026-10-12

Implemented: arithmetic and SQL backend availability defaults to unknown; Unix
adapters distinguish connected/in-use/closed local stream state. The shared tool
broker exposes retained-version status and refuses closed adapters during new
pinning, rollback activation and pinned dispatch. Existing malformed-frame and
forged-response regressions now verify both capabilities' inventory and refusal.
Grant validity, remaining calls and remote health remain separate checks; a
present stream is not proof of any of these. No implicit reconnection or permission
renewal is added. General lifecycle and residency scheduling remain partial.

### Expired/exhausted task-scoped capability routing — 2026-10-12

Implemented: local availability additionally distinguishes expired session deadlines
and exhausted call grants for Unix arithmetic/SQL adapters. New pins, historical
activation and already-pinned dispatch refuse these terminal observations without
queueing or renewing authority. Closed-stream state retains precedence when both
closure and expiry are observed. In-use adapters remain routable until their
absolute deadline; dispatch still validates the original grant. Paused-clock
expiry regressions prove refusal with unused connected streams and zero charges;
existing two-call tests now prove exhausted-state pin/activation refusal. Remote
health and automatic provisioning/reconnection remain separate incomplete work.

### Managed worker availability propagation — 2026-10-12

Implemented: inference `ProcessTask` delegates initialized availability to its
Unix adapter. Managed arithmetic/SQL wrappers share adapter observations and
report expired original host deadlines before worker initialization. Uninitialized
workers otherwise report unknown; observation never launches a child. Actual
socket/same-binary-child regressions verify connected state and closed state
after confirmed shutdown for all three paths; expired unlaunched tools remain
without initialized workers. This closes the wrapper propagation gap, not general
process health, startup-failure recovery or automatic grant renewal.

### Terminal managed-worker shutdown — 2026-10-12

Implemented: managed inference/arithmetic/SQL wrappers mark shutdown terminal
before awaiting cleanup. Later initialization/dispatch refuses the stopped state;
an initialization mutex coordinates successful handoff with shutdown inspection,
so an initialized child cannot fall between startup and managed ownership checks.
Observation reports stopped wrappers unavailable without claiming child completion.
Repeated shutdown remains allowed for cleanup retries. Regressions prove shutdown
before first use causes no worker initialization or launch audit for all three
capabilities. Existing interrupted-startup and actual child-reaping tests remain.
This does not stop external provider computation or provide hostile-code isolation.

### Real provider-managed model selection transition — 2026-10-12

Verified against the existing Ollama server: one managed FAST sorting task using
Q6 followed by one using the existing `latest` quantization, both independently
checked and reaped. Provider residency inventory changed to the respective
reported digest after each call. See `LIVE_INFERENCE.md` for exact limits, model
identities and timing. This strengthens configurable multi-model integration;
automatic framework residency ownership, native LM Studio switching, adapter
hot-swapping and small-model performance remain unverified/incomplete.

### Explicit alternative inference candidates — 2026-10-12

Implemented: trusted hosts can `register_candidate` with immutable bounded routing
rank. Only explicitly ranked modules may overlap inference roles; ordinary exclusive
registration retains conflict rejection. New snapshots choose the lowest rank,
then module ID, skipping observed unavailable adapters. Registered replacement and
rollback preserve each version's rank, backend and bounded queue; running task
pins never migrate or silently retry an alternative. Status metadata includes rank.
Regressions cover registration-order independence, role scope, exclusive conflicts,
rank bounds, transport-identity forgery, replacement/rollback and closed-route
fallback only for new tasks. Rank is declared operator policy, not learned utility
or measured cost. General plugin provisioning, residency and adaptive cost-aware
scheduling remain incomplete.

Ranked routing transport regression now additionally registers two authenticated
Unix adapters simultaneously, verifies lowest-rank task pinning, unregisters that
candidate and checks new tasks use the remaining adapter while existing pins
still send exact original module/version FlatBuffers headers. Forged version
relabeling is refused. This is actual local transport evidence, not remote plugin
attestation, process provisioning or model quality measurement.

### Explicit alternative tool candidates — 2026-10-12

Implemented: trusted Rust hosts can register ranked arithmetic and SQL alternatives
using `register_candidate` and `register_sql_candidate`. Lowest rank then module ID
selects new pins; unavailable transports are excluded. Existing handles retain their
original backend/version. Exclusive registrations still reject overlap, rollback
retains immutable rank, and activation/registration enforce 32 active modules and
128 historical versions. Rank cannot exceed 1,000,000. Capability and transport
identity checks precede registration; task/actor authorization remains mandatory.
Three new regressions cover arithmetic replacement/rollback and authorization,
SQL typed dispatch and capability separation, and capacity rejection without
consuming rejected versions. This is operator-declared routing, not an executable
plugin loader, adaptive utility estimator or automatic permission grant.

### Empty local-model answer diagnostics — 2026-10-12

Implemented: bounded HTTP parsing distinguishes empty responses with a reported
output limit from reasoning-only responses, with operator troubleshooting advice.
Internal reasoning is neither returned as an answer nor printed in diagnostics.
Regression fixtures cover length termination, null answer with reasoning and blank
answer; fixtures now construct explicit configuration instead of reading saved
operator settings. Automatic retries and budget expansion remain absent.

Live evaluation update: the user-selected LM Studio `qwen/qwen3-8b` completed all
six public workflow arms on one sorting case, with normal cleanup of all six
workers. Tools/recursive arms verified in approximately 0.40/0.34 seconds;
collective/memory/experts took approximately 1.8–2.0 seconds. Baseline output
contained correct numbers but failed the required format. This is not held-out
quality evidence; empty-corpus memory/expert labels and first-cycle recursive
success establish no retrieval, specialization or revision gain. See
LIVE_INFERENCE.md for exact usage and limitations. Two fresh default-parallel
350-test suites passed; prior intermittent failures remain unexplained.

Native residency update: real read-only LM Studio v1 inventory now succeeds and
reports Qwen3 8B loaded at context capacity 4096, with three other installed keys
unloaded. This closes the real unauthenticated inventory smoke gap only. Native
load/unload mutations, authenticated native access, physical weight attestation
and automatic ownership-aware residency scheduling remain unverified/incomplete.

Security update: a live Qwen3 8B retrieved-document trial produced the attacker's
requested marker in an unverified answer. No requested shell execution, promotion
or permission change occurred. Answer integrity therefore failed despite runtime
authority containment; stronger source/instruction separation and broader
behavioral evaluation remain release work. Installed runtime `2d2fb3b` passed
the full existing optimized installed-binary smoke; clean-OS setup is unverified.

Reference boundary mitigation: all inference roles receive fixed instructions to
ignore commands in external evidence. System wording alone failed the observed
live attack. Quoted nonempty reference serialization plus a final original-goal
reminder resisted that same attack once in fresh state; answer stayed unverified.
A regression checks that forged reference newlines/section endings remain inside
one JSON string. Empty-context prompts preserve their hashes/cost. Old nonempty
reference checkpoints may fail prompt-hash validation rather than silently resume
with changed prompts. This improves separation; it is not a general injection
safety proof, and broader adversarial evaluation remains incomplete.

### SQL worker backpressure — 2026-10-12

Implemented: replace immediate rejection on the third executing request with a
bounded six-request admission ceiling, two execution slots and a one-second worker
queue deadline. Excess admission still fails closed, queued cancellation releases
its admission permit, and executing blocking work retains both permits until
completion. Oversized raw input is rejected before admission. New independent
capacity fixtures cover admission exhaustion, queue timeout, cancellation, permit
restoration and successful queued execution. SQLite restrictions and task tool
charges remain unchanged. This addresses a demonstrated contention path; prior
intermittent SQL transport failures were not instrumented, so do not claim every
historical failure's cause is proven. Runtime-lease test contention is separate.

Recovery lease evidence: duplicated open-file descriptions keep recovery blocked
until the last descriptor closes, even after the original owner drops its handle.
A kernel regression verifies close-on-exec flags, zero interruption mutations
while blocked, and idempotent recovery after final closure. Historical transient
contention remains unexplained; the runtime deliberately retains fail-closed
ownership checks. Current mandatory tests pass 354 cases.

Installed-path update: current runtime `161dea5`, built from source `9b0cf59`,
passes the complete offline installed smoke and a real Qwen3 8B request submitted
through the installed authenticated socket service. Sorting independently verified,
managed worker reaped normally, service exited zero and removed its socket.
See QUALITY_REVIEW.md and LIVE_INFERENCE.md for hashes, paths and exact metrics.
This closes latest installed CLI/service/local-model smoke coverage; it does not
close clean-OS installation, general inference quality or broader release gaps.

### Explicit checkpoint prompt format — 2026-10-12

Implemented: new reasoning contexts declare `quoted-references-v1`; recovery
rejects unknown/null declarations and reports incompatible prompt hashes explicitly
before dispatch. Compatible undeclared snapshots remain accepted only with exact
current hashes. A positive regression verifies declaration preserves original
query/context and cumulative calls/tokens; negative cases cover unknown/null
format and changed prompt hash. Existing repeated recovery still validates source
bytes and chronological reservations. Automatic migration of old nonempty unquoted
reference prompts remains deliberately unsupported rather than silently weakening
the new reference separation or substituting task context.

Language-interface update: fresh public CLI settings with Qwen3 8B exercised real
interpretation, planning admission and whole-goal solving. Initial interpretation
omitted an explicit command prohibition; original-goal retention and runtime
authority remained intact. A stronger fixed interface instruction preserved that
constraint on one repeat trial. This is measured partial extraction quality,
not a guarantee of arbitrary constraint retention. README now includes the tested
384-token interface/no-thinking setup; detailed evidence is in LIVE_INFERENCE.md.

### Reproducible FlatBuffers compiler bootstrap — 2026-10-12

Implemented: `scripts/build-flatc.sh` clones public upstream v25.12.19, verifies
commit `7e163021e59cca4f8e1e35a7c828b5c6b7915953` before executing CMake, builds
with two jobs and checks the exact compiler version. It requires a new absolute
destination, refuses existing paths and retains source/build evidence on failure.
README documents durable user-cache installation and the FLATC export; build errors
point to the script. CI now invokes this same bootstrap rather than a separate
unverified-tag-only recipe. Actual public clone/build and negative argument/path
checks passed locally. Existing Git/CMake/C++/Make prerequisites were already
installed; this is not proof of fresh-OS prerequisite installation or authenticated
upstream provenance beyond the reviewed commit pin and HTTPS transport.

Hosted setup evidence: GitHub run 38295462158 passed for bootstrap commit
`fb7f9ec` in 3m21s, including verified compiler bootstrap, mandatory Rust checks,
CLI workflow smoke, security evaluation and training preflight. This verifies
fresh hosted checkout/dependency setup on a runner with existing prerequisites,
not a fresh end-user OS install or live GPU/security/quality evaluation. See
QUALITY_REVIEW.md for the immutable commit and run link. CI Node/runner-image
migration warnings remain maintenance work.

### Typed operator tool discovery — 2026-10-12 (Implemented)

`tool-modules` / `tool-modules --socket` expose append-only typed FlatBuffers host
inventory with capability, immutable version/rank, active selection and local
availability. Bounds: 128 versions/32 active IDs. Validation denies malformed or
conflicting registries and unknown enums; legacy replies omit the vector safely.
Borrowed fields are bounded before copying; envelope verification bounds shared
reference amplification. Thirteen wire tests, real socket discovery and expanded
CLI/service smoke pass. All 358 tests (337 library, 21 integration), formatting,
all-target check and Clippy with warnings denied pass with flatc 25.12.19; schema
conformity passes. Smoke used the debug binary, not a new optimized installation.
No worker is launched and no capability is granted by discovery. See
QUALITY_REVIEW.md for evidence. Automatic approval access recovered following the
operator's instruction; verification and private Git synchronization resumed.

Optimized installed validation for source `75e7e8a` now passes the expanded smoke
and direct Qwen3 8B independently checked sorting with thinking disabled. The
initial default-thinking request exhausted its output reservation, correctly
reported as failure. See RELEASE_AUDIT.md for actual results and remaining gates.

### Inbound inventory security regressions — 2026-10-13

Implemented in `src/wire.rs`: raw FlatBuffers fixtures bypass the trusted encoder
and confirm rejection of duplicate versions, multiple active versions, capability
identity changes, invalid ranks, excessive active/history counts and exclusive
capability overlap. Shared-string cases now separately exercise early field-bound
rejection and the 1 MiB apparent-size verifier limit using a physically small
frame with 128 repeated references. No runtime permission or behavior changes.

All 359 tests (338 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass; 14 wire tests pass. Logs:
`/tmp/cogfabric-inbound-all-tests.log`, `/tmp/cogfabric-inbound-inventory-tests.log`.
An initial Clippy pattern-style warning was corrected before the final pass.
These deterministic tests strengthen malformed-frame/resource-exhaustion evidence;
they do not establish general plugin isolation or neural safety.

### Clean installation prerequisites — 2026-10-13 (Implemented for Arch container)

A fresh rootless official Arch container passed package installation, pinned
compiler bootstrap, locked builds, all 359 tests, formatting, Clippy and optimized
CLI/service smoke without the host Cargo cache. README now includes build/smoke
prerequisites; `INSTALLATION_VALIDATION.md` records a reproducible image-pinned
workflow. QUALITY_REVIEW.md retains actual versions, hash and terminal log.
CachyOS-specific desktop/GPU setup and broader release requirements remain open.

### Typed inference lifecycle discovery — 2026-10-13 (Implemented)

`inference-modules` / `inference-modules --socket` expose all retained adapter
versions, roles, active selection, local availability and immutable routing ranks.
Append-only FlatBuffers control 8 preserves old empty-vector defaults. Inventory
limits, borrowed field bounds, duplicate versions, active version conflicts, role
conflicts, ranks and availability enums are validated. Ranked alternatives retain
the existing broker policy; observation grants no capabilities or model residency
ownership. General independent executable provisioning and adaptive scheduling
remain partial. Files: `protocol/cognitive.fbs`, `src/wire.rs`, `src/service.rs`,
`src/main.rs`, `scripts/installed-smoke.sh`.

All 361 tests (340 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass using flatc/crate 25.12.19. Sixteen wire tests
cover state/history round trips, malformed inbound inventories and old defaults.
Actual socket discovery and expanded CLI/service smoke pass on the debug binary.
Compiler `--conform` against the previous committed schema passes. Logs:
`/tmp/cogfabric-inference-inventory-all-tests.log`,
`/tmp/cogfabric-inference-inventory-wire.log`,
`/tmp/cogfabric-inference-inventory-smoke.log`; smoke evidence
`/tmp/cogfabric-installed-smoke.vEAe2u`. Optimized installed validation for this
new revision remains pending; the clean Arch result applies to source `35c4bdc`.
No live inference or neural safety evaluation was performed for this change.

The inference-inventory optimized installation gap above is now closed for
`da2a8b9`: expanded installed CLI/service smoke passes. RELEASE_AUDIT.md records
the binary hash, log and evidence; broad executable provisioning remains partial.

## Comparison output-contract parity — 2026-10-13 (Implemented)

The running 100-case public Qwen workflow at source `da2a8b9` exposed an ablation
fairness limitation: `Session::prompt_base` omitted the structured output contract
when executable verification was disabled for the model-alone profile. Tool-enabled
profiles received explicit array-format instructions. Some ordering answers were
semantically valid arrays wrapped in Markdown fences and were rejected by the
strict checker. Consequently this run must not support a causal reasoning-gain
claim, even after it completes. Its fixed executable remains unchanged.

Source now derives output contracts from the objective independently of
verification permission. A capturing-backend regression confirms identical solver
prompts and one call for both settings, with successful verification evidence only
in the enabled case. It also checks ordering contract parity with empty/reference
contexts and excludes the expected sorted answer from prompts. The first fixture
incorrectly expected no verification audit at all in the baseline; it was corrected
to count successful proof events, preserving the existing unverified audit. The
regression passes: `/tmp/cogfabric-baseline-contract-unit.log`. All 362 tests (341 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass. Full tests and expanded CLI/service smoke used
`CARGO_TARGET_DIR=/tmp/cogfabric-baseline-contract-target-20261013` to preserve the
running comparison executable. Logs: `/tmp/cogfabric-baseline-contract-all-tests.log`,
`/tmp/cogfabric-baseline-contract-smoke.log`; evidence
`/tmp/cogfabric-installed-smoke.jWuzXU`. The fixed isolated debug executable hash is
`77f79a6254e47d6cd82a342aec5b8cc8c9f9f8344c3db701f04218bed8402ae4`.
The running comparison binary SHA-256 remains `c5dfdb5bce170ec6697228c70c355c197913ff9f185ee706fb8658bdc83f0890`.
A corrected live rerun is required before reporting comparative gains.

### Arithmetic and SQL output contracts — 2026-10-13 (Implemented)

`src/reasoning.rs::Session::prompt_base` now states numeric-only arithmetic output
and the SQL columns/rows JSON shape for explicit tool-shaped objectives, without
computing answers or executing a tool. Format instructions apply independently of
verification permission. A real Engine model-alone regression captures both
prompts, confirms expected answers are absent, checks exactly one inference call,
zero tool calls, no dispatch/worker events, empty evidence and unverified status.
This extends sorting/ordering prompt parity; pre-fix arithmetic/SQL exact-match
scores also require the format caveat before any comparative-gain claim.

All 363 tests (342 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass. Full tests and CLI/service smoke used the
isolated target to preserve the ongoing original comparison executable. Logs:
`/tmp/cogfabric-tool-format-parity-unit.log`,
`/tmp/cogfabric-tool-format-parity-all-tests.log`,
`/tmp/cogfabric-tool-format-parity-smoke.log`; evidence
`/tmp/cogfabric-installed-smoke.7G8s1C`. Fixed isolated debug binary SHA-256:
`a2012ac53ec77dee168d4e2aadb1dca7067864b419a3ddd6e0db8f47e5c792d0`.
Original running-study executable remains checksum
`c5dfdb5bce170ec6697228c70c355c197913ff9f185ee706fb8658bdc83f0890`.
Additional instruction bytes count toward existing prompt bounds. No token budget,
permission or runtime proof is expanded. Older saved contexts with different
prompt hashes fail closed; there is no automatic migration of changed prompts.
A fresh corrected live study remains required.

### Full public corpus diagnostic — 2026-10-13 (Experimental)

All 600 profile tasks completed at source `da2a8b9`; raw answers and category/resource
summary are retained in `evaluation/results/`. Output-contract asymmetry and
format-sensitive scores prevent causal reasoning-gain claims. The corrected
`9fec58b` full rerun is active with frozen executable/fresh state; no results are
asserted yet. Independent label/split review and model-quality/security evaluation
remain incomplete. Both parity commits passed hosted CI.

### Compiled role prompt fingerprints — 2026-10-13 (Implemented)

`src/backend.rs` derives a versioned canonical SHA-256 from all compiled default
role/system prompts and the shared reference-data boundary, using the same helper
as actual local HTTP system messages. `Session` records it in original execution
settings before dispatch. `src/recovery/continuation.rs` requires a matching
fingerprint before continuation. Missing/null/mismatched values fail closed
without adding events or restoring permissions. Older records remain inspectable;
there is no automatic migration or charge refund. This is compiled-default
identity, not arbitrary-backend/provider behavior, weight or same-owner attestation.
See RECOVERY.md for the intentional legacy-continuation limitation.

All 364 tests (343 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass. Full tests and CLI/service smoke used an
isolated build directory. Logs: `/tmp/cogfabric-prompt-fingerprint-all-tests.log`,
`/tmp/cogfabric-prompt-fingerprint-smoke.log`; evidence
`/tmp/cogfabric-installed-smoke.PEsR38`. The corrected study's frozen executable
checksum remains unchanged. Host compilation overlapped part of that public
workflow study, so its timing remains uncontrolled and supports no causal latency
claim. No model call or permission was added by the fingerprint.


Corrected public Qwen3 8B output-contract parity workflow completed on 2026-10-13:
600/600 tasks at source `9fec58b`, 988.471 seconds, strict matches per 100 of
7 alone, 21 tools, 21 recursive, 51 collective, 48 memory and 50 experts. All 500
inference workers stopped normally. Full raw answers and category/resource summaries
are retained in `evaluation/results/`; see its README for exact hashes, configuration,
status interpretation and limitations. This supersedes earlier pending-run statements
only; independent label review, held-out evaluation, 1–3B/larger baselines and causal
quality/performance evidence remain incomplete. Host compilation overlapped the run.


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


Retirement concurrency verification (2026-10-13): the gated regression
`retirement_preserves_active_and_queued_calls_but_not_shutdown_immunity` in
`src/fabric/tests.rs` retires an original backend while one request is executing
and another is queued. New calls select the replacement; original calls complete
on their original backend. A separate shutdown branch cancels both original calls
without dispatching the queued request, then verifies the retired backend releases.
All 368 tests (346 library, 22 integration), formatting, all-target check and
Clippy with warnings denied passed. Log:
`/tmp/cogfabric-retirement-concurrency-tests.log`. This is deterministic lifecycle
concurrency evidence; runtime source and optimized installation are unchanged.

Hosted tool-retirement CI
[38319883518](https://github.com/DrapNard/cognitive-fabric/actions/runs/38319883518)
completed successfully for `a665df998699000e0f64839b06582d4411729cc1`; watcher
exited zero. Compiler bootstrap, mandatory checks, tests, CLI/service smoke,
security probes and offline training preflight passed. Local watcher log:
`/tmp/cogfabric-tool-retirement-hosted-ci.log`. This supersedes its earlier pending
status, without changing broader provisioning, residency or evaluation blockers.


Registry cleanup lock ordering (2026-10-13): inference and tool brokers now detach
retired versions and commit shutdown state under their write lock, then release
the lock before dropping backend handles. Trusted backend destructors may inspect
committed registry metadata without deadlocking on that same lock. Regression
fixtures cover retirement and shutdown for inference, arithmetic and SQL. The
initial inference regression reproduced cleanup under the held lock (failed test,
no hang); a SQL fixture signature compilation error was corrected before final
validation. All 370 tests (348 library, 22 integration), formatting, all-target
check and Clippy with warnings denied then passed. Log:
`/tmp/cogfabric-registry-cleanup-tests.log`; initial reproduction:
`/tmp/cogfabric-retirement-cleanup-before.log`.

Files: `src/fabric.rs`, `src/fabric/tests.rs`, `src/fabric/tools.rs`. Task pins,
shutdown flags, identity tombstones and permissions are preserved. Custom host
cleanup code is still trusted and may block or panic for unrelated reasons; this
is not a cleanup timeout, process sandbox or provider-unload guarantee. Optimized
installation evidence predates this source change and requires refresh before
release. General module provisioning and independent evaluations remain open.


Optimized installation and full installed CLI/service smoke passed for cleanup
source `b81fe34`. Fresh-state managed Qwen3 8B produced a STANDARD ownership
explanation (unverified, two calls) and a FAST sort result independently checked
by `integer_sort/v1` (one call). Both workers were reaped normally. Exact hashes,
budgets, traces and limitations are in `QUALITY_REVIEW.md` and `LIVE_INFERENCE.md`.
This refreshes installed-source verification; broader lifecycle and evaluation
requirements remain incomplete.


Implemented local queue occupancy tie-breaking for approved equal-rank inference,
arithmetic and SQL alternatives (`src/fabric.rs`, `src/fabric/tools.rs`). New pins
prefer fewer occupied local channel slots after operator rank; IDs settle remaining
ties. Gated regressions verify actual buffered work, rank precedence, replacement,
old-pin execution and return to ID order after draining. Adaptive calibrated cost,
remote load, residency scheduling and reservation-based balancing remain Partial /
Not implemented. All 372 tests and mandatory checks passed; see QUALITY_REVIEW.md.


Implemented affordable optional retrieval under zero tool ceilings. In
`src/engine.rs`, `max_tool_calls=0` audits skipped retrieval and permits plain
inference with no document/memory retrieval. Arithmetic, SQL and independent
structured checks still fail authorization/budget admission before tool dispatch.
The regression covers plain generation, excluded retrieved content and negative
dispatch; zero call/token/input/cycle coverage remains in `src/reasoning.rs`.
All 373 tests and mandatory checks pass; a real managed Qwen explanation used one
model call and zero tools. See CONFIGURATION.md, QUALITY_REVIEW.md and
LIVE_INFERENCE.md. Free-form language restrictions are still advisory.


Implemented retrieval-free explicit arithmetic/inline SQL admission in
`src/engine.rs`: tool-enabled Full/Memory/Experts profiles skip unrelated retrieval
and execute under one tool allowance, with original policy and host result checks.
Zero-tool denials remain enforced. New Engine coverage verifies no retrieved
memory/document evidence; real CLI/service worker fixtures verify one-call
execution, reaping and zero-budget launch refusal. Progress and comparison
resource expectations match actual charges. All 374 tests and mandatory checks
pass; debug CLI/service smoke passes. See QUALITY_REVIEW.md for evidence.


Installed verification refreshed for `8b30fd1`: expanded smoke exercises actual
one-tool arithmetic/SQL CLI/service workers, preserved restored tool ceilings and
zero-tool managed demo inference over direct and socket paths. Zero-budget socket
arithmetic is denied before worker launch. Runtime source remains the 374-test
implementation. See QUALITY_REVIEW.md for exact artifacts and pending hosted scope;
this does not complete general module provisioning or independent evaluation.

## Read-only continuation admission preview (2026-10-13)

Implemented `resume-preview TASK_UUID` in `src/main.rs` and
`src/recovery/continuation.rs`, sharing existing admission and managed workspace
checks with approved resume. Reports bounded counters and subtask states without
raw task/context/answer text. Does not write task/audit state or launch inference.
Uses a cooperative exclusive lease (which may create its lease file), not a new
permission grant. Resume always revalidates; existing continuation phase and
original-clock restrictions remain. Regression coverage verifies unchanged
status/event count, charged budgets, live-lease refusal and missing prompt
fingerprint refusal. No broader continuation or provider attestation is claimed.

Private GitHub CI runs 38322336100 (`5345f67`) and 38322010060 (`8b30fd1`)
both completed successfully before this change.

Status: **Implemented** for existing supported continuation checkpoints.
Validation: formatting, all-target check, Clippy with warnings denied, and
375 Rust tests (353 library, 22 integration) passed with flatc 25.12.19.
Socket/HTTP tests required execution outside sandbox network restrictions; the
initial restricted run failed on those operations. Full results are recorded in
`/tmp/cogfabric-resume-preview-tests.log`.

## Read-only preview CLI startup correction (2026-10-13)

Status: **Implemented**. Actual CLI integration exposed that common startup
ran `Config::prepare` and database bootstrap before preview, repairing directory
permissions and permitting initialization writes. Preview now branches before
that startup and uses `Store::open_existing_read_only` with SQLite read-only and
query-only flags, the current schema requirement and a five-second busy timeout.
No migrations or bootstrap run. Existing checkpoint validation remains shared.

`tests/resume_worker.rs` now exercises preview through the actual executable
before both successful and cancelled managed continuation. It checks that an
unsafe workspace is rejected without repairing permissions, safe preview leaves
task status/events unchanged, and no HTTP request reaches the provider. Another
negative test proves missing state is not created and old schemas are not
migrated. This corrects the previous CLI claim; its direct library tests alone
did not cover startup behavior. SQLite may use normal read-side WAL shared-memory
coordination; the cooperative lease file may still be created.

Validation: formatting, all-target check, Clippy with warnings denied and all
376 Rust tests (353 library, 23 integration) passed with flatc 25.12.19.
Actual socket/HTTP integration ran outside network-restricted sandboxing.
Full test log: `/tmp/cogfabric-preview-readonly-tests.log`.

## Module inventory callback isolation (2026-10-13)

Status: **Implemented** for inference, arithmetic and SQL inventory observations.
`src/fabric.rs` and `src/fabric/tools.rs` now snapshot bounded retained handles
and activation flags before calling backend availability methods. Those callbacks
run after releasing the registry lock, preventing registry inspection inside an
inventory callback from deadlocking against lifecycle writes. Regression tests
in `src/fabric/tests.rs` and `src/fabric/tools.rs` assert write-lock availability
and inspect version metadata from actual observation callbacks. No model or tool
execution occurs. Snapshot references temporarily keep observed versions alive.

Membership and activation flags are sampled together; availability is observed
afterwards and can change independently. This is not an atomic remote-health
report, a provider probe or permission grant. Backend code remains trusted and
can block or panic. Other routing/activation callbacks have their existing
contracts; this change does not establish general callback reentrancy or
independent executable provisioning/isolation.

Validation: formatting, all-target check, Clippy with warnings denied and
378 Rust tests (355 library, 23 integration) passed with flatc 25.12.19.
Full socket/HTTP tests ran outside network restrictions; log:
`/tmp/cogfabric-inventory-observation-all-tests.log`. Inventory snapshots
clone at most 128 retained handles; no additional model or tool calls occur.

## Modular OpenAI-compatible agent serving (2026-10-13)

Status: **Implemented** for authenticated loopback Chat Completions, native
text/function turns and the tested OpenCode v2 workflow; broader API compatibility
and native conversation worker/registered-model support remain **Partial**.

The `cogfabric` library remains transport independent. `src/agent/` adds a typed,
replaceable native conversation backend and bounded FAST solver-turn lifecycle,
using the existing security kernel, global SQLite admission and execution leases.
Only `llm.generate` is granted. Tool calls are validated proposals returned to the
client, never executed or marked verified. Native audit records exclude raw
conversation, tool argument and answer bodies. Existing collective modes remain
available through separate aliases without accepting client tool definitions.

`crates/cogfabric-server/` adds HTTP/SSE mapping, dedicated bearer authentication,
loopback binding, browser-origin refusal, bounded connections/request bodies/turns,
timeouts and shutdown/disconnect cancellation. Its thin worker entry points reuse
core FlatBuffers workers, including actual math/SQL subprocess reaping tests.
No internal IPC schema or canonical French architecture was replaced.

Regression coverage: `src/agent/tests.rs`, server listener unit tests and
`crates/cogfabric-server/tests/http.rs` cover protocol correlation, unknown tools,
budget denial, durable cancellation/deadlines, authenticated two-turn tool/SSE
exchange, malformed/oversized requests, slow-body admission, connection capacity
and expiry, eight-turn saturation, streaming/non-streaming disconnects, collective
verification and server-image worker relaunch. CI and default workspace checks
now include the server crate; root `cargo run` retains the CLI default.

Formatting, workspace all-target check, Clippy with warnings denied, and all
391 Rust tests passed (360 library including the server listener; 31 integration).
Test log: `/tmp/cogfabric-agent-final-tests.log`. `docs/HTTP_API.md` documents
setup, limits, profile differences and unsupported capabilities.

Actual OpenCode v2.0.26 + LM Studio `qwen/qwen3-8b` passed read/edit/read on a
disposable fixture, with all other client actions denied. The first attempt
identified OpenCode's `store:false` field, now supported and regression-covered.
The final `scripts/opencode-smoke.sh` passed directly against the server: evidence
`/tmp/cogfabric-opencode-smoke.fuLeiy`, runner log
`/tmp/cogfabric-agent-opencode-smoke.log`. Five native requests (including client
metadata/title work) reported 4,893 input and 103 output tokens, three tool
proposals, 7,165.843 ms summed turn duration. All five persisted unverified;
server tool dispatch count was zero. OpenCode performed the three approved
client actions and the actual file contained `Status: after`. This is integration
evidence, not a general coding-quality or performance benchmark.

Debug server SHA-256:
`51425d19c325aec5602d5475f38d42b4d67a97cabb3c98ad48a2d9d828311fc0`.
OpenCode JSONL SHA-256:
`3bdc2b3136128e68d29748739774b0217f0145e2cd2ecf23b65bc7c6d61a8edc`.
SSE currently frames buffered upstream completions. Responses API, multimodal
input, remote/multi-tenant serving, native conversation worker RPC and full
collective reasoning over client tool loops remain unsupported. Existing
managed requirements are rejected rather than silently bypassed. Full v1.0
release gates and independent quality/security evaluations remain open.

Existing root CLI self-test and full installed-style CLI/socket smoke also
passed after the workspace split (evidence `/tmp/cogfabric-installed-smoke.IQKfcx`,
logs `/tmp/cogfabric-agent-existing-cli-smoke.log` and
`/tmp/cogfabric-agent-existing-socket-smoke.log`). Final protocol review additionally
rejects provider reuse of historical tool-call IDs; the regression and full
391-test workspace suite pass. Updated final live evidence follows below.

Final historical-call-ID fix also passed the complete scripted real-client run:
`/tmp/cogfabric-agent-opencode-final-smoke.log`, evidence
`/tmp/cogfabric-opencode-smoke.mCoC2U`. Five requests reported 4,891 input
and 98 output tokens, three client tool proposals and 7,213.491 ms summed turn
duration; all five remained unverified and no server tool dispatch occurred.
Final debug server SHA-256:
`a74c5a13e1954899944028769cc09cb4ee2ac2fd3d30eef2888422146900d890`.
Final OpenCode JSONL SHA-256:
`791c5b92a37b5258f554b073f2b8ab835cded56cbe54280c9640ac04106f285f`.
Earlier hashes/measurements in this entry refer to the preceding verified revision.
Client/model variations and startup work prevent treating these timings as a
controlled performance benchmark. Optimized/clean-OS installation of the new
server remains separate release validation work.

## Native conversation model-registration pinning (2026-10-13)

Status: **Implemented**. `AgentRuntime::local` now supports enabled immutable
model registrations through the existing selected model configuration. Startup
and each admission validate endpoint, model, role-profile absence and output
ceiling against the exact stored version. `run_until` snapshots registration
metadata before creating the task and persists the same `model_registration`
event contract as Engine, with weight identity explicitly unverified. Admitted
turns retain that snapshot if the operator later disables the model; subsequent
admissions reject before creating another task or calling inference.

The actual HTTP regression in `src/agent/tests.rs` confirms the registered model
and allocation in the provider request, pauses the response, disables the
registration, then verifies the original completed-turn metadata and rejection
of later turns. Mismatched configured model and excessive output ceilings are
also denied. This is metadata consistency and lifecycle evidence, not weight
attestation, automatic loading/unloading or exact context-token enforcement.
Native managed-worker RPC and role overrides remain unsupported. This
supersedes the earlier blanket native-registration limitation only.

Formatting, workspace all-target check, Clippy with warnings denied, and all
392 Rust tests (361 library, 31 integration) pass with flatc 25.12.19. Log:
`/tmp/cogfabric-native-registration-all-tests.log`. HTTP_API.md and THREAT_MODEL.md
now describe the supported selection boundary. No additional model calls or
permissions are added per turn; one bounded registration read/snapshot is added.

Hosted CI for the original modular agent API commit `f6ef304` completed
successfully: [38366331807](https://github.com/DrapNard/cognitive-fabric/actions/runs/38366331807).
This confirms that revision's workspace checks and existing CLI/socket workflow;
it does not establish new server optimized installation or general agent quality.

## Typed native conversation worker packets (2026-10-13)

Status: **Implemented** codec, **Partial** managed native execution.
`protocol/agent.fbs`, `src/agent/protocol.rs` and build integration add a dedicated
verified FlatBuffers native conversation format. This preserves history, tool
proposals and results without JSON envelopes or flattening into existing role
prompts. It is the prerequisite for the next managed-worker transport slice.
Existing schemas and native worker admission behavior remain unchanged.

The codec bounds physical/verifier/copy resources, validates fixed identity and
UUID fields, exclusive payloads, typed enums, allocations and history, and
preserves null text/unknown usage for tool-only completions. Four regression tests
in `src/agent/protocol/tests.rs` cover all choice modes and conversation roles,
malformed packets and identities, invalid enums/allocations, duplicate and
malformed output calls, and borrowed shared-string amplification. The API only
returns data; it grants no permissions, checks no live peer/deadline and launches
no worker. HTTP native managed mode remains explicitly disabled until those
transport/lifecycle checks exist. PROTOCOL.md records the integration contract.

Formatting, workspace all-target check, Clippy with warnings denied and all
396 Rust tests (365 library, 31 integration) passed using exact flatc 25.12.19.
Full log: `/tmp/cogfabric-native-protocol-all-tests.log`. No inference or new
permissions were added; the previously verified OpenCode workflow is unchanged.
Next milestone: authenticated one-task Unix exchange with trusted bootstrap
settings, bounded cancellation and audited owned-child reaping.

Hosted native registration CI [38367824575](https://github.com/DrapNard/cognitive-fabric/actions/runs/38367824575)
completed successfully for `4cf64ed`. Final codec formatting passes after
removing a test-file leading blank line; all workspace checks and 396 tests pass.

## Managed native conversation inference (2026-10-13)

Status: **Implemented** for same-image owner-local native workers, including
registered model configurations. `src/agent/unix.rs` adds UID/PID-authenticated
one-task/one-call RPC over the native FlatBuffers codec. Trusted non-deserializable
grants enforce task, input/output/proposal budgets and lifetime; replies match
original correlation/task/expiry and are validated against the original tools.
Connections are consumed on failure/cancellation rather than reused. A partial
frame has a ten-second timeout after its first byte; generation may use its full
granted deadline. Invalid peers/tasks, audit failures and pipelined data prevent
backend dispatch. Native backend construction is now lazy under cancellation,
so an already-cancelled turn invokes no backend or worker.

`src/agent/process.rs` owns the same-running-image child, private journaled socket,
execution lease and native process-capacity slot through reaping. Eight children
per host process may remain owned, including background cleanup after an entire
runtime future drops. Normal completion/cancellation waits for cleanup; failed
cleanup never produces a successful result or a false reap claim. Without a live
runtime/after host death, cleanup remains best effort and recovery preserves
unknown outcomes. These workers are coordination, not a hostile-code sandbox.

Trusted bootstrap appends `max_tool_calls` with legacy default four, tested absent
from default-valued frames and round-tripped at 0/1/4/64. `flatc --conform` against
the previous committed schema passes. Existing role-worker fields/versions and
permissions remain unchanged. Both supplied binaries delegate `agent-worker` to
the core implementation. Only explicitly scoped upstream inference credentials
are forwarded into otherwise cleared environments; server API keys are excluded.

Unit and actual-process regressions cover one-use proposals, forged identities
and outputs, unauthorized tasks/ceilings, audit-before-dispatch, partial frames,
provider-future cancellation, eight-worker capacity, live-lease recovery refusal,
registered native model selection, independent upstream/server credentials,
normal shutdown, streaming disconnect, whole-future non-stream disconnect and
ownership-audit failure with confirmed child reaping. No client tool executes
inside the worker or server. Files: `src/agent/`, `protocol/bootstrap.fbs`,
`src/fabric/unix/bootstrap.rs`, shared socket helper, binary entry points and
`crates/cogfabric-server/tests/native_worker.rs`.

Mandatory formatting, workspace all-target check, Clippy with warnings denied and
all **414 Rust tests** (376 library, 38 integration) passed with flatc 25.12.19.
Full log: `/tmp/cogfabric-native-managed-all-tests.log`. Existing CLI self-test
and complete installed-style typed socket smoke pass; socket evidence
`/tmp/cogfabric-installed-smoke.wLpdmV`, logs
`/tmp/cogfabric-native-managed-cli-smoke.log` and
`/tmp/cogfabric-native-managed-socket-smoke.log`.

Actual OpenCode v2.0.26 + LM Studio `qwen/qwen3-8b` passed the final managed
read/edit/read smoke with only that disposable file allowed. Evidence:
`/tmp/cogfabric-opencode-smoke.TnXEVJ`; runner log
`/tmp/cogfabric-native-managed-final-opencode.log`. The script now asserts each
admitted turn launched and reaped a worker. Five requests (including client
metadata work) reported 4,901 input/103 output tokens, three tool proposals and
7,549.771 ms summed turn duration. All five tasks remained unverified; all five
children were reaped with no forced kills and exit code zero; server tool dispatch
count was zero. Actual fixture contents matched `Status: after`. These are
integration observations, not controlled quality or latency benchmark claims.

Final debug server SHA-256:
`a6371603c478bd6b23f6dc50e3bba129fe4ad6e52cda6d7a1b09f4e803181fde`.
Final OpenCode JSONL SHA-256:
`3eaad52ab1a5f54ad37067abb29ef887575c1074a77d596f1563f632e188306f`.
Updated smoke script SHA-256:
`bc4fe9a14427c9f10138ec82b5a33fa837213d0a75b291da9e6a4654743f1485`.

This supersedes earlier native-worker-disabled statements. HTTP_API.md,
PROTOCOL.md, WORKERS.md and THREAT_MODEL.md describe the supported boundary.
Native role overrides, arbitrary executable provisioning/attestation, stronger
isolation, independent agentic quality/security evaluation and optimized/clean-OS
installation of the expanded server remain incomplete. External GPU completion
is not established by local cancellation or reaping. Full v1 readiness is unproven.

Shutdown additionally drains native ownership for up to ten seconds, including
cleanup scheduled after a request future already disappeared. A cleanup/audit
error disables new native worker admissions for that host lifetime and makes
shutdown fail visibly. Actual-process regressions cover immediate shutdown after
non-streaming disconnect and injected stop-audit failure with no further child
launch; provider disconnect is observed asynchronously rather than inferred
from the reaping timestamp alone.

Managed frame encoding is also validated before task/child admission, so a
request within a raised HTTP/input ceiling but beyond the 256 KiB native frame
limit is rejected without launching a worker. The regression verifies no task
row or child intent is created. Final mandatory workspace checks and 414 tests
(376 library, 38 integration) pass.

Hosted codec CI [38369517206](https://github.com/DrapNard/cognitive-fabric/actions/runs/38369517206)
completed successfully for `e04fcd6`. The final managed-worker revision has local
414-test and live-client verification above; its hosted run follows private sync.

Update 2026-10-13: managed-worker source `d807e9e` also passed hosted CI
[38377800359](https://github.com/DrapNard/cognitive-fabric/actions/runs/38377800359),
locked offline optimized workspace binary builds, the core CLI/socket smoke and
the managed OpenCode/Qwen read/edit/read smoke using the optimized server.
RELEASE_AUDIT.md records binary hashes and fresh evidence directories. Optimized
workflow verification is implemented and tested; new-server clean-OS/package
installation and independent agentic-quality evaluation remain incomplete.

### Collective worker ownership on dropped futures

**Implemented:** `src/fabric/worker_cleanup.rs`, collective inference/tool process
Drop implementations and HTTP shutdown integration retain sockets and execution
leases until child reaping. Shared ownership also includes expert inference
processes. Failed cleanup/audit blocks subsequent collective process owners and
fails drain; startup ownership remains retained after explicit cleanup errors.
**Tests:** dropped startup subprocess owners for inference/tools;
`crates/cogfabric-server/tests/collective_cleanup.rs` exercises real managed
inference on HTTP disconnect, immediate shutdown and stop-audit failure with
blocked readmission. No new external tools or untrusted plugin sandbox. General
provisioning, residency scheduling and independent reasoning evaluation remain
incomplete. WORKERS.md records bounded waits and forced-host-termination limits.

Final verification: formatting, offline workspace all-target compilation and
Clippy with warnings denied pass. All 417 Rust tests pass (378 library, 39
integration). Full log: `/tmp/cogfabric-collective-cleanup-tests.log`. The first
sandboxed socket test attempt was denied by local sandbox policy; the complete
socket/subprocess suite passed with authorized local execution. No new live
model study or optimized installation was run for this cleanup revision.

### Installed HTTP interoperability verification

**Implemented and verified:** separate optimized core/server package installation
from runtime source `38948b1`, installed CLI/socket smoke, new offline installed
HTTP smoke and installed managed OpenCode/Qwen read/edit/read. HTTP_API.md records
installation commands; INSTALLATION_VALIDATION.md extends the reproducible clean
Arch workflow to both packages (that updated clean-OS workflow is not yet run).
The offline HTTP smoke now runs in CI; this revision's hosted result remains
pending. RELEASE_AUDIT.md records exact runtime source, binary/script hashes and
evidence. Rust runtime/tests are unchanged: the previous 417-test quality gate
and hosted runtime CI pass. Independent expert quality and general provisioning
remain partial.

### Clean-OS installation refresh

**Implemented and verified:** source `a9933b3` passes fresh Arch prerequisite
setup/dependency download, pinned FlatBuffers, all mandatory workspace checks and
417 tests, separate optimized core/server installs, and both installed offline
workflow smokes. Hosted CI `38381768218` also passed the new HTTP smoke step.
INSTALLATION_VALIDATION.md gives reproduction; RELEASE_AUDIT.md records immutable
image/source references, binary/log hashes and limits. This supersedes the prior
new-server clean-OS and pending HTTP CI gaps. Native GPU/model setup in Arch,
CachyOS-specific desktop setup, independent expert quality and general module
provisioning remain unverified or partial. No runtime changes or new test count.

### Read-only residency operation inspection

**Implemented:** `residency::inspect_operation` and CLI `residency-inspect` compare
one bounded recorded operator intent against a fresh inventory GET. Early
read-only startup avoids migrations/bootstrap. Exact endpoint matching, strict
intent/action parsing, original bounds and duplicate detection precede HTTP.
**Tests:** `tests/residency.rs` covers load/unload present/absent observations,
one GET without retry/POST, unchanged tasks/config/skill registry, and malformed,
oversized, duplicate, missing or wrong-endpoint intent before network.
**Partial:** observing desired state proves neither operation causality nor
instance ownership. Tasks are never finalized or granted cleanup authority;
automatic scheduling and live authenticated mutation validation remain open.

Final inspection checks: formatting, workspace all-target check, Clippy with
warnings denied and 419 Rust tests pass (378 library, 41 integration). Test log:
`/tmp/cogfabric-residency-inspection-tests.log`. No live residency mutation or
source-install refresh is claimed for the new command.

Inspection credential regression: **Implemented and tested** in `tests/residency.rs`.
Separate fixture residency/inference tokens verify scoped forwarding and encoded
provider-echo rejection without task changes or response metadata disclosure.
Current workspace gates pass with 420 tests (378 library, 42 integration).
This proves the local CLI credential path, not real server ownership/authentication.
MODELS.md records why documented native REST instance IDs do not supply the
coordination contract needed for safe automatic unloading.

Hosted runtime CI [38384114873](https://github.com/DrapNard/cognitive-fabric/actions/runs/38384114873)
passed for `d4b9cf5` (419 tests). The added credential regression is verified
locally with the 420-test gate above; its own hosted run follows private sync.

### External compiler oracle for public Rust diagnostics

**Implemented:** 20 balanced ownership/borrowing cases and a versioned hash/label
manifest under `evaluation/rust_ownership_*`; fixture regression compiles fixed
trusted snippets as Rust 2021 metadata and checks specific negative diagnostics.
Compiler subprocess waits and cleanup are bounded; snippets/model output never
execute. This adds an oracle outside the model and built-in framework checkers,
without adding runtime permissions. **Partial:** public authoring is not unseen
or independently reviewed data, and this test establishes no live-model result.

Compiler corpus gate: all 421 workspace tests, formatting, all-target check and
Clippy pass. Runtime library count stays 378; integration count is now 43.
