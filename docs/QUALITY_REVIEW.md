# Rust baseline quality review

Verified locally on 2026-10-10 using rustc/cargo 1.97.1 and flatc 25.12.19,
compiled from upstream tag v25.12.19 (commit
7e163021e59cca4f8e1e35a7c828b5c6b7915953). The Rust crate is pinned to the same
version. Set `FLATC` to an absolute compiler path when it is outside PATH.

- `cargo fmt --all -- --check`: initially failed; source formatted.
- `cargo check --workspace --all-targets --offline`: passed after fixing
  generated API usage, the crate-level alloc import and a query lifetime.
- `cargo test --workspace --all-targets --offline`: 14 passed.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `cargo run --offline -- --home /tmp/cogfabric-audit-state self-test`: passed
  arithmetic and FlatBuffers round trip.

Cargo commands requiring generation used
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc`. Dependencies were downloaded
with `cargo fetch`; Cargo.lock records the resolution. No live-model quality,
GPU behavior, socket integration, cancellation, migrations or rollback is
established by these checks. The release remains alpha.

## Inference boundary verification

The suite now has 17 passing tests, including a real loopback HTTP fixture for
compatible responses, redirects, oversized declared responses and oversized
bodies without a content length. Endpoint tests cover IPv4, IPv6, localhost,
remote hosts, credentials, query strings and fragments. Loopback binds are
blocked by the default execution sandbox, so the full suite was rerun with
socket permission; fixture setup fails explicitly when binding is unavailable.
This is protocol testing, not live-model inference validation.

## RAG lifecycle and migration verification

All 19 tests pass, including migration from the unversioned schema, reopen
idempotence, future-schema rejection, approved-version retrieval selection,
operator approval, unauthorized rollback, candidate and tampered-snapshot denial,
transactional audit counts and rollback durability. Formatting, workspace check
and Clippy with warnings denied pass. Full tests use local socket permission.

## Socket lifecycle verification

All 23 tests pass with socket access, including a real Unix socket round trip,
0600 permissions, existing-socket preservation, malformed lengths, interruption
of incomplete frames, cancellation during stalled HTTP on disconnect/shutdown,
and persistence before any tool runs on immediate cancellation. Formatting,
workspace check and Clippy pass. No inference-quality claims follow from fixtures.

## Bounded recursive reasoning verification

All 30 tests pass after adding scripted actual revision order, stagnant-answer
stopping, malformed-critique denial, call/cycle/output/input-byte/tool quotas,
cancellation-safe usage accounting, provider usage parsing and legacy FlatBuffers
metrics compatibility. Formatting, workspace check and Clippy pass.

`cargo run --offline -- --home /tmp/cogfabric-verified-state bench examples/arithmetic_bench.jsonl`
passed 10/10 expected outputs with zero model calls (6.423 ms total in this run).
This measures the deterministic arithmetic path only, not neural reasoning gains.
A permitted loopback request to `http://127.0.0.1:1234/v1/models` failed to connect;
real-model validation remains blocked by the unavailable default inference server.

## Expert registry and snapshot verification

All 34 tests pass. New tests cover staging, immutable-version conflicts, dependency
and reverse-dependency constraints, missing dependencies, cycles, tampered active
metadata, permission injection, metadata rollback, stable task snapshots and
exclusion of disabled skills from retrieval/routing. Formatting, workspace check
and Clippy pass. CLI registration, activation and health were exercised using
`examples/reference_manifest.json` in `/tmp/cogfabric-registry-smoke`; health
correctly reports zero documents and unsupported neural loading.

## Persistent configuration verification

All 37 tests pass, including configuration/profile round trips, owner-only saves,
invalid settings and symlink rejection, bounded regular-file reads, and HTTP
request assertions for per-role model/temperature selection. Formatting, workspace
check and Clippy pass. configure-local (shared model and critic profile) and
config-show were smoke-tested in `/tmp/cogfabric-config-smoke`. The configured
fixture label does not imply an installed or loaded model.

## Language interface verification

All 42 tests pass after adding strict interpretation parsing, clarification,
malformed-intent fallback, preservation of the original user goal despite a
malicious interpretation, missing-capability tracking, status-labelled synthesis,
explicit historical-reference isolation and typed conversation-option round trips.
Formatting, workspace check and Clippy pass. Scripted outputs validate orchestration
and boundaries only; live-model interpretation quality remains unverified.

## Candidate quarantine verification

All 46 tests pass, including source mutation isolation, content deduplication,
read-only artifact permissions, symlinked directory denial, aggregate quota denial,
modified-quarantine detection and concurrent unique version allocation. Formatting,
workspace check and Clippy pass. CLI candidate create was exercised in
`/tmp/cogfabric-quarantine-smoke`; its artifact is stored under the managed hash path.

### Held-out dataset and baseline regression gates
`cargo fmt --all -- --check`, `cargo check --offline --all-targets`, `cargo test --offline --all-targets` (48 passed), and `cargo clippy --offline --all-targets -- -D warnings` passed using pinned FlatBuffers 25.12.19. Full tests ran with local socket/loopback permissions. New negative tests cover duplicate normalized queries, missing provenance/attestation, loss of previously passing retrieval facts, and stale baseline promotion. This validates retrieval gates, not neural safety or live inference.

### Typed live service controls
Pinned flatc 25.12.19 regenerated the additive schema. Formatting, all-target check, all-target Clippy with denied warnings, and all-target tests passed (50 tests). Socket tests cover remote stop, incomplete clients, active status, busy errors, remote cancellation, disconnect cancellation and shutdown cancellation. Unknown controls and ambiguous envelopes fail validation. Control authorization remains same-UID local owner access.

### Dependency-aware subtask execution
All-target check and tests passed (54 tests); formatting passed. Clippy identified a redundant integer comparison, corrected without changing behavior. New tests verify strict acyclic plan validation, stable dependency order, shared call/token ceilings with whole-goal fallback, and cancellation of running/pending subgoals. Subtask results remain unverified and confer no tool capabilities.

### Immutable task capability scopes
Formatting, all-target check, all-target tests (56 passed), and Clippy with denied warnings passed using the pinned compiler. New tests verify task replay rejection, actor narrowing, denial of promotion grant creation, and task-linked audit decisions without charging denied tool invocations. No external executable capabilities were added.

### Controlled execution comparisons
Formatting, all-target check, all-target tests (59 passed), and Clippy with denied warnings passed. Tests verify solver-only recursion, disabled reference stages, candidate-document exclusion in the memory arm, expected-answer isolation, duplicate rejection and durable deadline/operator cancellation. CLI comparison smoke completed 60 executions over the 10 arithmetic examples: demo-alone produced no accepted answers; each tool-enabled arm passed 10/10 with zero inference calls. This tests ablation mechanics, not neural performance. The recorded base revision is cd8340242cbc76043840c86e0870d3f7cbd50202 plus the comparison worktree changes; no live model or immutable weight attestation was available.

### Integrated typed inference broker
Pinned flatc 25.12.19 generated both owner-IPC and internal module schemas. All-target check, formatting and all-target tests passed (64 tests); Clippy with denied warnings passed. Four new broker tests cover bounded queues/cancelled futures, forged/expired work, malformed frame bounds and immutable pinned replacement/unregistration. Discovery is exercised over the real Unix socket; an additional legacy-control fixture verifies absent appended module fields. Internal dispatch now serializes validated FlatBuffers with task/correlation/actor/destination/version/expiry metadata. This remains trusted in-process adapter routing, not independent-process attestation, neural loading or external GPU cancellation.

## Cumulative RAG regressions (2026-10-10)

Approved reviewed quality suites now reject replacement. Candidate regressions
cover all approved versions, bounded to 128 versions and 8,192 cases. Promotion
checks the historical version list inside its transaction, as well as the active
version. The regression test covers loss of first-version capabilities after
multiple successful upgrades and rejection of approved-suite replacement.

Validation: `cargo fmt --all -- --check`, `cargo check --all-targets --offline`,
`cargo test --all-targets --offline` (64 passed), and
`cargo clippy --all-targets --offline -- -D warnings` passed using flatc 25.12.19.
Socket/HTTP fixtures ran with local networking permissions. No live model study
was run. Paired quality/regression writes remain non-atomic; independent neural
safety evaluation remains unavailable and neural promotion stays disabled.

## Atomic RAG reviews (2026-10-10)

Quality and regression records now save in one immediate SQLite transaction,
sharing a runtime-generated review UUID. Promotion rejects mismatched or missing
review identities, including legacy unpaired records until re-evaluation. A
fault-injection trigger rejects the second insert; the test verifies no partial
review survives. Another negative check rejects independently recorded regression
results even for the same artifact. This supersedes the non-atomic write limitation
recorded above. No model permissions or neural promotion capabilities changed.

Validation with flatc 25.12.19: formatting check, all-target check, all-target tests
(65 passed, including local socket/HTTP fixtures), and Clippy with denied warnings
passed offline. No live-model inference or reasoning-quality claim is made.

## Operator-published reusable memory (2026-10-10)

Schema 4 adds bounded FTS5 episodic snapshots with source-task identity, immutable
content hashes, review notes, transactional publication/revocation audits and
future-retrieval revocation. Only operator actions publish entries. Arithmetic
verification independently recomputes completed answers; generic episodes remain
unverified. Full and memory-profile tasks pin labeled entries; other comparison
arms omit them. No arbitrary fact promotion or additional model permission exists.

Validation: schema-3 and legacy migrations preserve tasks; disk reopen, revoked
retrieval, duplicate publication, invalid fact kind, mismatched arithmetic, source
mutation, snapshot tampering and model permission denial are covered. Profile tests
check memory provenance and hostile text remains labeled data without upgrading
result status. `cargo fmt --all -- --check`, `cargo check --all-targets --offline`,
`cargo test --all-targets --offline` (68 passed), and denied-warning Clippy passed
with flatc 25.12.19. `cargo build --offline` passed. A fresh-home demo CLI roundtrip
calculated 72, published independently verified arithmetic memory, searched it,
revoked it and confirmed empty search. No live-model injection-resistance claim.
General database retention, wider fact verifiers and model registry remain open.
