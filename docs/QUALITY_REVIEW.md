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

## Optional Rust process memory measurements

Added a fixed-path no-follow Linux procfs read bounded to 16 KB, with checked byte
conversion, strict units/duplicate-field rejection and null fallback. Non-Linux
platforms report unavailable values. Sampling occurs once at task completion,
outside the inference timer; no polling thread, subprocess or new model permission
is introduced. Diagnostics also samples its own calling process.

Task resources/audits and appended optional FlatBuffers fields preserve RSS and
process-lifetime high-water RSS. Comparison totals use maxima and propagate missing
samples. These include the whole Rust process and previous tasks; external model
servers, GPU memory, task-attributable peak and energy remain unmeasured.

All 106 all-target tests pass, including parser bounds/units/duplicates, aggregation
and legacy IPC defaults; formatting, all-target check, Clippy with denied warnings
and CLI build pass with FlatBuffers 25.12.19. Actual isolated CLI arithmetic smoke
returned 36 with zero model calls and measured RSS/high-water 16,527,360 bytes;
separate diagnostics measured 15,159,296 bytes. These single-process observations
are not a comparative memory benchmark or inference-server measurements.

## Offline trainer termination and provenance preflight

Fixed an infinite training-loop path when all examples lose supervised labels to
truncation. A complete no-progress pass now fails. Input is read once through a
bounded no-follow regular-file descriptor; the manifest hashes that snapshot and
the actual deduplicated rows rather than rereading potentially changed data.
Strict fields, provenance bounds, normalized duplicate conflicts, seeded settings,
new disjoint output, safe-format loader requests and bounded output inventories
strengthen this manually invoked experimental path. Loader options follow the
official Transformers model-loading API; they do not provide process isolation.

Six standard-library tests pass: snapshot changes, malformed/duplicate/conflicting
data, symlink/size rejection, no-supervision termination guard, hashed artifact
inventory and subprocess dry-run/output separation. Fixtures do not contain valid
neural weights. All 106 Rust tests and mandatory fmt/check/Clippy checks pass with
FlatBuffers 25.12.19. CI now includes offline preflight tests without ML installs.
Environment inspection confirms torch, transformers, peft, safetensors and
bitsandbytes are absent. No training/GPU run is claimed. Model provenance manifests,
held-out neural evaluations and independently enforced isolation remain required;
neural promotion remains disabled.

## Bounded inline SQL tool

Added explicit original-request SELECT tasks over fresh in-memory inline tables.
Validated identifiers and bound values separate data from generated setup SQL.
After setup, query-only mode and a deny-default SQLite authorizer block mutations,
schema/host access, ATTACH, PRAGMAs, extensions and non-allowlisted functions. Batch
preparation rejects multiple statements before execution. Output preserves types
and fails on blobs/nonfinite numbers or exceeded bounds, without partial success.

Enforcement combines 12 KB engine requests (32 KB standalone tool bound), 4 KB SQL,
four 16-column/256-row tables, 64-row/16 KB results, 128 main database pages,
compiled-program/depth/function limits, approximately 100,000 VM operations and a
100 ms query deadline checked by progress/row iteration. Two static worker slots
bound queued/running blocking jobs and stay held after caller cancellation; dropped
futures signal cancellation. These are in-process guards, not hard heap/real-time
or OS isolation guarantees. No external connector or arbitrary native code exists.

Tests verify arithmetic aggregates, finite recursion, actual interrupted infinite
recursion, row/input/schema limits, isolation, nonfinite output, multiple-statement
and host/mutation/function denials, task budgets, actor denials, alone-profile
separation and persisted status. All 110 Rust tests pass; fmt, all-target check,
Clippy with denied warnings and CLI build pass with FlatBuffers 25.12.19. Isolated
CLI smoke returned total 5 with zero model calls, data.sql.s1 routing and hashed
input/answer evidence. Completion verifies the supplied query over supplied data,
not real-world truth or general model-generated SQL correctness. Dynamic tool
manifests and external executable expert isolation remain incomplete.

The negative CLI smoke also rejected `randomblob` with status `rejected`, no
verification evidence and exit code 1. This confirms denied execution is visible
to shell callers rather than reported as a successful task.

## Dependency-ordered expert advice workflows

Added mutually exclusive structured workflow_steps alongside legacy text. Strict
manifest validation bounds one to four steps, 1 KB per-step/2 KB total advice,
identifiers and acyclic dependencies. Omitted new fields preserve exact legacy
canonical serialization. One routed pinned workflow runs through the shared
solver backend; dependency output is bounded to 600 characters and labeled
unverified. Each step audits identity/state/result hash. The final solver receives
the bounded complete result map, without tools or additional authority.

Admission checks all step calls/tokens and conservative escaped prompt/dependency
headroom plus final planner/solver capacity; actual calls retain independent hard
limits. Refusal skips the entire workflow before advice calls. No partial graph
is reported as successful after interruption. This is a bounded advisory DAG,
not executable/conditional plugin code, automatic resumption or measured utility.

Tests cover exact legacy serialization, duplicate/missing/cyclic dependencies,
empty/over-count/over-size graphs, actual two/four-step execution order, dependency
data, final planner admission, pinned replacement/rollback, profile isolation and
call/token/input-byte refusal. All 112 Rust tests, fmt/check/Clippy and CLI build
pass with FlatBuffers 25.12.19. CLI smoke registered/activated the structured
example in an isolated /tmp home and reported valid dependency metadata. Execution
tests use the recording fixture backend; no live model-quality claim is made.

## Checked database restoration into new workspaces

Added operator-only restore-database before normal CLI Store/Engine initialization.
Exclusive mkdir refuses existing paths; bounded no-follow reads copy one reviewed
standalone snapshot through 64 KB buffers. Saved SHA-256, source/staging identities,
SQLite quick_check, DELETE journal and exact schema 6 must pass before no-overwrite
publication. A private versioned restore manifest records provenance without
altering the copied database. Model actors have no restore capability.

Running tasks remain unchanged and are never replayed. Configuration, external
artifacts/weights and candidate paths are not replaced. Input uses the configured
DB ceiling; ten-second copy/validation checks are soft around blocking I/O/SQLite
and fsync. Failed attempts can leave a reserved private directory; publication/
sync failure can leave valid data despite an error. Same-owner hostile tampering
is outside an absolute guarantee; source/staging/parent identity checks reduce
accidental replacement risk. Older-schema migration and full workspace archives
remain incomplete.

Tests cover copied state, unchanged running status, provenance records, destination
and file permissions, existing paths, checksum/symlink/corruption/schema/WAL
denials and model authorization denial. All 114 Rust tests, mandatory format/check/
Clippy checks and CLI build pass with FlatBuffers 25.12.19. Actual isolated CLI
smoke restored a 188,416-byte backup, independently matched its SHA-256 before
reopen, and retained completed result 56 plus all 11 audit events. The configured
control home was not created. No live inference or task replay occurred.

## Direct broker task identity

The broker inherited the backend's default generate_for method, which discarded
the supplied task ID before generating a new one. Engine-pinned dispatch was
unaffected. Explicit forwarding now preserves the caller UUID through both paths.
A task-aware fixture observes concurrent distinct UUIDs and rejects malformed
IDs on each path. All 115 Rust tests, workspace check, formatting and Clippy pass
with FlatBuffers 25.12.19. No new capability or process authentication is granted.

## Host-connected Unix inference adapter

Trusted host code can register an already-connected UnixInference adapter with
an immutable descriptor and approved peer UID/PID. OS credential checks precede
use. Four-byte length-prefixed COGM frames enforce existing bounds, task policy,
roles, correlation/version/expiry and output-token ceilings. Serialized exchanges
have a 300-second ceiling; the surrounding broker supplies bounded admission.
Taking ownership of the stream before I/O means cancellation or invalid responses
permanently close it, preventing stale cross-task response reuse.

Six tests cover reusable exchanges, real separate-process credentials and broker
dispatch, wrong UID/PID, malformed task/role, forged task response, oversized frame
length and cancellation disposal. The child is a deterministic fixture, not a
model or sandbox. All 121 tests, workspace check, formatting and Clippy pass with
FlatBuffers 25.12.19. No new dependency, model process launch, executable permission
or remote network access is added. Worker-side authorization, production lifecycle,
executable attestation, isolation and general routing remain unfinished.

## Worker-side inference authority

Worker Authority is trusted, non-deserializable setup holding approved host
UID/PID, immutable descriptor and at most 64 task UUID grants. Frames cannot add
grants. Each dispatch checks destination/version, role-derived actor, task scope
and bounded expiry, then charges cumulative calls/reserved output/prompt bytes.
Sessions are sequential and bounded by 300 seconds/1024 exchanges; reads/writes
have additional deadlines. Disconnect, extra bytes, shutdown and expiry drop the
backend future. Provider output is independently validated before transmission.

Five tests cover adapter/worker exchange, cumulative call/token/byte ceilings,
task/actor/version/deadline denials and active disconnect/shutdown cancellation.
All 126 tests, workspace check, formatting and Clippy pass with FlatBuffers
25.12.19. Initial Clippy findings in test slice construction were corrected.
No autonomous process launch or sandbox guarantee is added. Trusted cross-process
grant provisioning, worker audit persistence and production lifecycle remain
incomplete. Dropped HTTP futures do not prove external GPU work stopped.

## Durable worker audit attachment

Worker serve now requires a trusted Audit object bound to an existing SQLite
task. Bounded versioned metadata records session start/termination, authorization,
denial, inference outcome and cumulative reservations. Untrusted requested task
IDs appear only as claims inside records owned by the trusted anchor. Prompts,
answers, backend errors and provider metadata are excluded. Anchor existence and
insertion share one SQL statement, so removed anchors cannot produce orphan rows.
Authorization-write failure prevents dispatch; post-inference audit failure closes
the session but cannot undo inference. Expiry is rechecked after audit writes.

Three new tests verify actual file database reopening and record privacy, denied
task attachment, missing/deleted anchors and injected authorization-write failure
with zero backend calls. All 129 Rust tests and mandatory quality checks passed;
the eight worker tests and Clippy passed again after expiry rechecks. FlatBuffers
remains 25.12.19. SQLite calls retain existing busy/I/O limits, not hard real-time
guarantees. In-memory test stores do not supply durability. Abrupt process/future
termination may leave an open audit session; crash reconciliation, production
launch and cross-process grant provisioning remain incomplete.

## Typed single-task worker bootstrap

Added COGB v1 schema and codec for a task UUID, pinned backend/model/role generation
settings and cumulative worker ceilings. At most 16 KiB is accepted; existing
Config validation enforces loopback endpoints and settings bounds. Duplicate
roles, unknown versions, malformed/oversized frames and invalid ceilings fail
closed. Workspace paths, executable permissions and model-registry authority are
not transmitted. Trusted callers can convert validated settings into worker task
authority. Authenticated receive checks OS UID/PID before bounded length-prefixed
reads (10 seconds); send has a two-second deadline. Raw decode alone confers no
sender authentication.

Four codec/socket tests verify preserved settings, absent workspace paths,
malformed/forged configuration and authenticated delivery. The existing complete
adapter/worker test now consumes decoded bootstrap authority. All 133 Rust tests,
workspace check, formatting and Clippy pass with exact flatc/runtime 25.12.19.
The temporary compiler was absent and was rebuilt from the official v25.12.19
source before verification; no generated files or compiler binaries were committed.
Production process launch, lifecycle integration and crash reconciliation remain
unfinished. Model names/configuration are declarations, not weight attestation.

## Managed same-binary inference lifecycle

Added opt-in persisted inference_process mode and operator-only configure-worker.
Engine tasks lazily launch the current executable's internal worker entry point,
with cleared environment and private temporary socket resources. Both peers check
OS credentials; bootstrap settings pin one task and generation/database ceilings.
Child startup reads no mutable workspace configuration. One child is reused across
roles; zero-inference tasks launch none. Custom registries fail explicitly instead
of being silently replaced. Injected fixture backends retain their implementation.

Task termination closes the stream, waits two seconds, requests termination if
needed and waits two more seconds for reaping; cleanup errors fail the task.
Kill-on-drop remains a fallback. Socket cleanup checks directory/socket identities.
Clean idle EOF is distinguished from partial frames. Launch handoff and confirmed
reaping metadata are audited; no model-load, child/GPU memory or sandbox proof is
claimed. Execution budgets stop reasoning; cleanup can add bounded grace time.

All 138 tests (135 unit plus 3 actual CLI integration tests), workspace check,
formatting and Clippy pass with FlatBuffers 25.12.19. The integration tests verify
one demo child reused across a task, arithmetic without a child, and Ctrl-C during
pending loopback HTTP inference with disconnect and confirmed reaping. An initial
Clippy test-module placement warning was corrected. A fresh isolated diagnostic
probe of 127.0.0.1:1234/v1/models failed; live generation was not tested. The demo
and HTTP fixtures do not establish model quality or GPU operation.

Managed mode is same-user coordination, not arbitrary executable isolation.
General module lifecycle, crash/audit reconciliation and stale temporary-resource
cleanup remain incomplete. Worker sessions retain their tighter 300-second/idle
ceilings; no automatic restart or task replay is provided. See `WORKERS.md`.

## Approved worker lifetime and incomplete-frame deadlines

Managed bootstrap now appends the approved task max_wall_seconds. Its default
300 preserves legacy frames; explicit lifetimes validate within 1–3600 seconds.
Worker authority pins that lifetime and audits it. Idle waits consume the total
monotonic session budget rather than an unrelated ten-second idle cutoff.
After the first frame byte, the remaining prefix/body has a ten-second deadline,
preserving bounded partial-frame handling. Individual RPC deadlines and exchange
ceilings remain independent. Existing database/I/O and cleanup caveats still apply.

Two virtual-time regressions verify idle gaps above ten seconds, session expiry
and incomplete-frame denial with zero backend calls. Bootstrap tests verify the
3600-second setting, invalid limits and a physically absent appended field with
the legacy 300-second default. Tokio virtual-time support is a test dependency.
All 140 tests (137 unit and 3 process integration), mandatory workspace checks,
formatting and Clippy pass with FlatBuffers 25.12.19. Bootstrap tests/Clippy passed
again after the explicit absent-field assertion. Worker crash reconciliation and
general module lifecycle remain incomplete; no live model/GPU claim is added.

## Lease-protected worker audit reconciliation

Persistent worker serving and managed host launch now hold shared runtime leases.
Launch/status checks under that lease refuse terminal tasks. Operator-only
recover-workers requires exclusive ownership, validates open session identities
and appends session_interrupted metadata in one immediate transaction. It never
kills recorded PIDs, replays inference, changes task results or claims an external
server stopped. Closed/reconciled sessions are skipped idempotently. Batches above
10,000 fail before writes, with five-second SQL/progress and loop checks; the
temporary callback is cleared on error/success. SQLite blocking I/O remains soft.

Four unit regressions verify live-worker exclusion until its future drops,
persistent idempotency/task preservation, audit/metadata rollback and an actual
10,001-session refusal without partial changes. Process tests now kill only their
own fixture worker, observe host failure/confirmed reaping, invoke the actual
recover-workers CLI and verify the failed task result remains unchanged. Existing
Ctrl-C cancellation still passes. A terminal-task launch denial is also covered.
All 145 tests (141 unit plus 4 process integration), formatting, workspace check
and Clippy pass with FlatBuffers 25.12.19. No live-model/GPU quality claim is added.
Stale temporary-resource cleanup and generalized executable lifecycle remain
unfinished; recovery remains cooperative with the documented lease-inode caveats.

## Registered expert comparison disclosure (2026-10-11)

Exact flatc/Rust FlatBuffers pairing: 25.12.19. Commands used
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc` and offline Cargo dependencies.

- `cargo fmt --all -- --check`: passed.
- `cargo check --all-targets --offline`: passed.
- `cargo test --all-targets --offline`: 147 unit and 5 process tests passed.
- `cargo clippy --all-targets --offline -- -D warnings`: passed.

New regression exercises all six comparison profiles with a registered expert
whose local provider fails. Only the experts profile discloses the pinned expert
configuration; its checksum and unattested identity are preserved, and the report
flags loss of same-model configuration control. Existing single-model comparison
coverage confirms the ordinary configuration flag. This does not verify loaded
weights, domain quality, GPU integration or a real held-out performance advantage.

## Ranked expert fallback (2026-10-11)

With exact FlatBuffers 25.12.19 and `FLATC` set to the local compiler:
`cargo fmt --all -- --check`, `cargo check --all-targets --offline`,
`cargo test --all-targets --offline` and
`cargo clippy --all-targets --offline -- -D warnings` all passed.
The suite contains 148 unit tests and 5 process integration tests.

The new regression gives a lexically later workflow a higher relevance score,
checks its selection, replaces it with a registered-model version, disables that
model and verifies fallback to the lower-ranked shared-backend workflow. Ordered
admission audits show the unavailable candidate consumed no inference. General
utility estimation and broader candidate scheduling remain incomplete.

## Offline split manifest preflight (2026-10-11)

`python3 -m unittest discover -s training -v`: nine standard-library tests passed.
An actual CLI dry-run using `examples/training_splits.json` validated all three
four-row demo splits, reported `training_started:false` and
`semantic_independence_verified:false`, and created no adapter output.

Exact FlatBuffers 25.12.19 remains in use. Formatting, all-target Cargo check,
all-target Rust tests (148 unit + 5 process) and Clippy with denied warnings passed
with offline dependencies. No real ML training or neural evaluation was run.

The manifest preflight rejects hash changes even when the changed dataset remains
valid, normalized prompt overlap, path traversal, symlinks, unsupported versions,
duplicate/unknown fields and mismatched selected training paths. Only training
rows are returned to optimization; validation/test identity is recorded without
claiming semantic independence or behavioral acceptance. Base-model provenance,
actual GPU training, external isolation and independent neural acceptance remain
release/research limitations, and neural promotion stays denied.

## Structured SQL comparison criterion (2026-10-11)

Using exact FlatBuffers 25.12.19, formatting, all-target Cargo check, all-target
tests (149 unit + 5 process) and Clippy with denied warnings passed offline.
The targeted six-profile SQL comparison regression also passed. Its initial
resource assertion was corrected to account for separately charged reference
search in memory/experts profiles; the runtime accounting was unchanged.

Coverage checks formatting/object-order equivalence, wrong values/types,
duplicate/unknown fields, status exclusion, expected-shape preflight and actual
inline SQLite execution without model calls in tool-enabled profiles. Expected
labels remain operator-supplied; this is structural result scoring, not a new
neural evaluator or proof that supplied labels are correct. Real inference and
the diverse held-out study remain outstanding.

## Provisional corpus and column-less SQL reads (2026-10-11)

Exact flatc/Rust FlatBuffers 25.12.19. Formatting, all-target Cargo check,
all-target tests (151 unit + 5 process) and Clippy with denied warnings passed
using offline dependencies. The new corpus regression validates 100 cases in ten
categories, hashes/IDs/bounds and thirty deterministic arithmetic/SQL/order cases.

The first corpus run exposed a real count(*) failure: the SQLite authorizer
reported Read(items, empty-column) without a database name. The narrow fix permits
that special callback only for declared inline tables. A dedicated regression
retains schema, unknown-table, temp-table and attach denials. SQLite documents
column-less reads at https://www.sqlite.org/c3ref/set_authorizer.html.

The corpus is original synthetic data, not independently reviewed held-out data.
Its manifest explicitly sets independent review, held-out independence and live
model evaluation to false. The other seventy labels need independent review;
the thirty framework-checked cases also need external oracle review. No live
100-case comparison or model-quality/security performance claim was made.

## Optional reasoning admission reservations (2026-10-11)

With exact FlatBuffers 25.12.19, all mandatory checks passed offline:
formatting, all-target Cargo check, all-target tests (152 unit + 5 process),
and Clippy with denied warnings. The new regression covers four combinations of
call, output-token and cumulative input-byte constraints in DEEP/RESEARCH modes.

Decomposition now includes bounded dependency JSON expansion and reserves the
first whole-goal prompt before starting subtasks. Research exploration preserves
one final solver call, 384 output tokens and prompt headroom. Tests verify retained
whole-goal answers, exhausted optional-stage admission, audits and unchanged hard
ceilings. Conservative bounds may skip graphs that short outputs would permit;
this trades optional inference for completion capacity rather than increasing
limits. Wall time/cancellation and model correctness remain separate constraints.
No crash-resumable graph or measured utility policy is claimed.

## Optional operator GPU diagnostics (2026-10-11)

Exact FlatBuffers 25.12.19 remains in use. Formatting, all-target Cargo check,
all-target tests (156 unit + 5 process) and Clippy with denied warnings passed
with offline dependencies. GPU coverage includes strict parser/range/unit/missing
value cases, reasoning-actor denial, and same-binary children that overflow output
or stall until timeout. Cleanup must reap before returning the expected failure.

An actual CLI invocation with an isolated temporary demo workspace and
`diagnostics --probe-gpu` returned unavailable/null device metrics because NVIDIA's
driver query failed. No tasks, inference or GPU setters were requested. The fixed
utility exists here but could not communicate with the NVIDIA driver. Successful
GPU hardware sampling is therefore unverified; fixtures establish parser and
bounded process behavior only. These are whole-device observations, not task
budgets, integrated energy or proof of real model loading/inference.

NVIDIA query interface: https://docs.nvidia.com/deploy/nvidia-smi/index.html.

## Registered endpoint diagnostics and live availability (2026-10-11)

Exact FlatBuffers 25.12.19. Formatting, all-target Cargo check, all-target tests
(158 unit + 5 process) and Clippy with denied warnings passed offline.
New loopback fixtures verify endpoint deduplication, exact registration metadata,
missing model IDs, disabled-model exclusion, empty registry and eight-endpoint
preflight admission. Existing discovery fixtures cover redirects, malformed data,
response bounds and missing primary/role model IDs.

An actual CLI smoke registered explicitly labelled endpoint-unavailable fixture
metadata in a temporary workspace, then ran diagnostics --probe-registered-models.
It printed status failed and exited 1, with generation_tested false. No model
weights were loaded or represented as real inference.

The installed LM Studio CLI and two existing GGUF files (approximately 16 and
21 GB) were observed. Server status reported not running. Both lms ps --json and
lms ls --json ended with timeout waiting for daemon/API startup. No large model
was loaded, no resident user model was unloaded, and no new weights were downloaded.
This refines the blocker: local files exist, but a usable inference service and
resource-appropriate live generation remain unverified. CLI documentation used
https://lmstudio.ai/docs/cli and https://lmstudio.ai/docs/cli/local-models/load;
model filename/proximity alone does not establish licensing or weight identity.

## Reviewed dataset pins and host integration probe (2026-10-11)

Exact FlatBuffers 25.12.19. Formatting, all-target Cargo check, all-target tests
(159 unit + 5 process) and Clippy with denied warnings passed offline. The new
regression verifies an approved byte hash, malformed/mismatching pin refusal and
whitespace-only source changes without additional tasks. An actual negative CLI
comparison exited 1 with zero tasks for a mismatching --dataset-sha256.

The default endpoint still refused connections in an explicit host-access curl
probe. However, host-access lms ps subsequently succeeded with an empty model list,
refining earlier sandboxed daemon timeouts. Host NVIDIA queries and the actual
framework diagnostics --probe-gpu then succeeded: total memory 12,884,901,888 bytes,
used 3,182,428,160 bytes, utilization 18%, reported power 44.78 W in that sample.
These are whole-device observations, not model/task attribution or energy.

LM Studio estimated 6.24 GiB GPU / 17.54 GiB total for the existing 27B Q4_K_M
model at 35% offload, context 2,048 and one parallel prediction. A temporary unique
identifier cognitive-fabric-live-validation loaded successfully; LM Studio reported
29.68 s load time and 15.66 GiB size. These are provider CLI observations, not a
framework model-load metric or attested weights. The server was started on loopback
port 1234 only, with no CORS change or credential access.

Actual framework discovery failed its HTTP status check; a FAST generation request
returned HTTP 401 Unauthorized. Task 40f0f62d-6397-4b90-a3b3-f53ad06f9b5f failed
honestly with one charged call, 384 reserved output tokens and unknown token usage.
No model output or performance advantage was fabricated. Authentication was not
weakened and no token was searched. The owned identifier was explicitly unloaded,
the server restored to its prior stopped state, and lms ps returned []. Explicit
protected operator-supplied bearer authentication is the next integration gap.

Only temporary framework workspaces were configured. No weights were downloaded,
modified or committed. No independent held-out study or successful real inference
was completed. Earlier statements about unavailable hardware/services reflect
sandboxed observations and are superseded by this more precise host evidence.

## Scoped inference authentication verification

With FlatBuffers compiler/Rust pairing 25.12.19, the following checks passed:
`cargo fmt --all -- --check`, `cargo check --all-targets --offline`,
`cargo test --all-targets --offline`, and
`cargo clippy --all-targets --offline -- -D warnings`.
The suite contains 166 tests: 160 library tests, one authentication integration
and five managed-process integrations. Network/process tests ran with host access.
The authentication integration exercises four direct/managed and safe/escaped-echo
combinations using synthetic credentials; scope mismatch refuses credential reads
in a pure unit test. Bootstrap roundtrip preserves scope and absent legacy fields
remain unauthenticated. No real operator credential was read or live authenticated
generation claimed. Full discovery authentication and cross-endpoint live behavior
still need dedicated integration coverage beyond the shared credential helper.

## Authentication boundary integration follow-up

Dedicated CLI integration now covers both primary and registered-model discovery:
matching scopes send the synthetic bearer, other scopes send no Authorization
header, and escaped provider echoes cause a failed diagnostic without disclosing
the token. Direct and managed tasks with a missing dedicated credential fail before
any socket connection. Models cannot authorize authentication configuration.
This closes the dedicated discovery-coverage gap above; live authenticated LM
Studio generation remains unverified.

All mandatory checks passed again with FlatBuffers 25.12.19: formatting, all-target
check, all-target tests (168: 160 library, three authentication, five process), and
Clippy with warnings denied. Network/process tests used host access; all credentials
were synthetic and explicitly supplied or removed on child command environments.

## Paired benchmark change reporting

All mandatory checks passed with FlatBuffers 25.12.19: formatting, all-target check,
all-target test (169 tests: 161 library, three authentication, five process), and
Clippy with warnings denied. Tests cover improvement/regression/both-pass/both-fail,
missing baseline/arm outcomes, zero denominators, and actual six-arm SQLite results.
Aggregation introduces no model calls or permissions. Optional larger baselines
use the same paired machinery; no live performance measurement is inferred.

## Configuration snapshot and recovery

FlatBuffers 25.12.19 mandatory checks passed: fmt check, all-target check, all-target
171 tests (162 library, three authentication, five managed process, one restore
CLI), and Clippy with warnings denied. A final assertion-only path check was also
rerun through the restore CLI test and Clippy. Tests verify configuration opt-in,
private permissions, source-home exclusion/new-home binding, invalid settings and
missing metadata before publication, legacy database-only restoration, checksum
integrity, and actual CLI backup/restore without initializing an unused source.
The source database is not modified by snapshot metadata insertion. External
artifact/config lifecycle portability remains partial; weights are never copied.

## RAG artifact recovery verification

With FlatBuffers compiler/Rust 25.12.19, formatting, all-target check, all-target
172 tests (163 library, three authentication, five managed process, one restore
CLI) and Clippy with denied warnings pass. Archive tests cover RAG roundtrip,
read-only permissions, canonical candidate paths, unchanged candidate status,
changed restored/source hashes, tampered/missing/oversized payloads, malicious
candidate triggers, and payload omission on default re-backup. The CLI integration
exercises both explicit options alongside configuration recovery. Full tests use
host access for sockets/processes. Neural files and arbitrary external archives
are excluded; no automatic promotion, weight safety or full restoration is claimed.

## Explicit schema migration restore verification

FlatBuffers 25.12.19 checks pass: formatting, all-target check, full all-target
173 tests (164 library, three authentication, five process, one restore CLI), and
Clippy with warnings denied. Fixtures cover source schemas 1–5, byte-preserved
originals, current standalone output, running tasks without replay, default refusal
and conflicting migration failure before publication. CLI flag coverage passes.
The final diagnostic wording change is additionally checked by restore tests and
Clippy. Future/unversioned schemas remain unsupported, and tests do not establish
validity of arbitrary custom databases solely from their version number.

## Host module progress routing

All mandatory checks pass with FlatBuffers 25.12.19: fmt check, all-target check,
all-target tests (176: 167 library, three authentication, five managed process,
one restore CLI), and Clippy with denied warnings. New tests exercise persisted
milestones with pinned module replacement/unregistration, task/stage isolation,
content minimization, descriptor denials, immutable registry history/capacity and
shared subscriber overflow/admission. Existing socket/process fixtures pass with
host access. The feature is an in-process host routing API; authenticated external
module provisioning and independent executable handlers remain unverified/absent.

## Observer cancellation and pinned replay

FlatBuffers 25.12.19 mandatory checks pass: fmt check, all-target check, all-target
178 tests (169 library, three authentication, five process, one restore CLI), and
Clippy with warnings denied. Tests cover cancellation while pending and buffered,
ready-cancel priority, post-cancel closure/capacity release, unchanged running-task
state, durable audit preservation, overflow replay and pinned filter pagination
through empty pages after module replacement. Socket/process fixtures ran with
host access. Replay remains a trusted-host API and does not authenticate external
modules or implement automatic task execution resumption.

## Bounded audit trace verification

FlatBuffers 25.12.19 mandatory checks pass: fmt check, all-target check, all-target
182 tests (172 library, three authentication, five process, one restore CLI,
one audit CLI), and Clippy with denied warnings. New regressions cover 32-event
pages, interleaved task isolation/order, empty/unknown/overflowing cursors, 4,097
full-trace records, raw-byte exhaustion, oversized payloads and invalid JSON.
The actual CLI fixture follows a cursor through 35 complete audit events. Existing
benchmark model/expert identity fixtures still pass after selected-record reads.
Bounds describe raw data and rows; JSON overhead and SQLite blocking remain distinct
from hard process/disk quotas. No audits or execution outcomes are modified.

## Durable inference audit verification

With FlatBuffers 25.12.19, formatting, all-target check, full all-target tests
(185: 175 library, three authentication, five process, one restore CLI, one audit
CLI) and Clippy with denied warnings pass. Final extended audit CLI coverage and
Clippy also pass. Regressions cover actual runtime call IDs, complete provider
usage, pending/legacy unknown totals, duplicate/unmatched identities, malformed
call IDs, forged content hashes and output over-allocation. The CLI fixture runs
demo reasoning then inspects its ledger without claiming real provider usage.
No automatic replay, weight attestation or task resumption is established.

## Durable task settings verification

FlatBuffers 25.12.19 mandatory checks pass: fmt check, all-target check, all-target
188 tests (178 library, three authentication, five process, one restore CLI,
one audit CLI), and Clippy with denied warnings. Tests cover invalid embedding
configuration without task creation, original settings retained after engine
changes, snapshots preceding dispatch, excluded workspace paths and injected
snapshot write failure with zero dispatches. No backend attestation or automatic
resumption is inferred; initialization failure can require explicit task recovery.

## Interrupted-task reconciliation bounds

FlatBuffers 25.12.19 formatting, all-target check, all-target tests (190: 180
library and 10 integration), and Clippy with denied warnings pass. New negative
coverage rejects malformed/oversized task identities and 10,001-task batches
without partial writes. An expensive SQLite trigger is interrupted by the
five-second progress callback; the test verifies rollback, callback removal and
successful subsequent recovery. The deadline bounds SQLite computation and
between-record processing, not filesystem I/O, fsync or busy waits. Reconciliation
remains operator-only, lease-protected and does not replay inference.

## Durable advisory graph inspection

FlatBuffers 25.12.19 formatting, all-target check, 192 all-target tests (182
library and 10 integration) and Clippy with denied warnings pass. Graph auditing
is exercised against actual scheduler-generated events plus partial histories,
unrelated-task isolation, dependency-order violations, invalid/duplicate
transitions, oversized results, repeated plans and changed node definitions.
The CLI integration checks inspection of a task with no graph. No automatic
continuation, independent verification of advisory answers, or restored model
authority is claimed. Inspection is bounded and excludes objective/result bodies.

## Original inference admission reconstruction

FlatBuffers 25.12.19 formatting, all-target check, 194 all-target tests (184
library and 10 integration), and Clippy with denied warnings pass. New tests
verify that unmatched calls retain reserved charges and reject mode mismatch,
late/duplicate snapshots, uncorrelated legacy results, and original call/output/
prompt budget overdraws. Actual runtime and CLI reports include remaining call
admission. Missing snapshots retain null admission; other resource allowances
remain unavailable rather than inferred. No replay, permission restoration or
checkpoint execution is established.

## Durable tool and cycle admission

FlatBuffers 25.12.19 formatting, all-target check, 197 all-target tests (187
library and 10 integration), and Clippy with denied warnings pass. Regression
coverage proves reservation-write failures prevent tool admission and solver
calls, denied tools do not consume invocations, repeated solve calls cannot
restart the session cycle budget, and durable auditing rejects forged sources,
capabilities, numbering, incompatible settings and cycle overdraw. Actual runtime
cycles are reconstructed. Wall time remains unavailable and no continuation or
external-operation completion is inferred from reservation records.

## Shared monotonic task deadline

FlatBuffers 25.12.19 formatting, all-target check, 199 all-target tests (189
library and 10 integration), and Clippy with denied warnings pass. A slow-pinning
backend fixture consumes the full one-second setup budget and verifies exhausted
status with no tool, cycle or model dispatch. An expired standalone Session
rejects all three admissions, and a later attempted deadline extension has no
effect. Request timeouts are capped by remaining monotonic allowance. Synchronous
callbacks/I/O are not preemptible, cleanup may exceed execution time, and no
cross-process clock restoration or automatic resumption is claimed.

## Model registry read integrity

FlatBuffers 25.12.19 formatting, all-target check, 200 all-target tests (190
library and 10 integration), and Clippy with denied warnings pass. Six corruption
fixtures reject embedded ID/version changes with valid recomputed hashes, oversized
manifest/checksum fields and oversized row IDs/versions through both listing and
exact lookup. SQL field guards prevent oversized values from being copied into
Rust; registry count bounds remain unchanged. No loaded-weight attestation or
trust against an owner rewriting both data and hashes is implied.

## Independent integer-sort verifier

FlatBuffers 25.12.19 formatting, all-target check, 202 all-target tests (192
library and 10 integration), and Clippy with denied warnings pass. Tests cover
signed integer extremes, duplicate preservation, ascending/descending direction,
float/overflow and unexpected metadata rejection, 256-value bounds, scripted
DEEP revision after a failed check, FAST rejection and denied verification when
tool allowance is exhausted. Runtime evidence names integer_sort/v1 rather than
mislabeling it dependency_order/v1. The checker is a pure bounded Rust capability
with no host file/network access; scripted reasoning does not establish live-model
quality or general independent verification.

## Fresh structured-memory verification

FlatBuffers 25.12.19 formatting, all-target check, 203 all-target tests (193
library and 10 integration), and Clippy with denied warnings pass. Runtime tests
cover zero-inference completion after rechecking an unverified published sorting
answer, rejection of wrong/revoked/nonmatching answers, denied tool allowance,
Full/Memory support and Alone profile isolation. The CLI integration publishes an
episode and confirms fresh verified completion with zero model calls. Evidence is
current deterministic checking, not the memory's historical status. No general
fact trust, live reasoning quality or benchmark advantage is inferred.

## Durable assembled reasoning context

FlatBuffers 25.12.19 formatting, all-target check, 206 all-target tests (196
library and 10 integration), and Clippy with denied warnings pass. Tests verify
original retrieved context/hash persistence after document changes, snapshot
ordering before solver dispatch, no inference after injected snapshot-write
failure, and rejection of oversized objectives/contexts or JSON-escaped payloads
exceeding the audit ceiling. Snapshots remain private untrusted task data; they
do not attest source authenticity or authorize replay, and no automatic checkpoint
continuation or cross-process wall allowance is established.

## Independently checked saved-result recovery

FlatBuffers 25.12.19 formatting, all-target check, 210 all-target tests (199
library and 11 integration), and Clippy with denied warnings pass. Tests exercise
exclusive-lease denial, abandoned running/interrupted completion, fresh sorting
and ordering checks, unchanged inference charges, reopen durability, terminal and
model-actor denials, incorrect answers, forged result hashes, genuinely unresolved
reservations, missing tool allowance, unsupported objectives and transactional
rollback after injected audit failure. Actual CLI recovery completes a saved
sorting result without inference. Incorrect checks retain their charge; provider
usage remains unknown where absent and whole-resource completeness stays false.
This recovers provable terminal answers, not arbitrary subtask graphs or missing
results, and grants no restarted inference or restored wall allowance.

## Versioned compiled checker experts

FlatBuffers 25.12.19 formatting, all-target check, 212 all-target tests (201
library and 11 integration), and Clippy with denied warnings pass. Tests exercise
supported checker discovery/health, rejected unknown/missing/workflow/reference
bindings, model-actor registration denial, preserved legacy hashes, replacement
with historical proof pinning, disable fallback and actual reasoning/memory proof
identities. Compiled checker implementations remain fixed Rust code; manifest
versioning does not attest neural weights or grant permissions. Dynamic imported
executable plugins and broader independent module authentication remain absent.

### Offline security evaluation verification

The `security-eval` addition passes formatting, all-target checking, all-target
tests (202 library and 12 integration tests; 214 total) and Clippy with warnings
denied. Checks used offline dependencies and FlatBuffers compiler 25.12.19
paired with Rust FlatBuffers 25.12.19. The actual CLI produces identical reports
across repeated runs. Its 24 fixed probes are boundary regressions, not a general
security failure estimate or evidence of neural safety. Live inference and broader
adversarial evaluation remain release work.

### Release checklist reconciliation

Reviewed all 18 goal acceptance criteria against current transport, model, worker,
recovery, evaluation and security documentation and source locations. The release
audit preserves unverified live-generation, lifecycle, installation and benchmark
gates. Ran the actual CI security command locally: 24/24 probes passed, 19 expected
denials, zero unexpected accepts. All nine offline training preflight tests passed;
no ML training or neural safety evaluation was performed. CI execution itself has
not been observed. This documentation/workflow update changes no runtime code.

## Broker shutdown lifecycle

Targeted tests cover cancelling an active retired module version, discarding queued
work, permanent admission refusal, idempotence and stopping before lazy queue
initialization. Existing replacement tests preserve normal task pinning semantics.
The host stop operation grants no model authority and does not assert external
server termination or joined worker-loop completion.

All mandatory checks pass for this change: formatting, all-target check, 216 tests
(204 library, 12 integration), and Clippy with warnings denied. Commands used
offline dependencies and the exact FlatBuffers 25.12.19 compiler/crate pairing.
No per-request threads, new queues or inference calls are introduced; each broker
adds one shared stop signal and workers check it before waiting/dispatching.

## Service broker shutdown integration

Formatting, all-target check, all 216 tests (204 library, 12 integration) and
Clippy with warnings denied pass with compiler/crate FlatBuffers 25.12.19 and
offline dependencies. Extended real socket tests verify retained pin refusal
after typed STOP, broker cleanup following host shutdown and disconnect/task
cancellation, and preserved durable cancelled status. Broker shutdown occurs
after handler joins so normal service cancellation keeps precedence over module
errors. No additional inference, queues, model privileges or external server
termination claims are introduced. General module routing remains incomplete.

## Native residency inventory

Formatting, all-target check, 219 tests (206 library, 13 integration) and Clippy
with denied warnings pass offline with FlatBuffers compiler/crate 25.12.19.
Actual CLI HTTP fixtures cover authenticated/unauthenticated inventories, separate
credential selection, escaped token echoes, redirects, response-size bounds and
malformed JSON. Unit tests cover duplicate model/instance IDs, unsupported types,
invalid context capacities and native endpoint restrictions. Observations are
server-reported only; real authenticated LM Studio inventory, physical lifecycle
operations and automatic model switching remain unverified/incomplete. The
read-only command performs one bounded request and adds no runtime inference cost.

## Explicit native residency lifecycle

Formatting, all-target checking, 221 tests (207 library, 14 integration) and
Clippy with warnings denied pass offline with FlatBuffers compiler/crate 25.12.19.
CLI HTTP fixtures execute load and unload with durable pre-POST intent and fresh
inventory confirmation. Negative fixtures cover existing loaded-model refusal,
malformed load outcomes retained as interrupted, and injected intent-write failure
preventing POST. Unit tests verify approval/context bounds, shared-lease exclusion,
zero task creation on rejected preflight and model-actor permission denial.
No real weights were loaded by these tests. Actual host lifecycle and generation,
cross-client instance ownership and automatic switching remain unverified.
Mutation requests are bounded to 60 seconds; no automatic retries or inference
calls are added. Missing outcomes cannot safely trigger automatic cleanup.

## Atomic residency completion

Formatting, all-target check, 222 tests (208 library, 14 integration) and Clippy
with denied warnings pass offline with FlatBuffers compiler/crate 25.12.19.
Fault injection verifies that both audit-insert and task-update failures roll back
the entire local completion transaction. Subsequent unknown-outcome finalization
works, while missing and already-terminal anchors fail without extra evidence.
Existing actual CLI load/unload and credential tests still pass. Completion
metadata is capped at 4 KiB; no network calls, retries or model authority are
added. External side effects remain outside SQLite rollback and real host
validation remains outstanding.

## Isolated optimized installation and CLI walkthrough

`cargo install --offline --locked --path . --root /tmp/cogfabric-v1-install-audit-20261011`
passed with FLATC 25.12.19, producing an optimized installed binary from `abb23c1`.
The build used cached dependencies and took 3m21s; this is build timing, not task
performance. `bash -n scripts/installed-smoke.sh` passed. The workflow passed both
with the development binary and with the installed release binary in separate new
state directories. Explicit JSON assertions verify returned behavior, not merely
exit status. Evidence/source identity is recorded in `RELEASE_AUDIT.md`.

Runtime Rust code is unchanged since the 222-test mandatory check run. This
change adds reproducible installation/workflow verification and CI configuration;
remote CI and a fresh OS/package-manager installation were not executed. Ten
arithmetic examples all passed deterministically, which proves no live LLM quality,
held-out benchmark improvement or neural safety. The service is stopped by the
script and its socket removal is checked; retained temporary files contain only
synthetic demonstration data.

## Per-subtask resource observations

Formatting, all-target check, 223 tests (209 library, 14 integration) and Clippy
with warnings denied pass offline with FlatBuffers compiler/crate 25.12.19.
The dependency fixture verifies two distinct one-call/192-token reservations,
provider deltas and measured timings without exposing result bodies. Budget-skip
fixtures retain zero charges and unavailable timing; cancellation retains one
reserved incomplete call and null provider usage. Injected cost-audit failure
prevents the next advisory dispatch. Existing graph reconstruction still validates
the scheduler's node histories; cost records do not alter recovery budgets.

Instrumentation adds at most four small terminal-node metadata events and one
active counter/timestamp snapshot per plan; no new model calls, concurrency or
privileges are introduced. General module scheduling and measured utility-based
escalation remain incomplete, and no live-model efficiency gain is inferred.

## Boot-bound recovery clock evidence

Formatting, all-target checking, 227 tests (212 library, 15 integration) and
Clippy with warnings denied pass offline with FlatBuffers compiler/crate 25.12.19.
Tests cover actual bounded kernel clock reads, synthetic expiration/backwards time,
malformed clock/version/allowance/UUID/field denials, different-boot null allowance,
and separate real CLI processes observing decreasing original remaining time.
The existing slow-setup regression now proves zero recovered wall allowance after
setup exhausts a one-second task. Legacy snapshots preserve null wall evidence.

Engine captures once before setup; Session receives that start rather than making
a fresh deadline. Optional Linux clock evidence adds one bounded fixed-path boot
UUID read and kernel clock observation at task start/audit, without network or
model calls. It is not attested time, portable restoration across boots or replay
authority. Resume authorization remains false and graph continuation is incomplete.

## Explicit advisory continuation (2026-10-11)

The all-target offline suite passed 232 tests (217 library, 15 integration) with
FlatBuffers compiler/crate 25.12.19. Five new continuation tests exercise pending
node reuse, real local HTTP responses, checkpoint corruption, original deadlines,
unsupported configuration, cancellation, exclusive leases and terminal audit
rollback. After tightening the terminal running-task anchor, all five continuation
tests passed again. Formatting, all-target checking and Clippy with warnings denied
passed. HTTP results are deterministic fixtures; no live-model quality is claimed.
General task/backend/phase resumption and release readiness remain incomplete.


## Registered direct model continuation (2026-10-11)

`cargo fmt --all -- --check`, offline all-target check, all-target Clippy with
warnings denied and the full offline all-target test suite passed. The suite now
contains 234 passing tests (219 library, 15 integration). New tests cover original
registered model HTTP selection and six invalid registry/snapshot cases. These
fixtures do not establish loaded-weight attestation or live-model quality. Exact
FlatBuffers compiler/crate pairing remains 25.12.19. Managed/custom backend and
later-phase recovery remain open.


## Managed advisory continuation (2026-10-11)

Formatting, offline all-target checking, all-target Clippy with warnings denied and
all-target tests passed using exact FlatBuffers compiler/crate 25.12.19. There are
237 passing tests (220 library, 17 integration). The new lease test verifies worker
shared access and recovery refusal. Actual CLI integration tests run a fresh child
worker through authenticated typed IPC and fixture HTTP, verifying successful
continuation and cancellation with child reaping and unresolved call reservations.
No external GPU shutdown, real-model quality or general-phase recovery is claimed.

## Durable solver-boundary recovery (2026-10-11)

Formatting, offline all-target checking, Clippy with warnings denied and all-target
regressions passed with FlatBuffers compiler/crate 25.12.19. The suite has 241
passing tests (224 library, 17 integration). New checks cover boundary persistence
before reservation, exact failed-answer/revision state, altered/stale boundaries,
repeat detection after restoration and real HTTP continuation into independent
verification. An initial restricted run could not bind fixture sockets; the full
suite subsequently passed with local socket permission. No general within-cycle,
no-graph or repeated recovery, model-quality gain or release completion is claimed.

## Whole-goal boundary recovery (2026-10-11)

Formatting, offline all-target checking, Clippy with warnings denied and all-target
tests passed with FlatBuffers compiler/crate 25.12.19: 244 tests (227 library,
17 integration). New coverage verifies real HTTP revision without a graph, FAST
admission with zero prior inference, explicitly budget-skipped plans, refusal of
missing valid graphs and consumed boundaries. Original counters and stopping
state remain enforced; no new inference or authority is granted by absent plans.
General repeated and within-cycle recovery remain unfinished.

## Repeated solver-boundary recovery (2026-10-11)

Formatting, offline all-target checking, Clippy with warnings denied and all-target
regressions passed with FlatBuffers compiler/crate 25.12.19: 246 tests (229 library,
17 integration). New tests restore two successive boundaries while retaining one
settings record and the original call/cycle ledger. Changed context bytes and
chronological counters, exhausted cycles and more than 17 assemblies are refused.
Existing HTTP/process tests also pass. This validates saved-boundary repetition,
not recovery inside consumed cycles or external inference termination.

## Failed saved-answer reconciliation (2026-10-11)

Formatting, offline all-target checking, Clippy with warnings denied and all-target
tests passed with FlatBuffers compiler/crate 25.12.19: 250 tests (233 library,
17 integration). New tests reconcile interruption after a recorded failed solver,
create a freshly checked revision without repeated inference, execute it through
HTTP, refuse repeat-stopped answers and roll back tool/proof writes when boundary
persistence fails. Unknown provider outcomes and arbitrary within-cycle phases
remain unsupported; this supplies no model-quality or release-readiness claim.

## Authenticated process event modules (2026-10-11)

Formatting, offline all-target checking, all-target Clippy with warnings denied and
all-target tests passed with FlatBuffers compiler/crate 25.12.19: 258 tests
(241 library, 17 integration). Eight new tests include a separate child process,
mutual UID/PID checks, immutable grant/version scope, unknown protocol shapes,
malformed frames, task/stage denials, overflow, observer cancellation and virtual
session/partial-frame deadlines. COGS schema generation uses the same exact compiler
pairing. Delivery remains a trusted host observer API; no general module command
execution, automatic provisioning, executable attestation or live-model gain is
claimed. Existing inference, service, persistence and recovery regressions pass.

## Durable authenticated module replay (2026-10-11)

Formatting, offline all-target checking, Clippy with warnings denied and all-target
tests passed with FlatBuffers compiler/crate 25.12.19: 263 tests (246 library,
17 integration). Five new tests cover filtered replay/cursor advancement,
reconnect after unregistration with the original grant, live overlap deduplication,
v1/v2 compatibility, cursor/version denials and cursor-only frame/session limits.
The existing separate-process event fixture and complete inference/recovery suite
also pass. Schema changes append fields while v1 emission remains unchanged;
replay explicitly selects v2. Automatic provisioning and general module command
transport remain incomplete; no exactly-once or live-model claim is made.


### Typed arithmetic routing — 2026-10-11

Using FlatBuffers compiler/crate 25.12.19, formatting, offline all-target check,
all-target tests (268 passed: 251 library and 17 integration) and Clippy with
warnings denied passed. Full tests included local sockets and child processes.
Five new tests cover immutable pins/replacement, task actor/input denials,
malformed packets, independent rejection of wrong backend output, zero tool
allowance and active/buffered shutdown. No live model was invoked. General process
tool transport and model effectiveness remain unverified/incomplete.


### Authenticated arithmetic process commands — 2026-10-11

Mandatory formatting, offline all-target check, all-target tests (278 passed:
261 library and 17 integration) and Clippy with warnings denied passed with exact
FlatBuffers compiler/crate 25.12.19. Ten new test entries include the child fixture
and real separate-process arithmetic, broker reuse/call limits, peer/task denials,
forged responses/frame lengths, cancellation, partial-frame/session deadlines,
duplicate-correlation denial and authorization-audit failure before backend work.
Registration also rejects a descriptor differing from its socket backend grant.
No live inference or arbitrary executable sandbox was tested. Automatic tool-worker
launch, task-grant provisioning and service/CLI integration remain incomplete.


### Managed arithmetic task workers — 2026-10-11

Mandatory formatting, offline all-target check, all-target tests (285 passed:
266 library and 19 integration), and Clippy with warnings denied passed with
FlatBuffers compiler/crate 25.12.19. New checks cover typed bootstrap bounds and
built-in grant constraints, terminal/wrong-task refusal, custom registry refusal,
forced reaping/socket removal for unfinished startup, and actual CLI/service asks.
Integration tests assert zero inference calls, one audited arithmetic authorization,
confirmed reaping, rejected-expression cleanup, and no launch when the math-tool
allowance is exhausted after retrieval. The startup child fixture is included in
the count. No live inference or sandbox isolation is established by these tests.


### Typed SQL capability routing — 2026-10-11

Formatting, offline all-target check, all-target tests (292 passed: 272 library,
20 integration) and Clippy with warnings denied passed with FlatBuffers
compiler/crate 25.12.19. Six new library tests cover typed scalar round trips,
malformed/mixed frames, aliased decoded-size exhaustion, separate capability pins,
replacement/unregistration/shutdown, denied actors, wrong/malformed answers,
actual tool-budget refusal, unsafe SQL rejection and pending-future cancellation.
A new real CLI test verifies SQL routing with both process settings enabled and
no arithmetic/inference worker launch. The initial unprivileged broad tool test
run could not open local sockets; the final full run used the required socket
permissions and passed. SQL execution/checking is in-process; live inference,
SQL process provisioning and arbitrary executable isolation remain unverified.


### Current installed tool workflow verification — 2026-10-11

The optimized executable built from Rust source commit
`69190875aef58b821e6daa37e9dbd887698bba64` was installed with
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-tool-install-audit-20261011`.
Rust/Cargo 1.97.1, FlatBuffers 25.12.19 and cached locked dependencies were used.
The release build completed successfully (45.05 seconds reported by Cargo).
Executable SHA-256:
`4b0145d6323a9e9b81ddf342e535dfaa10f709bdc638d69bf6428bd56d176e42`.
Only smoke-script/documentation changes followed that Rust source commit.

The expanded `scripts/installed-smoke.sh` passed against that installed executable
in a new temporary workspace. It retains executable/script hashes and version
metadata along with JSON task traces. In addition to the earlier offline workflow,
it verifies one-call arithmetic worker launch/authorization/confirmed reaping,
cleanup after rejected arithmetic, SQL capability dispatch with no child launch,
restored worker configuration and actual worker execution after database/RAG
restore, and both arithmetic and SQL socket-service asks. All six new execution
cases used zero model calls. The rejected arithmetic case exited with status 1;
service STOP succeeded and removed the socket. Final local evidence is retained
at `/tmp/cogfabric-installed-smoke.Q1YToY`.

Mandatory formatting, offline all-target check/tests (292 passed: 272 library,
20 integration) and Clippy with warnings denied also passed. This supersedes the
older installed source evidence for tool routing. It does not establish clean-OS
prerequisite setup, real inference/authentication, model switching, independent
held-out results, executable sandboxing or neural safety. Future runtime changes
must repeat installation verification before release; the project remains alpha.

### SQL Unix transport verification (2026-10-11)

Formatting, offline all-target check, all-target tests and Clippy with warnings
denied pass with FlatBuffers compiler/crate 25.12.19. Tests total 301 (281 library,
20 integration); the nine new focused transport tests also passed separately.
Full output: `/tmp/cogfabric-sql-unix-gate.log`. Actual child-process SQLite and
negative identity, replay, budget, framing, cancellation and audit-failure cases
are covered. Installed evidence for source `6919087` predates this runtime change;
repeat installation verification before release. Automatic SQL launch/reaping,
real authenticated model inference and final release gates remain incomplete.

A fresh optimized installation of `af92168` also passed the complete installed
smoke script. Exact binary/script hashes and evidence directory are recorded in
`RELEASE_AUDIT.md`; this closes the installed-source gap noted above for this
commit. The script checks existing CLI/service behavior, while SQL Unix transport
is covered by the actual child-process Rust test rather than automatic CLI launch.

### Managed SQL worker integration (2026-10-11)

All mandatory checks pass: formatting, offline all-target check/tests and Clippy
with warnings denied. Tests total 304 (283 library, 21 integration), including
SQL CLI/service success, unsafe-query cleanup and budget refusal without launch.
Full test log: `/tmp/cogfabric-managed-sql-gate.log`; focused tool-worker log:
`/tmp/cogfabric-managed-sql-tests.log`. FlatBuffers compiler/crate remains 25.12.19.
The expanded smoke script passed against the current debug executable, including
SQL worker configuration, rejected-query reaping, restored SQL configuration and
socket SQL execution. Optimized installed verification must follow this source
change before release. No live model, sandbox or automatic crash cleanup is proven.

The optimized installation of managed SQL source `04a3a03` passed the expanded
installed smoke script. SQL processes execute from both CLI/service, rejected
queries are reaped and restored configuration launches functioning SQL workers.
See `RELEASE_AUDIT.md` for exact build identity, hashes and retained evidence.
This closes the pending optimized-install gap for this source; other release
gates remain open.

### Inference startup ownership (2026-10-11)

Formatting, offline all-target checking/tests and Clippy with warnings denied
pass. Tests total 306 (285 library, 21 integration), using FlatBuffers 25.12.19.
Focused real-child tests verify cancellation and owned-PID audit failure before
handoff, second-launch refusal, forced reap and socket cleanup. Existing CLI/HTTP
worker regressions also pass. Full log:
`/tmp/cogfabric-inference-startup-gate.log`; focused log:
`/tmp/cogfabric-inference-startup-tests.log`. The expanded smoke script passes
against the current debug executable and now checks demo inference launch intent,
owned PID, completed handoff and matching reaped PID. Optimized installed
verification must follow this runtime change. Host crashes and actual authenticated
model behavior remain unverified.

Fresh optimized installation of `fcd6465` passed the expanded installed smoke
workflow, including demo inference launch/ownership/handoff/matching reap.
`RELEASE_AUDIT.md` records exact source/binary/script hashes and retained evidence.
The pending installed-source gap above is closed for this commit; cancellation
and audit-failure startup coverage comes from the real-child Rust regressions.
No real model or host-crash behavior is inferred from the demo workflow.

### Mixed tool/inference worker recovery (2026-10-11)

All mandatory checks pass: formatting, offline all-target check/tests and Clippy
with warnings denied, using FlatBuffers 25.12.19. Tests total 309 (288 library,
21 integration). Focused recovery tests passed, including mixed idempotent repair,
closed-session exclusion, unchanged task answers/charge events, transactional
rollback for invalid capabilities/charges/formats/oversized metadata and audit
failure, and live arithmetic/SQL socket lease exclusion. Full log:
`/tmp/cogfabric-tool-recovery-gate.log`. The expanded smoke workflow passes against
the current debug executable and verifies recovery remains empty after normal
worker completion. Optimized installed verification is pending for this source.
No tool replay, PID termination, file cleanup or external-effect proof is inferred.

Optimized installation of mixed recovery source `dbb9c47` passed the expanded
installed workflow, including empty recovery after clean tool/inference completion.
Exact source/binary/script identity and retained evidence are recorded in
`RELEASE_AUDIT.md`. The installed-source gap above is closed for this commit;
abandoned/malformed/live-session proof comes from the recovery regressions, not
a simulated host crash in the smoke script.

### Journaled worker-file cleanup validation (2026-10-12)

Rust/Cargo 1.97.1 and freshly rebuilt FlatBuffers compiler 25.12.19 pass formatting,
offline all-target check/tests and Clippy with warnings denied. Tests total 318
(297 library, 21 integration). Nine cleanup tests cover approved/idempotent removal,
workspace/plan/lease binding, symlink/type/mode/extra-file refusal, changed inodes,
missing-socket re-review, audit failures, invalid/oversized/duplicate journals,
pagination and actual child-process death. Full log:
`/tmp/cogfabric-worker-files-gate.log`. The expanded debug smoke script also passes
preview/apply of an empty plan after clean worker completion. Optimized installed
validation is pending for this source. No automatic cleanup, unjournaled sweep or
hostile same-user race isolation is claimed.

Optimized installation of `054505c` passed the expanded installed workflow,
including empty cleanup preview/apply commands. Exact build identity and hashes
are recorded in `RELEASE_AUDIT.md`; actual stale-file deletion after child-process
death is covered separately by the Rust regression. The pending installed-source
gap above is closed for this source. Unjournaled resources, automatic cleanup,
hostile-owner race isolation and live model behavior remain outside the proof.

Tool rollback validation (2026-10-12): `cargo fmt --all -- --check`,
`FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo check --offline --all-targets`,
`cargo test --offline --all-targets` and `cargo clippy --offline --all-targets -- -D warnings`
(with the same FLATC setting) pass. Tests: 300 library + 21 integration = 321.
Log: `/tmp/cogfabric-tool-rollback-full-tests.log`. Coverage includes retained
math/SQL pins, capability identity after unregistration, conflict refusal,
shutdown and full-history rollback. Real inference observations are recorded
separately in `LIVE_INFERENCE.md`; these do not constitute benchmark results.

Inference rollback validation (2026-10-12): formatting, offline all-target check,
all-target tests (303 library + 21 integration = 324) and Clippy with warnings
denied pass using `FLATC=/tmp/cogfabric-flatbuffers-build/flatc` (25.12.19).
Log: `/tmp/cogfabric-inference-rollback-full-tests.log`. Three regressions verify
original backend/queue reuse at the history ceiling, preserved role pins,
conflicts, absent versions and shutdown. This does not prove remote health or
external model weight switching. Optimized installation needs refreshing.

Optimized installation and installed smoke subsequently passed for `dec46fa`;
`RELEASE_AUDIT.md` records the binary hash and evidence. A separate installed
socket-service test passed real Ollama sorting with independent verification and
confirmed child/service cleanup. This closes the installation gap above, without
establishing clean-OS prerequisites or general live-model reasoning quality.

Inference identity validation (2026-10-12): all mandatory formatting, offline
all-target check/tests and Clippy checks pass with FlatBuffers 25.12.19.
Tests: 304 library + 21 integration = 325. Log:
`/tmp/cogfabric-inference-identity-tests.log`. Registration mismatch tests verify
no backend calls or retained forged version; the actual connected child test
verifies discovery and role restriction. Optimized installation must be refreshed.

Fresh optimized source installation and installed smoke passed for `8506352`.
Evidence/hashes are in `RELEASE_AUDIT.md`; this closes the installation gap for
transport identity validation. No additional live-model study was run.

Event rollback validation (2026-10-12): mandatory formatting, offline all-target
check/tests and Clippy with warnings denied pass with FlatBuffers 25.12.19.
Tests: 306 library + 21 integration = 327. Log:
`/tmp/cogfabric-event-rollback-tests.log`. Live/replay filtering, immutable metadata,
rollback at full history and active-capacity refusal are covered. Optimized
installation verification is pending for this source.

Optimized installation and installed smoke subsequently passed for `2ac215b`;
`RELEASE_AUDIT.md` records hashes and evidence. This closes the installation gap
above; no new model quality or executable sandbox result is inferred.

Interface reservation validation (2026-10-12): mandatory formatting, offline
all-target check/tests (308 library + 21 integration = 329) and Clippy with
warnings denied pass, using FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-interface-reservation-tests.log`. Legacy/default serialization,
nondefault roundtrip, invalid range, exact admitted allocation and insufficient
task-budget no-dispatch behavior are covered. Existing recovery/worker tests pass.

Fresh optimized installation and installed CLI/service smoke passed for `a918e18`.
Installed CLI configuration saved 384 interface tokens and rejected 4097 without
altering the valid file. `RELEASE_AUDIT.md` records binary hashes and workflow
logs. The separate live interpretation trial timed out at the original limit;
this does not undermine the deterministic admission tests or prove model quality.

Provider effort validation (2026-10-12): formatting, offline all-target check,
all-target tests (309 library + 21 integration = 330) and Clippy with denied
warnings pass using FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-reasoning-effort-tests.log`. Actual direct/managed authenticated HTTP
fixtures assert the opt-in field; default fixture requests assert absence. COGB
roundtrip/malformed tests cover v2 controls and legacy v1. Compiler `--conform`
against the previous bootstrap schema succeeds. No arbitrary request extensions
or budget increases are accepted.

Remaining-budget planning validation (2026-10-12): mandatory formatting, offline
all-target check/tests (310 library + 21 integration = 331) and Clippy with denied
warnings pass with FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-planner-admission-tests.log`. The new regression independently
constrains remaining calls/output/prompt bytes after interface dispatch and verifies
planner refusal plus whole-goal solving; existing graphs/recovery regressions pass.

Exact intent fence validation (2026-10-12): mandatory formatting, offline
all-target check/tests (312 library + 21 integration = 333) and Clippy with denied
warnings pass with FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-intent-fence-tests.log`. Negative wrapper/schema tests and bounded
two-call original-goal preservation cover the newly observed real-model formatting.

Fresh optimized installation and installed smoke passed for final source
`3b186cd`; `RELEASE_AUDIT.md` records hashes/logs. Separately, the real debug
STANDARD language path accepted fenced structured intent, skipped planning and
returned an unverified solver explanation within two calls, with confirmed child
reaping. `LIVE_INFERENCE.md` records actual totals and reproduction. This closes
single-case integration, not independent quality or benchmark evaluation.

Sorting comparison validation (2026-10-12): mandatory formatting, offline
all-target check/tests (313 library + 21 integration = 334) and Clippy with denied
warnings pass using FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-sort-comparison-full-tests.log`; focused comparison tests also
passed (`/tmp/cogfabric-sort-comparison-tests.log`). Actual HTTP fixtures exercise
both structured criteria through all six arms. These are fixture correctness
results, not independent held-out scores or live-model comparative gains.

Optimized installation and installed smoke passed for runtime source `3eb870c`;
`RELEASE_AUDIT.md` records hashes and logs. Recovery tests were then strengthened
to pin `reasoning_effort: none` in saved task settings while current configuration
changes to `high` and a different interface allocation. Actual resumed CLI workers
still send the original provider control on normal completion and cancellation,
reap children and leave the current operator configuration untouched. All mandatory
checks pass again with 334 tests; log:
`/tmp/cogfabric-resume-provider-pins-tests.log`. This final change affects tests only.

Structured role JSON validation (2026-10-12): formatting, offline all-target
check/tests (314 library + 21 integration = 335) and Clippy with denied warnings
pass using FlatBuffers 25.12.19. Log: `/tmp/cogfabric-advisory-json-tests.log`.
Fenced graph execution/durable validation and bounded fenced-critic revision are
covered, with negative wrapper/schema/permission cases. Existing raw JSON,
recovery, transport and budget tests continue to pass.

### Public comparison workflow verification — 2026-10-12

`compare --workflow-check` preserves execution/scoring controls while withholding
held-out attestations. All 336 Rust tests (315 library, 21 integration), formatting,
all-target check and Clippy with warnings denied passed with pinned FlatBuffers
25.12.19. Log: `/tmp/cogfabric-workflow-check-tests.log`. The initial restricted
sandbox run denied socket operations; the complete rerun with local socket/child
permissions passed. No security permission or inference capability is added.

### Local inference availability — 2026-10-12

All 337 tests (316 library, 21 integration), formatting, offline all-target check
and Clippy with warnings denied passed using pinned FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-module-availability-tests.log`. Actual malformed peer response
closes the adapter; new routing, pinned dispatch and reactivation refuse it.
Connected/in-use observations do not prove remote health. No network probes,
credentials, grants or automatic retries are added. The inventory is bounded by
the existing 128-version registry and performs no inference. Prior optimized
installation evidence predates this runtime change and needs refreshing.

### Capability transport availability — 2026-10-12

All mandatory checks and 337 tests passed with pinned FlatBuffers 25.12.19; log
`/tmp/cogfabric-tool-availability-tests.log`. Existing actual arithmetic and SQL
malformed/forged-response tests now verify closed-state inventory, new-pin refusal,
rollback refusal and already-pinned rejection before queueing. No test was removed.
The bounded status inventory makes no network call; closed transports are not
silently retried. Original grants, expiry and call ceilings remain authoritative.
Optimized installation proof must be refreshed for these runtime changes.

### Grant-aware availability — 2026-10-12

All 339 tests (318 library, 21 integration), formatting, offline all-target check
and Clippy with warnings denied passed with FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-grant-availability-tests.log`. Actual two-call exchanges prove
exhausted-grant refusal; paused-clock tests prove expiry before any call despite
retaining a connected socket. No grant renewal, retry or additional tool authority
is introduced. Inventory stays bounded; state checks make no provider call.
Prior optimized installation proof predates this runtime change and must be
refreshed before readiness is claimed.

### Managed availability propagation — 2026-10-12

All 341 tests (320 library, 21 integration), formatting, offline all-target check
and Clippy with warnings denied passed using FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-managed-availability-tests.log`. New tests insert actual connected
adapters with same-binary child ownership, inspect managed wrapper state, shut
down/reap children and confirm closed observations. Expired unlaunched tool
wrappers remain uninitialized. No model probe, process spawn during observation,
credential, permission or deadline renewal is added. Optimized installation proof
must be refreshed after these runtime changes.

### Terminal managed shutdown — 2026-10-12

All 343 tests (322 library, 21 integration), formatting, offline all-target check
and Clippy with warnings denied passed using FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-terminal-shutdown-tests.log`. New regressions exercise repeated
shutdown before initialization and then valid inference/arithmetic/SQL requests:
all refuse dispatch without launch audit, startup ownership or initialized worker.
Existing interrupted-startup ownership and actual child shutdown tests still pass.
The initialization gate adds bounded startup coordination; it does not imply
immediate cancellation of external inference or completed reaping when shutdown
is requested. Original deadlines, authority and cleanup audits remain required.
Optimized installation evidence predates this runtime change and needs refresh.

### Concurrent initialization/shutdown coordination — 2026-10-12

The managed availability regressions now hold the actual initialization gate while
a separate shutdown task marks inference/arithmetic/SQL wrappers stopped. Cleanup
must remain pending until the gate is released, then finish real child reaping.
Bounded scheduling waits avoid arbitrary sleeps. All 343 tests and mandatory
format/check/Clippy checks pass; log `/tmp/cogfabric-shutdown-coordination-tests.log`.
This tests synchronization plus owned-child cleanup, not every OS scheduling
interleaving or proof that an external provider stopped. Runtime source is
unchanged from `a2f37c0`; these are additional regression assertions.

### Ranked inference candidate routing — 2026-10-12

All 345 tests (324 library, 21 integration), formatting, offline all-target check
and Clippy with warnings denied passed using FlatBuffers 25.12.19. Log:
`/tmp/cogfabric-ranked-routing-tests.log`. New tests verify rank/ID deterministic
selection independent of registration order, unchanged exclusive conflicts,
bounded ranks, actor-role scope, immutable replacement/rollback and task pinning.
A closed pinned candidate fails without invoking its alternative; a new task
selects the already approved alternative. Transport-identity forgery also fails
through candidate registration. Existing 32-active/128-version/four-slot queue
bounds and per-call permissions remain unchanged. Rank is human policy metadata,
not measured performance. Optimized installed proof requires refresh.

### Ranked authenticated Unix routes — 2026-10-12

All 346 tests (325 library, 21 integration), formatting, offline all-target check
and Clippy with warnings denied passed with FlatBuffers 25.12.19; log
`/tmp/cogfabric-ranked-unix-tests.log`. Two actual authenticated Unix adapters
compete by rank; pinned discovery identifies the preferred adapter, removal
redirects new tasks, and old pins still deliver exact original typed destinations
and versions. Forged version relabeling fails before dispatch. Existing peer
credential and separate-process regressions also pass. This adds transport
verification, not provider/weight attestation or unrestricted plugin execution.
Runtime source remains `2a94b93`; installed proof still needs refreshing.

## Ranked tool routing verification — 2026-10-12

Formatting, all-target offline compilation and Clippy with denied warnings passed
using `FLATC=/tmp/cogfabric-flatbuffers-build/flatc`. The three new ranked-tool
regressions passed. Complete sequential verification
`cargo test --offline --all-targets -- --test-threads=1` passed 349 tests
(328 library and 21 integration); log:
`/tmp/cogfabric-ranked-tools-tests-serial.log`.
Two default-parallel runs failed different existing tests: SQL authenticated RPC
returned an execution failure in the first; mixed worker recovery encountered a
held runtime lease in the second. Logs are `/tmp/cogfabric-ranked-tools-tests.log`
and `/tmp/cogfabric-ranked-tools-tests-rerun.log`. Causes remain unresolved;
sequential success does not establish parallel-suite stability or release readiness.
New routing retains deterministic host policy, deny-by-default task authorization,
immutable pins and bounded queues/registries; it grants no executable provisioning
or additional model capabilities.

## Empty-answer diagnostics verification — 2026-10-12

After removing operator-configuration dependence from HTTP fixtures, formatting,
all-target offline compilation and Clippy with denied warnings passed. Full
sequential tests passed 350 tests (329 library and 21 integration), recorded in
`/tmp/cogfabric-empty-answer-tests-fixed.log`. Fixtures verify fixed diagnostic
classification without exposing supplied reasoning text. An earlier run correctly
failed because the fixture inherited a newly saved reasoning-effort setting;
the corrected fixture uses explicit defaults instead. No additional model call,
automatic budget expansion or acceptance of reasoning-only output was introduced.
The previously observed default-parallel failures remain unresolved; these results
do not establish parallel-suite stability or final release readiness.

Two subsequent complete default-parallel runs at runtime revision `2d2fb3b` passed
350 tests each, logs `/tmp/cogfabric-parallel-investigation.log` and
`/tmp/cogfabric-parallel-investigation-second.log`. These satisfy the ordinary
command on these runs; neither success explains the earlier intermittent SQL
execution / runtime-lease failures. Retain those failures as unresolved evidence
rather than claiming a proven race fix from the independent HTTP-fixture change.

## Installed ranked-tool runtime — 2026-10-12

Cached offline locked optimized installation of source `6954a74` (runtime
`2d2fb3b`) succeeded in 1m04s into
`/tmp/cogfabric-v1-ranked-tools-install-20261012`. Binary SHA-256:
`0f96745c8a4cf6a490079b22f780d68c7c1b576ced1f53353b32940df496ab23`.
The unchanged installed smoke script passed CLI/service, typed socket, managed
worker, candidate rollback and restore workflows. Evidence:
`/tmp/cogfabric-installed-smoke.B6mzcD`, install/smoke logs
`/tmp/cogfabric-ranked-tools-install.log` and
`/tmp/cogfabric-ranked-tools-installed-smoke.log`. This uses installed existing
prerequisites/cached dependencies, not a clean-OS installation or neural safety
proof. Separately, the live Qwen retrieved-document attack contaminated answer
text while deterministic permissions stayed unchanged; see LIVE_INFERENCE.md.

## Quoted reference boundary verification — 2026-10-12

Formatting, all-target offline compilation and Clippy with denied warnings passed.
The final full suite passed 351 tests both sequentially and with default parallel
execution (330 library and 21 integration); logs:
`/tmp/cogfabric-quoted-reference-tests-fixed.log` and
`/tmp/cogfabric-quoted-reference-parallel-final.log`. The new regression preserves
forged reference section endings/newlines as JSON-string content. An intermediate
run caught prompt overhead exhausting a 500-byte admission fixture; empty-context
prompts now retain their original representation and that regression passes.
Nonempty-context recovery remains fail-closed on old prompt hashes; no migration
silently substitutes a changed prompt. The system-only live mitigation failed;
quoted references plus original-goal restatement resisted the identical attack
once. No general injection-resistance or independently verified prose claim follows.
Prior intermittent parallel SQL failures remain relevant: SQL execution has a
process-wide two-slot try-acquire semaphore, so concurrent tests can contend for
real runtime capacity. That is a source-level explanation to investigate, not yet
an instrumented reproduction proving every earlier SQL failure's cause.

## Bounded SQL worker admission verification — 2026-10-12

Formatting, all-target offline check and Clippy with denied warnings passed with
the pinned compiler. Default-parallel all-target tests passed 353 tests (332 library
and 21 integration), log `/tmp/cogfabric-sql-backpressure-tests.log`. Two new
capacity regressions verify overload rejection, queue deadline, cancellation,
permit restoration and queued SQL execution. Fixtures use independent instances
of the same limiter/dispatch function used by production, avoiding global capacity
contention while directly testing its bounds. The runtime still has two executing
slots and six total admitted requests; queue latency is capped at one second and
oversized input is rejected before admission. Historical SQL failure attribution
remains inferential; the separate runtime-lease contention remains unresolved.

## Retained lease descriptor verification — 2026-10-12

A new real-kernel regression duplicates a shared execution lease descriptor,
confirms both descriptors have FD_CLOEXEC, drops the original and verifies worker
recovery still refuses without writing interruption records. After the duplicate
closes, recovery succeeds once and is idempotent. This proves last-descriptor
ownership semantics; it does not establish that pre-exec inheritance caused the
historical parallel failure. No force-unlock, automatic retry or authority bypass
was added. Full default-parallel tests passed 354 tests (333 library / 21
integration), log `/tmp/cogfabric-retained-lease-tests.log`; formatting,
all-target compilation and Clippy with denied warnings passed. Leading release
status now reflects current counts, real LM Studio evidence and renewed private
GitHub synchronization rather than stale authentication blockers.

## Current installed service and live model verification — 2026-10-12

Source `9b0cf59` / latest runtime `161dea5` installed offline and locked into
`/tmp/cogfabric-v1-current-install-20261012` in 1m29s. Binary SHA-256:
`8164bbe9f89166c914e3bf50309ff047e9f158caac48e72f2e09010fde8c7ae1`.
The existing installed smoke passed, evidence
`/tmp/cogfabric-installed-smoke.jDsXEf`, logs
`/tmp/cogfabric-current-install.log` and
`/tmp/cogfabric-current-installed-smoke.log`. This includes latest SQL admission
and quoted-reference runtime changes; no fresh-OS prerequisites are implied.

Separately, a fresh installed service with managed Qwen3 8B inference processed
an actual `ask --socket` sorting request and returned independently verified
`[-3,1,5,5]`. The owned service stopped with exit zero and removed its socket;
worker audit reports exit zero, reaped true, forced false. Evidence:
`/tmp/cogfabric-installed-live-service.CfKtiB` (configuration, outcome, service log,
status/stop responses, SQLite audit and binary hash). This verifies installed
CLI → authenticated typed local socket → service → managed worker → LM Studio
inference → compiled verifier. It does not establish general model quality or
security, controlled native residency mutations, arbitrary plugin isolation or
held-out benchmark results.

## Checkpoint prompt-format verification — 2026-10-12

Formatting, all-target offline compilation and Clippy with denied warnings passed.
Full default-parallel tests passed 355 cases (334 library and 21 integration), log
`/tmp/cogfabric-prompt-format-tests.log`. A positive compatibility fixture preserves
original query/context and cumulative model-call/output reservations; additional
negative variants refuse unknown/null declared formats and incompatible hashes.
Existing repeated recovery, registered-model and real local HTTP integrations pass.
An initial restricted-sandbox focused run could not bind its HTTP fixtures;
the full run used the required local transport permissions. No saved prompt,
permission or budget is automatically migrated to make an incompatible task resume.

## Natural-language constraint instruction verification — 2026-10-12

Formatting, all-target offline check and Clippy with denied warnings passed.
Default-parallel all-target tests passed 355 cases, log
`/tmp/cogfabric-language-constraints-tests.log`. A public-CLI live Qwen trial
initially omitted a command prohibition; after refining the fixed interpreter
instruction, a fresh repeat retained it. Original-goal preservation and immutable
permission enforcement remained in force on both runs. See LIVE_INFERENCE.md for
actual configurations, tasks and usage; one improved extraction does not establish
complete constraint fidelity, independently verified prose or release readiness.

## Pinned compiler bootstrap verification — 2026-10-12

`bash scripts/build-flatc.sh /tmp/cogfabric-flatc-bootstrap-20261012` successfully
cloned the public annotated v25.12.19 tag, verified source commit
`7e163021e59cca4f8e1e35a7c828b5c6b7915953`, built flatc with two jobs and checked
`flatc version 25.12.19`. Compiler SHA-256:
`6689abb8a0124cc94320b4ad7e48402221c98cb001e1d07fd80cb25d170a0ed1`.
Log: `/tmp/cogfabric-flatc-bootstrap.log`. Missing arguments, relative destination
and existing destination each returned exit 2 without cloning or overwriting.
`bash -n`, diff whitespace and Rust formatting checks passed. With FLATC pointing
to this newly built compiler, all-target offline check and Clippy passed; ordinary
parallel tests passed 355 cases, log `/tmp/cogfabric-bootstrap-compiler-tests.log`.
CI now uses the same script; its remote run has not yet been observed for this
change. This verifies source/compiler/runtime pairing on existing local build
prerequisites, not a fresh OS install or reproducible binary hashes across toolchains.

## Hosted bootstrap pipeline verification — 2026-10-12

The confirmed CI watcher for source
`fb7f9ec061dbb42b4ce2da6f81c9af6b3d397449` exited zero. GitHub's Rust job completed
successfully in 3m21s: checkout, stable Rust setup, the new commit-pinned FlatBuffers
bootstrap, formatting, all-target check, Clippy, tests, CLI self-test, complete
local CLI workflow smoke, offline security evaluation and offline training
preflight all passed. Run:
https://github.com/DrapNard/cognitive-fabric/actions/runs/38295462158
Watcher evidence: `/tmp/cogfabric-hosted-bootstrap-ci.log`.
Earlier runs for `666b789e11388bff67a1693380b4c183be6880ca` and
`cee0de5a558ad3c4d292d048d6cb70c184bb202c` were also observed completed/success.

This supplies hosted checkout/dependency/compiler/workflow evidence beyond the
existing local Cargo cache. The runner already provides build prerequisites;
it does not prove fresh Arch/CachyOS package installation, GPU inference, model
security, general plugin provisioning or held-out reasoning gains. The hosted log
also warns about checkout@v4's Node 20 runtime and a future ubuntu-latest image
migration; these are reproducibility maintenance items, not failed checks.

A separate request to inspect job-step metadata was not executed because the
automatic approval reviewer hit a usage limit. The watcher had already been
approved and running; its terminal result supplied the evidence above. No rejected
request was retried or routed around review.

## Typed tool discovery verification — 2026-10-12

The typed tool inventory covers all availability states, legacy empty-vector
compatibility, registry limits/conflicts, invalid capability/rank, forged unknown
availability and malicious shared oversized strings. Borrowed field bounds precede
allocations; envelope verifier limits depth 32, tables 4096 and apparent bytes
1 MiB with the existing 256 KiB physical cap. Discovery grants no capabilities.

After operator confirmation that GitHub access was restored, permissioned commands
succeeded. All 358 tests (337 library, 21 integration) passed, including actual
socket inventory, migrations, malformed frames, denial, cancellation and rollback
fixtures: `/tmp/cogfabric-sync-tests.log`. Formatting and all-target check passed.
An initial Clippy invocation caught a redundant fixture vector; it was replaced
with an equivalent fixed slice and Clippy then passed with warnings denied.
Compiler 25.12.19 `--conform` against the prior committed schema passed.

The expanded CLI/service smoke passed using the current debug binary:
`/tmp/cogfabric-sync-smoke.log`, evidence
`/tmp/cogfabric-installed-smoke.IAFaGs`. It verifies local and socket tool inventory
alongside existing workflows. This is not an optimized install, live inference,
GPU test or neural safety evaluation. Earlier hosted CI evidence applies to
`fb7f9ec`; CI for the new commit is separate.
