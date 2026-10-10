# Implementation matrix — 1.1.0-alpha.1

Canonical requirements remain in the complete French `ARCHITECTURE.md`.
Baseline verified 2026-10-10: builds, 14 tests and Clippy pass; see
`QUALITY_REVIEW.md`. Development is on `develop/v1.0`. No stable release is ready.

| Architecture sections | Requirement | Status | Implementation / evidence | Remaining work |
|---|---|---|---|---|
| 7, 12 | Typed local transport | Partial | `protocol/cognitive.fbs`, `src/wire.rs`, `src/service.rs`; round-trip, garbage and size tests | Socket integration, identities, broker, discovery, events, cancellation, shutdown |
| 3 | Language interface | Partial | `src/backend.rs`, `src/engine.rs`; explicit demo test | Structured intent, conversations, configurable roles, live endpoint validation |
| 4, 9 | Jev+ coordination | Partial | `src/store.rs::route`, `src/engine.rs`; routing test | Dependencies, scheduling, escalation, capability gaps |
| 5 | Jev-R reasoning | Partial | `src/engine.rs`; zero-call arithmetic verification | Recursive revisions, task graph, Verifier and Reflector, independent evidence |
| 6 | Expert network | Partial | `src/store.rs`, CLI skill-add | Versioned manifests, dependencies, lifecycle, health, rollback, trainer category |
| 7 | Persistent memory | Partial | SQLite WAL/FTS5 and task/events in `src/store.rs`; retrieval gate test | Explicit migrations, model/evaluation stores, task isolation, quotas |
| 8 A/B | RAG evolution | Partial | `src/evaluation.rs`, `src/store.rs`; approval and retrieval test | Immutable quarantine, independent dataset provenance, rollback, regression suite |
| 8 C/D | Offline neural adaptation | Experimental | `training/train_lora.py`; not executed | Independent evaluator, artifact manifests; reconstruction remains research |
| 9 | Resource limits | Partial | `src/config.rs`, `src/engine.rs` | Input tokens, actual usage, tool limits, cancellation, queue telemetry |
| 10, 11 | Security kernel | Partial | `src/security.rs`; actor/self-promotion denial tests | Task-bound authorization, artifact tampering tests, secure artifact handling, sandbox before untrusted execution |
| 13 | Evaluation | Partial | `src/evaluation.rs`, examples | Comparable baseline arms, 100–200 diverse tasks, regression/security measurements |
| 12, 14 | CLI/service integration | Partial | `src/main.rs`, `src/service.rs`; CLI self-test passed | End-to-end sockets, stop/progress/configuration, diagnostics |
| 12, 13 | Hardware metrics / performance advantage | Not implemented | No measurements claimed | Real backend, GPU/energy telemetry and controlled comparisons |
| 5, 14 | Novel learned neural architectures | Not implemented | Future research | Separate research and independent evaluations |

The historical Python reference remains in Git history. The Rust state directory
is separate (`~/.local/share/cogfabric-rust`). The canonical vision is preserved;
passing demo tests does not establish neural quality or release acceptance.

Inference boundary update: `src/backend.rs` implements bounded HTTP with
redirect/proxy denial and literal loopback endpoint validation. Three additional
tests pass (17 total). Real LM Studio/llama.cpp inference is still unverified.

RAG lifecycle update: schema version 1 migrates legacy state transactionally;
`src/store.rs` filters retrieval to the active candidate snapshot and records
promotion/rollback audit metadata. `candidate rollback ID --human-approve` restores
an approved older RAG version after stored content integrity checks. Migration,
rollback failure and reopen tests pass (19 total). External artifact quarantine,
full task version pinning and general registry lifecycle remain Partial.

Socket lifecycle update: `src/service.rs` enforces OS peer UID, one active handler,
bounded frame/response I/O, disconnect cancellation and joined shutdown with
inode-checked cleanup. `Engine::run_until` persists cancellation and wall-budget
termination. CLI Ctrl-C uses these paths. `docs/PROTOCOL.md` documents semantics.
Real Unix socket and blocked-inference cancellation regressions pass; broker,
module identity, typed stop/status/cancel commands and general discovery remain Partial.

Reasoning update: `src/reasoning.rs` implements bounded critique/reflect/re-solve
cycles, no-progress stopping, strict advisory critique parsing, persisted typed
role dependency nodes and replaceable `InferenceBackend`. Verifier-R independently
verifies arithmetic; general generated results have status `unverified`. Calls,
reserved output tokens, input bytes, tool invocations and time are bounded;
provider token usage remains optional and is carried through typed FlatBuffers
fields. Tests cover actual revisions, stagnation, malformed critique, budget
exhaustion and legacy replies. General subtask DAGs, role-specific configurations,
exact input-token preflight and hardware metrics remain Partial / Not implemented.

Expert lifecycle update: `src/registry.rs` implements immutable semantic-versioned
reference manifests, bounded registration, staging, dependency/reverse-dependency
validation, cycle rejection, activation/metadata rollback, disable and integrity
health checks. Schema 2 adds persistent expert manifests and audit records.
`Store::task_context` atomically snapshots versions and documents for in-flight
pinning. Tests verify conflicts, missing dependencies, cycles, metadata tampering,
permission injection, snapshot stability and disabled retrieval. Neural/executable
expert loading remains unsupported; categories include trainer.s1 as metadata only.

Configuration update: `Config` supports bounded validated persistent settings,
atomic owner-only saves, environment overrides, independently selected role
models/temperatures and explicit resource ceilings. `configure-local` and
`config-show` expose it. Loopback HTTP fixtures verify role profile selection.
Reviewed configuration, manifests, documents, benchmarks and candidate checksum
reads now share bounded no-follow regular-file opening in `src/security.rs`.
Language interface generation and real loaded-model validation remain outstanding.

Language interface update: `src/language.rs` defines bounded strict structured
interpretation; `Engine` keeps the original goal authoritative, requests
clarification when needed, tracks missing capabilities and refines existing
routes without granting permissions. CLI/socket support explicit conversation
references and optional result synthesis under the shared budget. Scripted tests
cover injection, goal preservation, ambiguity, fallback and historical isolation.
Real-model quality and exact input-token preflight remain unverified/incomplete.

Candidate quarantine update: `src/evolution.rs` imports operator-selected artifacts
into content-addressed read-only snapshots with deduplication, directory/symlink
checks and aggregate storage/entry limits. Candidate version allocation is
transactional and bounded. Tests cover source mutation, modified quarantine,
quota denial and concurrent imports. Legacy external-path candidates remain
compatible; held-out dataset manifests and regression promotion gates remain Partial.

### Reviewed RAG evaluation datasets
Implemented: schema-3 immutable dataset manifests, explicit held-out provenance, duplicate query rejection, isolated FTS evaluation, active-baseline retrieval regressions and stale-baseline promotion rejection (`src/evaluation.rs`, `src/store.rs`). Tests cover losing old facts despite passing new cases and concurrent baseline replacement. Partial: independence remains operator-attested; cumulative historical regression suites are not yet retained.
