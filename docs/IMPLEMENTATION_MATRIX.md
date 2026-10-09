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
