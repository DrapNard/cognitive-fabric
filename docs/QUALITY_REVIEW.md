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

## Lease-protected interrupted task recovery (2026-10-10)

Persistent Engine execution now acquires a shared kernel flock before task
creation. Operator recovery takes an exclusive nonblocking lease and atomically
marks abandoned running tasks interrupted with explicit no-replay/unknown-usage
audit evidence. Existing terminal results remain unchanged. Lease files reject
symlinks, public permissions, non-owner files and multiple hard links.

Three tests cover independent live leases, idempotence, terminal preservation,
audit-write failure rollback, unsafe lease paths and a real Engine future aborted
during pending inference. A separate-process CLI smoke held a shared lease,
confirmed recovery rejection, killed the holder, then confirmed successful audited
recovery and an empty second recovery. This tests kernel release after process
death; it does not prove external server computation stops.

All 71 tests passed, including local socket/HTTP fixtures. Formatting check,
all-target check and denied-warning Clippy passed offline with flatc 25.12.19.
The binary built and recovery CLI smoke passed. No automatic checkpoint resumption,
stale-socket cleanup or general storage retention is claimed; see `RECOVERY.md`.

## Persistent local model registration (2026-10-10)

Schema 5 adds immutable, canonical-hashed local inference manifests, provenance,
declared context/output limits, optional unverified weight hashes and audited
enable/disable. Explicit selection persists an exact reference and narrows existing
output budgets; task-start checks reject disabled or mismatched configuration and
record registration evidence. Manual configuration and larger-baseline overrides
do not falsely retain a different model's registration.

Four model tests cover immutable registration, disable/pinned snapshot behavior,
configuration mismatch, corruption, invalid endpoints/fields/limits/hashes,
schema-4 migration preserving tasks, truthful trace metadata and an actual local
HTTP request carrying the registered model ID. Schema-3 memory migration fixtures
now correctly omit the newly added model tables. All 75 tests passed with local
socket/HTTP fixture permissions. Formatting check, all-target check and denied-
warning Clippy passed offline using flatc 25.12.19; the binary built. A fresh-home
CLI smoke registered/listed/selected a model, executed verified arithmetic without
a live model call, denied a task after disable and allowed it after re-enable.

No weights are loaded or attested by registration. Declared context capacity is
not exact tokenizer preflight. Saved selections require service restart; physical
residency, dependency graphs and live inference studies remain open (`MODELS.md`).

## Typed progress subscriptions and durable catch-up (2026-10-10)

A third pinned FlatBuffers schema (`events.fbs`, COGE v1) carries content-minimized
task milestones and optional actual counters. Store persistence precedes ordered
local publication. Task-scoped subscriptions cap active observers at 64 and each
queue at 16 frames; overflow disconnects and reports a gap. Durable progress scans
at most 32 audit records per page, advancing over audit-only entries without
returning prompt, answer or document bodies. Recovery notifications publish only
after transaction commit. No JSON replaces FlatBuffers on the subscription path.

Four new tests cover malformed/oversized/unknown-stage frames, optional metrics,
task filtering, observer capacity/drop, overflow, live/replay equivalence, cursor
advancement, content minimization and actual Engine outcome counters. Recovery
coverage also checks committed live and cross-Store durable observation. All 79
tests passed, including local HTTP/socket fixtures. Formatting check, all-target
check and denied-warning Clippy passed offline with flatc/Rust 25.12.19. The binary
built; a fresh-home CLI smoke calculated 132, read four typed progress milestones
and confirmed empty catch-up after its final cursor. No answer content was exposed.

Live observers are trusted in-process clients of the same Store; independent-
process streaming, generic module event routing and socket-specific task controls
remain incomplete. See `EVENTS.md` for subscription/catch-up race handling.

## Task-specific Unix-socket controls (2026-10-10)

Append-only COGF fields now expose the active service task UUID and accept bounded
progress pages or targeted cancellation. Task IDs are generated by the runtime,
separate from request correlation IDs. A mutex coordinates current identity with
cancellation, preventing stale targets from cancelling a later task. Progress
responses carry independently verified COGE frames bound to the requested task,
ordered sequences and bounded cursors. Legacy absent fields remain accepted.

Two new codec tests cover task-action identity requirements, nested frame parsing,
page/task/cursor mismatches and non-monotonic sequences. Real socket tests now
read progress during blocked HTTP inference, reject unknown and actual prior-task
cancellation targets, cancel the matching execution, verify reply identity and
read its terminal cancellation event. Existing active cancellation still works.
All 81 tests passed; formatting, all-target check and denied-warning Clippy passed
offline with flatc/Rust 25.12.19. The binary built. A fresh-home real service CLI
smoke calculated 144 over the socket, read four typed progress events, confirmed
empty final catch-up, denied completed-task cancellation and stopped cleanly.

The service remains owner-local with bounded polling, not independent-process
streaming or generic executable-module authentication. External backend GPU
cancellation and live-model reasoning quality remain unverified.

## Logical database limits and reviewed retention (2026-10-10)

Schema 6 adds retention summary audits and indexed conversation references. Every
configured CLI connection applies a SQLite page ceiling (default 256 MB), with
transactional admission requiring 16 MB available/reusable pages and fewer than
eight running tasks. Retention previews are bounded, hashed, operator-approved and
revalidated under an immediate transaction. Running tasks, all published memory
sources and explicit conversation references remain protected. No automatic
deletions, VACUUM, artifact removals or backup mutation were introduced.

Three tests cover task admission capacity/headroom, actual SQLITE_FULL growth
denial, plan approval/staleness, protected references, repeated apply denial,
delete-failure rollback and retained audit counts. An initial broad test-fixture
edit affected an earlier migration and was corrected before final verification;
all 84 tests now pass, including legacy/schema-3/schema-4 migration, real socket
and HTTP fixtures. Formatting, all-target check and denied-warning Clippy passed
offline with flatc/Rust 25.12.19. The binary built. A fresh temporary-home CLI
smoke rejected unapproved retention, deleted exactly one reviewed test task,
preserved a published memory source and checked the summary audit/storage stats.

Main database pages have an enforced logical ceiling; WAL settings remain soft
checkpoint/retention controls (`wal_hard_limit=false`). Strict physical disk bounds
need filesystem quotas. Concurrent writes can still exhaust reserved headroom and
return durable-write errors. Backups, automatic resumption and archive management
remain open; see `STORAGE.md`. No real user history was cleaned up during this run.

## Frozen comparison retrieval (2026-10-10)

Comparison arms now share one consistent read-only SQLite backup held in memory.
Source read transactions prevent concurrent source updates from restarting or
changing capture. The complete image is capped at 64 MB; reports and every task
trace record its size/hash, and overall deadline accounting includes capture time.
Task results remain in the original live database. SQLite backup/serialization
features reuse the existing pinned rusqlite dependency; no new package was added.

Two new corpus tests cover oversize denial without source mutation, read-only
write rejection, subsequent document additions, episode revocation and expert
version activation while the original snapshot remains stable. Existing Engine
profile tests now prove later corpus text cannot enter frozen prompts. Comparison
tests verify identical snapshot provenance in every durable task trace.

All 86 tests passed with actual local HTTP/socket fixtures. Formatting check,
all-target check, denied-warning Clippy and binary build passed offline. A missing
temporary flatc executable initially blocked Clippy; the exact 25.12.19 generator
was restored from upstream commit 7e163021e59cca4f8e1e35a7c828b5c6b7915953,
then all required checks were rerun successfully. A fresh CLI arithmetic comparison
completed 60 executions with one 188,416-byte snapshot and 60 matching durable
provenance records. The smoke report uses the d3555e2 base plus this worktree,
not an attested immutable final build, and establishes runner mechanics only.

The checksum includes unrelated database history; it is not a content-only corpus
digest. Temporary memory includes an extra serialization buffer and SQLite
overhead. Live weight identity, availability and hardware remain unfrozen. A new
probe of 127.0.0.1:1234 found connection refused; live-model quality is unverified.

## Independent dependency-order verification

Added a deterministic verifier selected only by an explicit original-request
`order:` prefix. Strict bounded criteria reject cycles and unknown fields; answers
must satisfy every node and edge. Verification events bind criterion version,
objective hash and answer hash. Failed checks inform revisions within existing
limits; repeated candidates stop; verified answers bypass language synthesis.
The pure checker grants no filesystem, process or network access. Each invocation
requires task-scoped authorization and consumes a tool call. The alone comparison
profile has no executable checker. This is constraint validation, not general
factual verification or a neural safety evaluation.

Actual checks: exact FlatBuffers generator 25.12.19; `cargo test --offline
--all-targets` passed 88 tests including socket/HTTP fixtures. Formatting,
all-target check and Clippy with denied warnings passed. Initial test fixtures
incorrectly omitted an unverified synthesis call and charged zero tools before
retrieval; corrected the fixtures and reran the entire suite successfully.
No live-model effectiveness measurement was performed.

## Preserve independently rejected verdicts

Inspection found ordering failures could be followed by language synthesis,
replacing the candidate bound by the verification hash. Failed checks now produce
`rejected` outcomes and bypass synthesis, just as passing checks do. Model assertions
cannot alter that verdict. Added malformed-criterion/no-dispatch and FAST rejection
coverage; rejected episodes preserve their status. Reviewed retention includes
unreferenced rejected tasks, with existing protections unchanged.

Testing revealed typed progress omitted the existing arithmetic `rejected` state.
The validator now accepts it, with an actual division-by-zero engine/progress/codec
regression. No schema change is needed: state is a validated string. Final checks:
89 tests pass with actual socket/HTTP fixtures; all-target check, formatting and
Clippy with denied warnings pass using FlatBuffers 25.12.19. No live model quality
or additional inference capability is claimed.

## Score dependency constraints across comparison arms

Added explicit `dependency_order_v1` comparison rows, preserving legacy exact-match
rows. Criteria are validated for every case before execution and reject ambiguous
expected labels. Scoring independently checks candidate constraints even when a
runtime reports completed; unverified baseline answers can pass the same check.
No expected label is passed into inference. A real loopback HTTP fixture returns
an alternative valid order across all six arms and verifies per-case report metadata.
Additional negative tests cover malformed criteria with zero task creation, invalid
completed answers, excluded terminal statuses and unknown criterion names.

Final checks: 92 all-target tests pass, formatting/all-target check/Clippy with
warnings denied pass using FlatBuffers 25.12.19. One Clippy fixture-initialization
warning was fixed before the final checks. This HTTP fixture verifies integration,
not real model inference or quality; no performance advantage is claimed.

## Bounded operator diagnostics

Added local diagnostics for schema/storage/registered selection and an explicit
optional operator model-list probe. The probe is GET-only, three-second bounded,
64 KiB/256-ID limited, no proxy/redirect, using the same loopback URL boundary as
inference. Model advertisement does not imply generation, residency or weight
attestation. Reasoning actors cannot invoke the diagnostic capability. Diagnostics
creates no tasks/events, though normal CLI workspace initialization still applies.

Actual checks: all 94 tests pass including socket and HTTP fixtures; formatting,
all-target check, Clippy with denied warnings and CLI build pass with FlatBuffers
25.12.19. CLI offline smoke reports schema 6/storage and no generation. An initial
smoke used an invalid positional configure-local argument; corrected to `--model`
and reran. The corrected local endpoint probe returned a failed discovery report
and exit 1 at 127.0.0.1:1234. No live model success is claimed.

## Gap-guided reviewed candidate preparation

Added operator gap inspection/preparation and RAG import from recorded Jev+ gaps.
Reads are bounded to 64 events and 16 KB/event, capability identities are validated,
and model advice is labeled untrusted. Reviewed artifacts use existing quarantine,
licensing, hashes and quotas. Candidate metadata and provenance link are committed
in one immediate transaction after gap hash revalidation. Linked task histories
are protected against reviewed retention. No permission, source acquisition,
registration, evaluation threshold change or automatic promotion occurs.

Tests cover preparation without candidate creation, available-skill reuse metadata,
missing/invalid gaps, model capability denial, immutable evidence binding, injected
audit rollback, inactive candidate status and protected provenance retention. Final
checks: 96 tests pass including actual socket/HTTP fixtures; formatting, all-target
check and Clippy with denied warnings pass using FlatBuffers 25.12.19. An initial
test incorrectly assumed optional active_version; corrected to the existing zero
inactive version. Source discovery/cleaning and poisoning evaluation remain partial.

## Measured module queue waiting

Added runtime-owned enqueue-to-worker timing, overriding backend queue claims.
Optional bounded finite values cross append-only FlatBuffers response/resource
fields and accumulate separately from existing total caller inference duration.
Zero-call tasks report zero; missing legacy values and cancelled dispatches retain
unavailable totals. Comparison totals preserve uncertainty. No queue capacity,
permissions or model-call budget increase occurred. This is broker scheduling
telemetry, not GPU, energy or model-load measurement.

Controlled contention test holds the first backend call, queues a second and
confirms its measured delay. Codec tests cover absent/finite/negative/nonfinite/
over-limit values and owner IPC legacy defaults; cancellation retains unknown totals.
Final checks: 98 all-target tests pass including actual socket/HTTP fixtures;
formatting, all-target check and Clippy with denied warnings pass with flatc/Rust
FlatBuffers 25.12.19. Initial compilation identified constructors and owner IPC
resource paths requiring the added optional field; fixed before final checks.

## Version-pinned bounded prompt/workflow experts

Added optional workflow manifest text with strict kind-specific bounds and legacy
canonical hash preservation. Explicit operator registration/activation retains
existing dependency and immutable-version checks. Full/experts execution admits
at most one routed workflow advice call under the shared task policy/broker/model;
reserves planner/final-solver calls, tokens and conservative byte headroom; zero
cycles prevent advice dispatch. Audit binds selected manifest version/checksum.
Text and results remain advisory and cannot grant permissions or verification.

Tests cover enabled execution, isolated profiles, insufficient calls, zero cycles,
version-pinned text across replacement, disable behavior and invalid kind/text
bounds. Final 99 all-target tests pass including actual socket/HTTP fixtures;
formatting, all-target check and Clippy with denied warnings pass with FlatBuffers
25.12.19. No live usefulness or expected-utility measurement is claimed. Multi-step
workflows, executable experts and independent backend loading remain partial.

Built CLI smoke: registered `examples/workflow_manifest.json`, activated
rust.ownership.workflow.s1 1.0.0 and inspected skill-health in an isolated /tmp
workspace; commands succeeded and reported valid dependency metadata. This smoke
performed no live generation and did not modify the user's normal workspace state.

## Consistent operator SQLite backups

Added SQLite backup API copying from a pinned read snapshot, separate nonblocking
flock admission, owner-only temporary storage, quick consistency checking, streaming
hash verification and immutable file publication. Limits: eight entries, per-file
configured DB ceiling, total twice that ceiling capped at 2 GB. Copy/hash loops
check a ten-second deadline; blocking filesystem calls/quick_check are not a hard
real-time guarantee. Other WAL writers can continue; same-Store users wait on the
connection mutex and checkpoints may be delayed by the read snapshot.

Tests verify committed WAL task/event preservation, unchanged backup after later
writes, checksum/permissions, restored read-only data, backup lease contention,
byte/count ceilings, symlink denial and model capability denial. Initial use of a
SQLite write reservation prevented backup reads; replaced it with a separate file
lease and pinned read transaction. Final 100 all-target tests pass, formatting,
all-target check, Clippy with denied warnings and CLI build pass with FlatBuffers
25.12.19. Actual CLI smoke backed up a completed 8*9 task, independently matched
SHA-256 and reopened a copy in a separate /tmp workspace with result 72 and its
audit events intact. No user's normal state was replaced. Configuration/external
artifacts/weights are excluded; full workspace restore remains incomplete.

## Bounded socket progress subscriptions

Appended SubscribeProgress and optional stream state without changing existing
FlatBuffers fields or ordinals. Owner-authenticated watchers subscribe before
durable replay, deduplicate by sequence and receive only task-bound typed events.
Limits: two service watchers, 16 queued frames each, 32-row replay pages, two-second
writes and 60-second watches. Overflow is an explicit error requiring durable
resume. Disconnecting observation does not cancel execution. Other Store instances
require reconnect/replay; their writes do not publish through this service hub.

Tests cover catch-up followed by live delivery, unrelated-task exclusion, cursor
resume, completion, unknown tasks, watcher capacity preserving controls, overflow,
shutdown with a pending watcher and invalid stream states. All 103 all-target
tests pass; formatting, all-target check, CLI build and Clippy with denied warnings
pass with FlatBuffers 25.12.19. No live inference or neural-quality claim follows
from these socket tests.

Separate-process CLI smoke also passed: started `serve` in an isolated /tmp home,
watched a previously completed demo task with `--json`, received one terminal
JSONL page with cursor 17 and eight typed events, then stopped the service through
typed control and confirmed clean exit. This verifies cross-process durable
catch-up/termination; the integration tests exercise live delivery and overflow.
