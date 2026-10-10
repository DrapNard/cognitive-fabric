# Implementation matrix — 1.1.0-alpha.1

The complete French `ARCHITECTURE.md` remains canonical. This matrix describes
current evidence on `develop/v1.0`, not a reduced target architecture. The project
remains a pre-release. `QUALITY_REVIEW.md` preserves verification history.

| Architecture sections | Requirement | Status | Current implementation and tests | Remaining release work |
|---|---|---|---|---|
| 7, 12 | Typed local IPC | Partial | `protocol/*.fbs`, `src/fabric.rs`, `src/wire.rs`, `src/service.rs`: bounded verified IPC and module RPC, version-pinned inference registration/discovery, correlation/actor/destination/expiry checks, queues and cancellation; socket, spoofing, overflow, replacement and compatibility tests | General non-inference/event routing, subscriptions, separate-process actor authentication and broader compatibility fixtures |
| 3 | Language interface | Partial | `src/language.rs`, `src/backend.rs`, `src/config.rs`: strict advisory intent, clarification, original-goal preservation, explicit conversation references, optional synthesis, persistent role profiles; scripted and HTTP tests | Live local-model validation and measured interpretation quality |
| 4, 9 | Jev+ coordination | Partial | `src/engine.rs`, `src/planning.rs`, `src/store.rs::route`: capability routing/gaps, bounded topological subtask execution, admission budgets; dependency and budget tests | General module scheduling, measured escalation policy, automatic subtask resumption and queue telemetry |
| 5 | Jev-R reasoning | Partial | `src/reasoning.rs`: planner/solver/explorer/critic/reflector, strict recursive revisions, no-progress stopping, unverified subtask results and original-goal synthesis; order, revision and cancellation tests | General independent executable verifiers and live-model effectiveness measurements |
| 6 | Expert network | Partial | `src/registry.rs`: immutable semantic-versioned reference manifests, dependency/reverse-dependency checks, health, activate/disable/metadata rollback, bounded registry; lifecycle/tampering tests | Tool-backed, prompt-workflow and independent-model experts; backend loading/unloading; neural adapters remain experimental |
| 7 | Persistent memory | Partial | `src/store.rs`: SQLite WAL/FTS5, schema migrations through 4, durable task/events, isolated explicit history, pinned registry/document snapshots, explicit lease-protected interrupted-task recovery, operator-published episodic snapshots, recomputed arithmetic memory, revocation and payload quotas; migration/isolation/tampering tests | Broader independent fact verifiers, model registry, general storage quotas and automatic checkpoint resumption |
| 8 A/B | RAG evolution | Partial | `src/evolution.rs`, `src/evaluation.rs`: content-addressed quarantine, hashes/provenance/deduplication/quotas, isolated held-out retrieval, bounded cumulative approved-version regression gates and immutable accepted suites, atomic paired review records, transactional human promotion/rollback; integrity, regression, concurrency and rollback tests | Stronger poisoning/security evaluation, automated gap-to-candidate preparation; dataset independence remains operator-attested |
| 8 C | Offline neural adaptation | Experimental | `training/train_lora.py`, inactive artifact registration; neural evaluation/promotion denied | Validated training/dataset/evaluation manifests and an independently tested isolated offline pipeline; no hot-swap claim |
| 8 D, 14 | Novel neural reconstruction | Not implemented | Research only; no trained Jev networks or superiority claims | Separate research and independent evaluations, beyond proven v1.0 infrastructure |
| 9 | Resource enforcement | Partial | `src/config.rs`, `src/reasoning.rs`: FAST/STANDARD/DEEP/RESEARCH ceilings, calls/cycles/reserved outputs/prompt bytes/tools/time, cancellation; exhaustion tests; optional provider usage and measured inference duration | Exact input-token preflight, model-load cost/retry/queue metrics, process/GPU memory and optional energy telemetry |
| 10, 11 | Security kernel | Partial | `src/security.rs`: immutable task/actor scope intersected with global policy, per-dispatch audits, deny-default capabilities, bounded no-follow reads; replay/actor/promotion denials and artifact tampering tests | Authenticated independent module identities and isolation before executable experts; signatures/attestations and neural behavioral evaluator absent |
| 13 | Reproducible evaluation | Partial | `src/evaluation.rs`, `src/benchmarking.rs`: expected exact-match benchmarks, six isolated same-model ablation profiles plus optional larger baseline, per-arm resources, reviewed dataset hashes and durable run cancellation; profile/isolation/deadline tests | 100–200 diverse held-out tasks, robustness/security/regression reports, frozen study corpus and measured live-model comparisons |
| 12, 14 | CLI/service | Partial | `src/main.rs`: initialization/configuration, ask, service controls, skill lifecycle, ingestion/search, task traces, candidates/rollback, benchmark, self-test; real socket fixtures | Model listing/lifecycle, diagnostics, module discovery and task-specific remote progress/cancel |
| 12, 13 | Hardware/performance claims | Not implemented | No GPU/energy or large-model advantage claimed | Real local endpoint, hardware telemetry, equal-budget comparisons |

All 71 current tests pass, including actual socket/loopback HTTP fixtures; formatting,
all-target build checks and Clippy with denied warnings pass. Fixtures do not prove
GPU integration, neural quality, immutable backend weights or live model switching.

The last live endpoint check found no server at `127.0.0.1:1234`. Live inference is
therefore unverified. It is not a blocker for implementing independent infrastructure,
but it is a release gate. No stable release tag or remote publication is authorized.
