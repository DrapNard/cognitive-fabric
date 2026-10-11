# Implementation matrix — 1.1.0-alpha.1

Direct and task-pinned inference broker dispatch both preserve caller task UUIDs.
Concurrent distinct-task and malformed-task regressions cover this boundary in
`src/fabric/tests.rs`; separate-process authentication remains incomplete.

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
| 7 | Persistent memory | Partial | `src/store.rs`: SQLite WAL/FTS5, schema migrations through 6, durable task/events, isolated explicit history, pinned registry/document snapshots, explicit lease-protected interrupted-task recovery, immutable versioned local model manifests and task selection snapshots, operator-published episodic snapshots, recomputed arithmetic memory, revocation and payload quotas, SQLite page ceilings, global task admission and reviewed retention; migration/isolation/tampering/deletion-rollback tests | Broader independent fact verifiers, backend model residency, physical WAL/filesystem bounds, full artifact/configuration restore and automatic checkpoint resumption |
| 8 A/B | RAG evolution | Partial | `src/evolution.rs`, `src/evaluation.rs`: content-addressed quarantine, hashes/provenance/deduplication/quotas, isolated held-out retrieval, bounded cumulative approved-version regression gates and immutable accepted suites, atomic paired review records, transactional human promotion/rollback; integrity, regression, concurrency and rollback tests | Stronger poisoning/security evaluation, automatic source discovery and cleaning; dataset independence remains operator-attested |
| 8 C | Offline neural adaptation | Experimental | `training/train_lora.py`, inactive artifact registration; neural evaluation/promotion denied | Base-model provenance and independent neural evaluation manifests, actual training and externally isolated pipeline; no hot-swap claim |
| 8 D, 14 | Novel neural reconstruction | Not implemented | Research only; no trained Jev networks or superiority claims | Separate research and independent evaluations, beyond proven v1.0 infrastructure |
| 9 | Resource enforcement | Partial | `src/config.rs`, `src/reasoning.rs`: FAST/STANDARD/DEEP/RESEARCH ceilings, calls/cycles/reserved outputs/prompt bytes/tools/time, cancellation; exhaustion tests; optional provider usage and measured inference duration | Exact input-token preflight, model-load cost and retry metrics, process-isolated inference/GPU memory and optional energy telemetry |
| 10, 11 | Security kernel | Partial | `src/security.rs`: immutable task/actor scope intersected with global policy, per-dispatch audits, deny-default capabilities, bounded no-follow reads; replay/actor/promotion denials and artifact tampering tests | Authenticated independent module identities and isolation before executable experts; signatures/attestations and neural behavioral evaluator absent |
| 13 | Reproducible evaluation | Partial | `src/evaluation.rs`, `src/benchmarking.rs`, `src/corpus.rs`: expected exact-match benchmarks, six isolated same-model ablation profiles plus optional larger baseline, per-arm resources, reviewed dataset hashes, bounded read-only corpus snapshots and durable run cancellation; profile/isolation/deadline tests | 100–200 diverse held-out tasks, robustness/security/regression reports, measured live-model comparisons |
| 12, 14 | CLI/service | Partial | `src/main.rs`: initialization/configuration, ask, service controls, task-specific socket progress/cancellation, skill lifecycle, ingestion/search, task traces, candidates/rollback, benchmark, self-test; real socket fixtures | Live residency lifecycle |
| 12, 13 | Hardware/performance claims | Partial | Optional whole-device GPU diagnostics sampled real host memory/utilization/power; no energy or large-model advantage claimed | Authenticated live inference, task-attributed telemetry and equal-budget comparisons |

All 164 current tests pass, including actual socket/loopback HTTP fixtures; formatting,
all-target build checks and Clippy with denied warnings pass. Fixtures do not prove
GPU integration, neural quality, immutable backend weights or live model switching.

The last live endpoint check found no server at `127.0.0.1:1234`. Live inference is
therefore unverified. It is not a blocker for implementing independent infrastructure,
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
smoke cover this path. This does not bundle external artifacts/configuration or
provide full automated workspace restoration.

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
Explicit protected operator-supplied API authentication is the next integration
gap; external operator model loading is not automatic framework residency control.

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
