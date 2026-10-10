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

Verification: 88 tests, formatting, all-target check and Clippy pass using exact
flatc/Rust FlatBuffers 25.12.19. HTTP fixtures and socket tests ran locally; no
compatible live model server was available at the default endpoint. No neural
quality, GPU integration, hot-swap, energy or model-superiority result is claimed.
See `IMPLEMENTATION_MATRIX.md` for every remaining engineering requirement and
`QUALITY_REVIEW.md` for actual command evidence.

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
