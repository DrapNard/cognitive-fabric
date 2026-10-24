# v1.0 completion audit

Initial audit: 2026-10-23; evidence updated: 2026-10-24. **Completion is not proven. No release approval is implied.**

This audit applies the full French `ARCHITECTURE.md`, the owner's engineering
instructions, and the request to implement all remaining Jev+/Jev-R code. It does
not replace the architecture. Historical entries in `IMPLEMENTATION_MATRIX.md`
and `CURRENT_IMPLEMENTATION.md` describe earlier snapshots and can be superseded.



## Token preparation requires a live task — 2026-10-24

**Implemented.** Architecture sections 7, 9 and 10: `src/token_admission.rs`
checks the task status through a bounded read-only database connection before
rendering or tokenization can expose a prompt to the provider. Missing and
terminal identities fail without native HTTP requests. The independent immediate
write transaction still rechecks status before reserving tokens and dispatching
generation; a task stopped during preparation cannot obtain that reservation.
Two new regressions in `tests/input_token_admission.rs` cover both boundaries.
This adds one local database read per admitted attempt, grants no permissions and
makes no claim that cancellation stops already-started remote preparation.
The WAL hard-size bound remains an open storage requirement.


## Exact wire-input token admission — 2026-10-24

**Implemented for the optional llama.cpp native adapter; general provider support
remains Partial.** `src/token_admission.rs` renders complete text history, counts
and submits identical token IDs, and durably charges a cumulative task allowance
before generation. Context admission includes the reserved output. Concurrent
reservations, malformed ledger history and audit failures fail closed; failures
never refund input. `Config.input_token_admission`, the human-only CLI setting,
typed bootstrap v3, backend capability gates and managed credential forwarding
preserve the limits across supported dispatch paths. Default behavior is unchanged.

Five loopback/managed-worker integration tests pass. They cover exact IDs, context
refusal, provider failure without refund, concurrent admission, audit-write failure,
chat history, early unsupported requests and dedicated credential separation.
See [input token admission](INPUT_TOKEN_ADMISSION.md) for commands and restrictions.
Tools, role profiles, non-none provider effort and native LM Studio lifecycle are
unsupported in this mode. Real llama.cpp inference, physical GPU/energy enforcement
and reasoning quality remain unverified. No reflection campaign was run.


## Kernel bounds for managed workers — 2026-10-24

**Implemented; focused resource-boundary verification passed.** Architecture
sections 4, 9 and 10 now have optional Linux per-process virtual address-space and
CPU-time limits for managed native chat, inference, arithmetic, SQL and compiled
typed workers. `src/process_limits.rs` restricts only the child before exec,
preserves stricter inherited limits, disables child core dumps and refuses setup
failure. `Config.worker_process_limits` defaults to None; the human-only
`configure-worker-limits` command validates and saves it. Existing services require
restart, and existing tasks preserve their admitted settings.

Typed module identities and reconstruction retain exact original limits. New
launch/ownership audits report requested bounds without claiming physical usage
measurement. See [worker limits](WORKER_LIMITS.md). This does not constrain an
external model provider, GPU, total task CPU, RSS, filesystem or network, and is
not a full sandbox. The logical scheduler's allocation declarations remain distinct.

Four subprocess regressions pass (one ignored fixture is explicitly invoked by
them), along with two configuration/permission tests, one original-limit recovery
test, seven bounded native-worker tests and the executable-upgrade worker test.
A temporary CLI workflow verifies typed arithmetic under limits and clearing the
setting. Default and all-feature workspace all-target checks and strict Clippy pass.
The NVML feature check also exposed and fixed an unnecessary return. No reflection, live reasoning-quality or
performance benchmark was run for this milestone.

## OpenCode compatibility follow-up — 2026-10-24

Focused integration verification resumed at the operator's explicit request to
fix OpenCode. OpenCode v2.0.32 completed a local-Qwen read/edit/read smoke and a
deterministic real-client regression. Ten HTTP regressions and strict server
Clippy pass. After operator-approved Qwen reload from 4096 to 32768 context, a
normal project session executed a real read tool and returned the correct package
name; the startup helper exited cleanly and reaped its own server. This updates
the earlier untested OpenCode startup status only. It does not prove broad coding
quality, physical quotas, isolation or full release readiness.

## Installation and chat additions — 2026-10-24

A source installer with curl instructions and a separate bounded, tool-free
`cogfabric-chat` client are implemented. Shell syntax validation passes; an actual
source installation, live chat and concurrent OpenCode session have not been run
for these changes. Rust 1.99 default/all-feature production workspace compilation
and formatting pass. These conveniences do not close the reasoning, isolation or
physical resource evidence gaps below. No release approval is implied.

## Current GPU implementation — 2026-10-24

Optional NVML telemetry and explicit provider-confirmed KV-cache placement are
implemented in the current source. Protocol generation now includes an eleventh
schema, `protocol/gpu.fbs`, with the pinned 25.12.19 generator. Production-only
all-feature workspace compilation passed after integration fixes; it does not
establish runtime correctness. New tests, live GPU sampling and benchmarks are
deferred at the operator's request. The historical suites below precede these
changes. Physical GPU quotas, task attribution and model quality remain unproven.

## Explicit project tracking — 2026-10-24

Architecture §3 now has explicit immutable project profiles: operator-published
goals and typed presentation preferences selected by exact version for each task.
The optional FlatBuffers selector, SQLite schema-9 registry, CLI/socket path,
pre-inference snapshots and exact snapshot continuation are implemented in the
current source. Final serialized suites pass **1,048 default tests** and **1,057
all-feature tests**, each across 72 targets, zero failed or ignored. Formatting,
Rust 1.99 workspace check and strict default/all-feature Clippy pass. CLI/socket
profile workflows are covered by the completed tests. No further test or installed
smoke run was started after the operator requested implementation work.
No inferred preference learning or live-model style-adherence result is claimed.
See [project profiles](PROJECT_PROFILES.md).

## Exact Wasm registry recovery — 2026-10-24

Explicit operator manifests now reconstruct an unfinished nonnative task's exact
v4 guest registry after restart, preserving broker UUID, capabilities, artifact,
settings/host-source hashes and queue/concurrency limits. Persisted metadata
never supplies an executable path. Existing dispatch-history gates still refuse
uncertain calls. Native/mixed registries and snapshots lacking original limits
are not reconstructed by this option. See [Wasm experts](WASM_EXPERTS.md).

Frozen-source serialized runs pass **1,032 default tests** and **1,041 all-feature
tests**, each across 71 targets with zero failed or ignored. Rust 1.99 workspace
check and strict default/all-feature Clippy, formatting, diff checks and final
offline installed CLI/HTTP smoke checks pass. Earlier editing/concurrent-build
verification artifacts were resolved by these full reruns. No live-model,
physical-resource, neural-safety or general native sandbox result is implied.
The local model endpoint probe still refused connection.

## Optional Wasm expert integration — 2026-10-24

`src/fabric/modules/wasm.rs` now supplies bounded no-import untrusted guest
execution under host task grants; see [ABI and limits](WASM_EXPERTS.md).
The all-feature workspace suite passes **1,039 tests across 71 targets**.
Rust 1.99 workspace check, strict default/all-feature Clippy, formatting, pinned
FlatBuffers generation and offline CLI/HTTP smoke checks pass. The repeated
default suite passes **1,031 tests across 71 targets**, zero failed or ignored.
These results narrow the
expert execution gap but do not prove generic native isolation, physical resource
attribution, live reasoning quality, or the research objectives below.

## Evidence rules

“Implemented” below means the named runtime mechanism exists with relevant
automated coverage. It does not mean the whole subsystem's scientific objective
is achieved. “Partial” means an explicit requirement still lacks implementation,
independent evidence, or a live measurement. Mock/loopback tests establish local
mechanics, not general reasoning, model quality, physical isolation or efficiency.
Research requirements remain requirements of the vision even when the engineering
instructions explicitly place them beyond proven v1.0 functionality.

## Seven subsystems and named roles

| Requirement | Current evidence inspected | Assessment and remaining obligation |
| --- | --- | --- |
| Language: original goals, constraints, ambiguity and clarification (§3) | `src/language.rs`, `src/engine/interpretation_tests.rs`, `src/reasoning/interpretation.rs`; bounded structured parsing, original-goal retention and clarification tests | Implemented mechanisms; natural-language accuracy remains unmeasured on independent live tasks. |
| Language: explanations, preferences and conversation context (§3) | `src/language.rs`, `src/engine.rs`, `src/config.rs`; explicit conversation references, synthesis option and role profiles; language and native service tests | Explicit immutable profile tracking is implemented in `src/projects.rs`, schema-9 registry and task/recovery snapshots with CLI/UDS coverage. Model adherence and explanation quality remain unmeasured; no inferred preference learning is claimed. |
| Jev+ Router (§4) | `src/routing.rs`, `src/routing/{workflow,empirical}.rs`, `src/capabilities/cover.rs`; `tests/{capability_cover,workflow_router,resource_route_selection}.rs` | Implemented bounded replaceable routing, version filters, declared coverage and fallback; calibrated usefulness remains unproven. |
| Jev+ Scheduler (§4, §9) | `src/resource_scheduler/`, `src/scheduling.rs`; `tests/{configured_resources,resource_scheduling,working_profiles,critical_path_scheduling}.rs` | Implemented logical resource/queue/dependency admission; physical GPU/CPU/memory enforcement and attribution are not proven. |
| Jev+ Evaluator (§4) | `src/verification.rs`, `src/evaluation.rs`, `src/reasoning/verification.rs`; criterion and candidate regression tests | Implemented independent compiled criteria for supported structured tasks. General factual/technical answer verification remains Partial. |
| Jev+ Escalator (§4) | `src/escalation.rs`, `src/escalation/empirical.rs`, `src/reasoning/advice_guard.rs`; retry-study/advice-guard tests | Implemented bounded budget and reviewed empirical policies. Independent live calibration and cost/utility evidence remain Partial. |
| Jev+ Dependency Manager (§4) | `src/planning.rs`, `src/scheduling.rs`, `src/registry.rs`; graph, critical-path and activation tests | Implemented bounded DAG ordering and versioned dependency validation; advice remains unverified. |
| Jev+ Capability Manager (§4) | `src/capabilities.rs`, `src/gaps.rs`, `src/registry.rs`; capability cover/version/gap tests | Implemented declaration discovery, compatibility filtering, gap tracking and bounded cover search. Declarations do not attest executable health or ability. |
| Jev+ desired uncertainty/order robustness (§4) | `src/routing/empirical/`, `src/capabilities/tests.rs`, `tests/{strategy_portfolio,advice_guard}.rs` | Partial: deterministic policies and bounded statistical evidence exist; general probabilistic robustness is not demonstrated. |
| Jev-R Planner and Solver (§5) | `src/reasoning.rs`, `src/planning.rs`, `tests/{parallel_subtasks,parallel_subtask_resume}.rs` | Implemented bounded planning, advisory DAG execution and final solving; general problem-solving gains remain unproven. |
| Jev-R Explorer (§5) | `src/reasoning/exploration.rs`, `tests/strategy_portfolio.rs` | Implemented bounded alternative portfolios and strict parsing; semantic diversity/utility is not established by shape checks. |
| Jev-R Critic and Reflector (§5) | `src/reasoning.rs`; role-script, revision and candidate convergence tests | Implemented advisory critique/revision with retained charges and stopping rules. Critic assertions cannot establish success. |
| Jev-R Verifier (§5) | `src/verification/{path,finite,provider,wasm}.rs`, `tests/{verifier_provider,typed_module_reasoning,wasm_verifier}.rs` | Implemented supported compiled criteria, optional trusted callback and bounded secondary Wasm checks; arbitrary untrusted native verifiers and general factual proof remain unsupported. |
| Jev-R recursion and evidence-aware stopping (§5, §9) | `src/reasoning/{convergence,progress,finite_search}.rs`; convergence, progress, pruning and recovery tests | Implemented hard ceilings and structured stopping/search. Heuristic stopping does not prove unsatisfiability or general reasoning quality. |
| Expert Network (§6) | `src/registry.rs`, `src/registry/providers.rs`, `src/models.rs`, `src/fabric/modules.rs`; activation, provider, model and typed worker tests | Implemented immutable versions, hot-swap pins, dependency graphs and reviewed provider adapters. Arbitrary executable loading/isolation and independently measured usefulness of all proposed domain experts remain Partial. |
| Sparse native lifecycle and shared backbones (§6, §9, §12) | `src/residency/`, `src/backend/native.rs`, `tests/{managed_residency,native_lifecycle,resource_route_resume}.rs` | Implemented approved task model sets, receipt reuse/release, exact-target checks and owned continuation. Actual provider weight sharing, adapter switching, load costs and physical continuity remain unverified. |
| Cognitive Fabric typed messaging (§7, §12) | Eleven `protocol/*.fbs` schemas, generated bundle, `src/fabric/`, `src/events/`, `src/wire.rs`; protocol corruption/socket/process tests | Implemented supported local FlatBuffers IPC and typed workers. Independent arbitrary actor provisioning, remote transport and general executable attestation are not implemented. |
| Persistent task/episodic/semantic/skill/model/evaluation stores (§7) | `src/{store,memory,models,evaluation,storage,restore}.rs`, schema-9 migration and rollback tests | Implemented SQLite-backed logical stores, FTS retrieval, explicit publication and migrations. General external/neural artifact portability and physical filesystem/WAL bounds remain Partial. |
| Crash continuation and unknown outcomes (§7, §10) | `src/recovery/`, native restoration and typed reconstruction; resume/parallel/receipt/refusal tests | Implemented supported original checkpoint forms with retained budgets, exact callbacks or fixed source-bound workers. Unknown calls are refused, not replayed; arbitrary crash-phase resumption remains unsupported. |
| Evolution A: reusable memory (§8) | `src/memory.rs`, `src/gaps.rs`; publication/revocation, bounded retrieval and provenance tests | Implemented explicit verified memory publication and gap evidence. Independent transfer of learned strategies remains unproven. |
| Evolution B: RAG candidates (§8) | `src/evolution.rs`, `src/evaluation.rs`, `src/rag_archive.rs`; quarantine, deduplication, held-out, promotion and rollback tests | Implemented operator-selected candidate pipeline. Reviewed local source normalization, deduplication and split preparation now exist in `src/evolution/preparation.rs` with `tests/source_preparation.rs`. Bounded local inventory drafts now exist in `src/evolution/discovery.rs` with `tests/source_discovery.rs`, and `src/evolution/prepared.rs` atomically binds validated preparation provenance to inactive candidates with `tests/prepared_candidate.rs`; remote discovery and independently established dataset quality/poisoning resistance remain Partial. |
| Evolution C: offline LoRA (§8) | `training/train_lora.py`, inactive candidate import, deterministic neural-promotion denial | Experimental, as allowed by owner instructions. Actual isolated training, base-model provenance, independent neural evaluation and validated adapter promotion are not proven. |
| Evolution D/new trained Jev networks (§5, §8, §14) | No trained Jev network or self-rebuilding active weights are claimed | Not implemented: explicit future neural research. RAG tests must not be used to promote neural weights. |
| Independent Security Kernel (§10) | `src/security.rs`, task grants, protocol validation, worker cleanup, candidate gates, `src/security_evaluation/corpus.rs`; denial/tampering/cancellation tests | Implemented deny-default supported capabilities and local boundaries. Same-image processes and trusted Rust callbacks are not a hardened untrusted-code sandbox. |

## Technical, measurement and release obligations

| Requirement | Evidence / status |
| --- | --- |
| Rust 2021 + Tokio own orchestration; Python offline only (§12) | `Cargo.toml`, `src/`, `training/`; implemented runtime split. No edition migration or Python runtime substitution. |
| Exact FlatBuffers generator/runtime pairing | `build.rs`, `build_support/protocol.rs`, `protocol/generated/manifest.sha256`, `tests/protocol_bundle.rs`; fresh `flatc 25.12.19` output matches all eleven generated files. |
| Safe UDS permissions, bounded messages, validation and cancellation | `src/fabric/unix*`, `src/service.rs`, `src/agent/unix*`; socket spoofing, ownership, malformed frame, cancellation and reaping tests. Local same-user threat remains. |
| FAST/STANDARD/DEEP/RESEARCH ceilings (§9) | `src/config.rs`, `src/reasoning.rs`, resource admission and policy tests enforce supported calls/cycles/output/prompt/tool/time/queue limits. Exact input-token preflight is implemented for the opt-in llama.cpp adapter; general provider support and physical energy/GPU ceilings remain Partial. |
| Comparable baseline arms (§13) | `src/benchmarking.rs`, `tests/{benchmark_cli,progress_ablation,progress_ablation_cli}.rs`; six same-model profiles and optional larger-model baseline exist. Equal-budget independent live results are not established. |
| 100–200 reproducible technical/reasoning tasks (§13) | `src/benchmarking.rs::authored_v1_suite_has_bounded_unique_cases_and_checked_tool_labels` validates 100 authored cases. Independent review, unseen domain breadth, ambiguity/tool necessity and distribution robustness remain Partial. |
| Verified success, latency, calls, regressions (§13) | Supported criterion scoring, timing/resource totals, hashes and RAG regression gates exist. These do not demonstrate a statistically meaningful collective gain. |
| Joules per solved task, peak VRAM, total physical compute (§13) | `src/gpu.rs` and opt-in `src/gpu/window.rs` observe whole-device metrics and partial benchmark-window power intervals; `src/telemetry.rs` observes process memory. Task-attributed energy/peak VRAM/compute accounting is not implemented/proven. Missing values must remain unknown. |
| Independent hostile-input and neural safety evaluation (§10, §13) | Fixed compiled security corpus and negative tests exist; independent broad behavioral/neural evaluator remains absent. |
| No implicit permissions, live-weight mutation or self-promotion (§15.1–4) | Replaceable adapters, immutable task grants, inactive candidates and neural-promotion denial provide supported local mechanisms; no guarantee for forks or hostile owners. |
| Cost justification and independent success (§15.5–6) | Budget/empirical gates and supported compiled verification exist; universal cost benefit or external truth is not proven. |
| Untrusted content separated; versioned schema semantics (§15.7–8) | Quoted references, strict parsing, role/task audits and migrations have negative coverage; arbitrary untrusted native code remains outside the trusted host API. |
| Evaluators/thresholds and publication remain operator-controlled (§15.9–10) | Immutable reviewed evaluation records, explicit approvals, no release/visibility change performed. A Git push is not a public-release safety approval. |
| Roadmap V0.4/V0.8/V0.9 scientific success criteria (§14) | Equal-budget collaboration gain, non-regressing neural adaptation and measured collective learning remain unproven. Infrastructure tests cannot satisfy these criteria. |

## Current decision and next work

The previously pushed prepared-provenance snapshot passed these checks:

- `cargo fmt --all -- --check` and `git diff --check`.
- `cargo +1.99.0 check --locked --workspace --all-targets`.
- `cargo test --locked --workspace --all-targets --no-fail-fast`: **1,030 passed
  across 70 targets**, zero failed or ignored.
- Rust 1.99 strict workspace Clippy with default and all features.
- `cargo test --locked --features wasm-verifier --test wasm_verifier`: **4 passed**.
- Fresh exact `flatc 25.12.19` generation matches all ten reviewed binding files.
- Both installed CLI and authenticated HTTP smoke scripts pass against copied
  binaries and fresh temporary state. These exercise offline tools/transports,
  not live model inference.

The framework has substantial implemented Jev+ and Jev-R mechanisms. **“Every
part is complete” remains false.** Prior claims that only a real server remained
were too narrow: independent evaluation, task-attributed physical telemetry,
general verifier/expert isolation, automatic source preparation and the explicit
research milestones still have missing evidence or implementation.

Code correction completed from this audit: the native generation-history gate now
has an independent SQLite deadline as well as row/payload bounds. Regression tests
verify that expired inspection cannot become clearance or leak an interrupt hook.
Continue the remaining runtime/measurement requirements above without turning
optional neural research into a claim of trained models. Live experiments require
an available operator-configured model server; the scoped OpenCode follow-up demonstrated local Qwen inference and tool
conversation after an operator-approved context reload. No model download, training run, release tag or repository visibility
change is implied by this audit.
