# Current Rust implementation

### Plan reviewed resident-model reconciliation — 2026-10-17

**Implemented (bounded operator planning/application); Partial (autonomous/physical lifecycle).**
`src/residency/planning.rs` plans a desired installed model/context set, reuses exact
loaded matches, loads only missing models and releases only explicitly selected
instance IDs. Canonical ordering and projected inventory hashes make decisions
reproducible. Invalid targets, incompatible unreleased contexts, expired/tampered
plans and stale inventories fail before mutation. Requests/actions are bounded to
eight; a 300-second review lifetime and 120-second mutation phase bound execution.

`residency-plan REQUEST --api-base ...` reads native inventory without initializing
state. `residency-apply PLAN --api-base ... --human-approve` uses an existing store,
rechecks explicit endpoint/inventory and persists an atomic one-attempt receipt.
Each step rechecks operator policy and expected inventory, audits its child operation,
and requires only the expected projected transition. Drift/uncertainty stops later
operations without retry or claimed rollback. Parent reports preserve confirmed
steps and explicit non-attestation flags. Existing credential/response/endpoint
limits remain; no new provider API or model actor grant is added.

All residency mutations now acquire configured-resource ownership before runtime
ownership, blocking changes under an idle live pool as well as active inference.
Retention protects plan receipts and linked operation tasks against replay-gate
removal. Receipt inspection is bounded to 1,024 records/32 KiB each under a five-second
SQL deadline. Retirement/compaction remains unimplemented and this quota is explicit.

Sources include residency/planning, existing mutation refactoring, CLI and retention.
Unit and actual HTTP/CLI fixtures cover reuse, exact unload/load, independent next-step
checks, expiry/content changes, corrupt/replayed receipts, no-state preview, approval,
audit rollback, unknown outcomes, idle ownership and retention. Native provider/weight
ownership, real load/inference cost, automatic per-task residency and physical GPU
limits remain unproven/incomplete. Jev+ and Jev-R remain Partial. Verification
passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**629 tests across 31 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. The first
five focused integration cases passed before adding two more CLI/drift cases and
expiry/receipt regressions; the final full suite includes all of them. Local
TCP/Unix fixtures required elevated socket access. Existing protocol pairing,
malformed frames, migrations, mock inference, denial, cancellation, recovery and
candidate rollback gates remain green. `flatc` is unavailable; schemas/bindings
were unchanged and fresh generation was not run. No real native load/GPU/quality
performance gain is claimed.


### Restore original configured resource admission during continuation — 2026-10-16

**Implemented (supported ready checkpoints and compiled configured pools); Partial (general recovery).**
Continuation now restores original `resource_admission` settings, working allocation,
resident catalog and target checks instead of refusing every configured task. A
bounded strict validator requires complete per-call intent/admission/dispatch/
completion chains, matching roles, identities, capacities, resident binding and
coverage. After reasoning context, targets must also match the pinned default/role
configuration. Earlier completed advisory targets must match declared coverage;
their untrusted results are not replayed. Unresolved workspace dispatches block
restoration. Native pools absent from execution settings remain unavailable.

Read-only preview validates history and probes resource ownership outside Store
locks. Approved resume performs a read-only preflight, restores/reuses a matching
compiled pool outside Store locks, then revalidates configuration and checkpoint
in the write admission transaction. Both preparation phases share one five-second
admission deadline. Original task deadlines, calls, output/prompt/tool/cycle charges
and model/skill snapshots remain in force. No fresh task budget or executable is
loaded from metadata. Resource admission and its original history are included
in continuation reports and start audits; the restored pool is attached to Session
before any missing graph/solver work.

Sources: `src/recovery/continuation_resources.rs`, continuation orchestration,
`src/resource_scheduler/{configured,resident}.rs` and shared binding helpers.
`tests/resource_resume.rs` exercises actual HTTP, actual direct/managed CLI resume,
current-config drift, ready cancellation, admission rollback, initial solver,
partial advisory graph, legacy empty catalogs, ownership/approval denial and 13
corruption/unresolved-history mutations. The seven focused tests pass. The expanded
fixture initially missed a `mut` binding; corrected without production changes.

Only fully recorded ready checkpoints are supported. Old empty-catalog records may
omit all resident metadata fields; partial metadata is rejected. Nonempty catalogs
require durable target checks. Uncertain in-flight calls cannot be refunded or
approved by a resource-release acknowledgement alone. General custom executable
pool recovery, provider stop/weight attestation, physical limits, dynamic lifecycle
and independent model-efficiency evidence remain open. Jev+ and Jev-R remain
Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**617 tests across 30 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Local
TCP/Unix fixtures required elevated socket access. Existing protocol pairing,
malformed frames, migrations, mock inference, denials, cancellation and candidate
rollback regressions remain green. `flatc` is unavailable; schema/binding files
were unchanged and fresh generation was not run. These fixture runs do not
establish real-model quality, GPU efficiency or provider-stop guarantees.


### Bind resident declarations to selected inference targets — 2026-10-16

**Implemented (trusted adapter/configuration coverage); Partial (provider/weight attestation).**
Nonempty resident catalogs now require the selected adapter to identify its actual
configured endpoint/model (including role overrides), or explicitly identify a
compiled deterministic backend. Unknown, malformed and unlisted targets fail before
pool queueing, provider contact or managed worker launch. Empty catalogs retain
legacy behavior without invoking new metadata callbacks.

`InferenceBackend::inference_target` and `ChatBackend::inference_target` default to
unknown. Local and same-image managed adapters derive metadata from their immutable
configuration; native conversations use the actual native model rather than role
profiles. Pinned Fabric routes and scheduled wrappers forward the selected module's
metadata. Scheduled dispatch pins a dynamic broker before checking coverage, so a
registry replacement during observation/waiting cannot substitute another backend.
Resident consumers now share the bounded canonical `backend::ModelTarget` type.

Target intent precedes the trusted metadata callback. Coverage/denial audit must
commit before resource reservation or execution. Accepted records bind the canonical
target, resident identities and catalog digest with no weights/permission attestation;
rejected records exclude raw invalid target/error data. Metadata callbacks run
outside Store/pool/registry locks. Task charges already committed by Session remain
charged on denial; there is no retry, alternate route, budget refund or extra grant.

Tests cover reentrant callbacks, unknown/unlisted/invalid targets, audit failures,
unchanged empty-catalog behavior, role selection/canonical endpoints, route mutation
without target substitution, and actual CLI refusal before HTTP/worker launch.
Existing native/collective HTTP and bound-expert coverage exercise listed targets.
The existing resident-test doubles now explicitly declare their synthetic target.

This checks declared configured-target coverage, not provider-returned model labels,
actual loaded weights, catalog memory completeness or real backbone sharing. Native
operators can misdeclare metadata; raw Unix/custom adapters remain unknown unless
the host implements the contract. Dynamic catalog/lifecycle integration, measured
residency, physical limits and independent real-model studies remain open. Jev+
and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**610 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Full tests
required elevated local TCP/Unix fixture access. An initial new CLI fixture wrongly
expected exit success for a failed task; corrected to assert the existing nonzero
exit and retained charges. New native error assertions were also corrected for the
existing non-Debug TurnResult type without changing production types. Final protocol
compatibility/malformed frames, migrations, mock inference, denial, cancellation,
recovery and candidate rollback regressions passed. `flatc` is unavailable;
schemas/bindings were unchanged and fresh generation was not run. No real-model
quality, physical memory or GPU-efficiency result is claimed.


### Resident backbone memory admission — 2026-10-16

**Implemented (operator-declared, pool-lifetime memory accounting); Partial (measured/dynamic residency).**
Optional `resource_admission.residents` declares immutable backbone IDs/versions,
weight digests, host/device footprints and consumer endpoints/model names. The pool
reserves each unique declared backbone once before request admission; multiple
consumers do not multiply its cost. Working requests must fit the capacity remaining
after resident charges. Completion, cancellation, quarantine resolution and shutdown
never refund resident memory. Snapshots separate total, resident and working charges.

Bounded validation rejects duplicate/conflicting identities, consumer aliases,
malformed versions/digests, oversized catalogs and impossible resident-plus-working
allocations. Canonical catalog/consumer ordering makes pool identity insensitive to
input order; live catalog changes are rejected. Existing empty/omitted catalogs keep
legacy configuration bytes. Each resource intent records the canonical catalog hash
and resident total with `residency_attested:false`; configured execution snapshots
retain the declared catalog. No model load, grant, endpoint contact or weight read
is performed by declaring a catalog.

Implementation: `src/resource_scheduler/{resident,resident_tests}.rs`, pool/state
initialization, configured pooling and admission metadata. Coverage includes shared
consumer accounting, actual memory-constrained FIFO dispatch, cancellation/release,
live-catalog pinning, invalid metadata, CLI validation/execution, primary/bound expert
calls and mixed HTTP native/collective contention. The HTTP fixture now has two
compute slots; the resident memory charge is what prevents simultaneous dispatch.

This enforces declared arithmetic, not measured provider RSS/VRAM or actual weight
sharing. Consumer declarations do not attest that every runtime backend/model is
covered. Automatic load/unload, measured catalog completeness, provider ownership,
physical isolation, task-pool restoration and independent real-model efficiency
studies remain open. Jev+ and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**603 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Focused
resident tests also passed all four cases. Full tests required local TCP/Unix
fixture access outside the restricted sandbox. Existing protocol pairing, malformed
frames, migrations, denials, cancellation, recovery and candidate rollback gates
remain green. `flatc` is unavailable; schemas/bindings were unchanged and fresh
generation was not run. No live-model quality or GPU efficiency gain is established.


### Configure and operate Jev+ shared admission — 2026-10-16

**Implemented (configured workspace and trusted-host scope); Partial (physical/global GPU scheduling).**
Optional `Config.resource_admission` validates fixed working allocations and bounded
pool settings. `configure-resources FILE` / `--disable` updates operator config
without database bootstrap. Engine clones share one retained pool; Session applies
it to every role and bound-expert call, including managed inference. Native agent
turns use the same executor, and HTTP Server construction shares the Engine pool
across both request types. Original task budgets/charges/deadlines remain in force.

Configured pools hold a private exclusive workspace resource lease. Another
configured host cannot start fresh counters while the owner is live. Bounded strict
lifecycle validation rejects unresolved prior dispatches at startup. Human-only
`resource-status` reads an existing database; `resource-release ID --human-approve`
takes resource/runtime leases, validates exact identity and records an operator
stop acknowledgement before removing the restart gate. No provider stop is inferred.
Retention preserves unresolved resource audits and excludes live configured owners
when applying deletion. Original configured-task continuation still requires explicit
restoration and is denied, even before a first resource intent exists.

Implementation: `src/resource_scheduler/{configured,admission,adapter}.rs`, Config,
Engine, Session, AgentRuntime, CLI, HTTP Server, retention and continuation guards.
Tests cover real CLI configuration/reconciliation, managed process ordering/reaping,
primary/bound calls, shared native/collective requests over actual loopback HTTP,
workspace ownership, audit tampering and retention. The first new HTTP fixture
compile exposed a missing `IntoFuture` import; fixed in the test. Historical config
bytes retain the omitted optional field. See `docs/RESOURCE_SCHEDULING.md`.

General cross-workspace/device arbitration, resident/backbone accounting, physical
VRAM/RSS enforcement, task-pool restoration and independent reasoning/efficiency
studies remain open. Jev+ and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**598 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Socket
fixtures required elevated local TCP/Unix access. Both ordinary mock and managed
inference, protocol compatibility/malformed frames, migrations, denials, cancellation
and candidate rollback regressions are included. `flatc` remains unavailable;
schemas/bindings were unchanged and fresh generation was not run. No real-model
quality/latency improvement, GPU enforcement or general reasoning superiority is
established by these fixture runs.


### Shared Jev+ inference resource admission — 2026-10-16

**Implemented (trusted native, process-local scope); Partial (general GPU scheduler).**
`src/resource_scheduler.rs` and `src/resource_scheduler/adapter.rs` add a shared
FIFO pool across independently registered inference backends/brokers. Fixed
operator allocations reserve compute slots, host bytes and device bytes together;
queue and execution deadlines, bounded inventory and scoped cancellation prevent
unbounded pending work. Backend pins retain original allocations and transport
identity. Per-task kernel budgets and charges remain enforced independently.

Intent/admission/dispatch audits precede provider work. Success requires a durable
completion before freeing capacity; failed/timed-out/dropped dispatches quarantine
the allocation. Only an explicit audited operator stop acknowledgement can release
it. No prompt/answer/credential/error body is included in pool events. Native-only
`resources.configure` and `resources.resolve` reject model actors. Default recovery
rejects original pool usage instead of silently continuing without its limits.

Tests in `src/resource_scheduler/tests.rs` and
`src/recovery/continuation_policy_tests.rs` cover three-dimensional contention,
FIFO fairness, cancellation races, queue overflow, deadlines/shutdown, audit
failures, preserved transport pins, independent Engine brokers, original charges,
and pool-bound recovery denial. Initial compilation identified a private role
helper and a MutexGuard dereference; both were corrected using the existing role
mapping without duplicating authority. See `docs/RESOURCE_SCHEDULING.md`.

Settings and allocations are host declarations, not physical GPU/RSS enforcement.
Default CLI/HTTP configuration, resident/backbone accounting, provider ownership,
durable/cross-process allocation and pool restoration remain open. Jev+ and Jev-R
remain Partial; no model training, quality gain or live GPU benchmark is claimed.
Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**589 tests across 27 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Full tests
required elevated local TCP/Unix fixture access. Existing protocol pairing,
migrations, malformed-frame, denial, cancellation and rollback regressions remain
green. `flatc` is unavailable; schemas/bindings are unchanged and fresh generation
was not run. The focused first pass also passed all 11 then-present pool tests;
final tests include subsequent cancellation/state/recovery regressions.


### Restore pinned executive policies during continuation — 2026-10-16

**Implemented (supported checkpoint and trusted-host scope).**
`recovery::continuation::{preview_with_escalation_policy,resume_with_escalation_policy}`
accept an explicitly restored native policy. Every saved descriptor must match;
strict parsing rejects ambiguous policy snapshots. A supplied handle cannot claim
legacy compatibility merely by using the built-in descriptor. For a revised
solver, restoration additionally requires the original durable intent/allow pair
bound to candidate, mode, cycle, checker, original configuration, compiled prompts,
and current source fingerprint, with exact prompt-chain costs and sufficient
recorded remaining resources. Missing/denied/altered allowances fail before
backend construction. An existing allowance is validated, not invoked again.

The restored handle is carried into the Session, so subsequent unresolved answers
use the original policy under original budgets and independent checking. New
CLI `--retry-study` and `--retry-thresholds` options on `resume-preview` and
`resume-task` reconstruct only the compiled empirical policy from bounded
operator-selected data; there is no executable loader, implicit policy swap or
promotion. Resume approval is checked before file/configuration access. The CLI
opens an existing current-schema database without creation, migration or bootstrap.
Default/legacy recovery remains available through the existing built-in path.

Sources: `src/recovery/continuation.rs`, `src/recovery/continuation_policy.rs`,
`src/escalation.rs`, `src/escalation/empirical.rs`, `src/main.rs`, `src/store.rs`.
Regressions in `src/recovery/continuation_policy_tests.rs` and
`tests/policy_resume.rs` cover native and actual CLI/HTTP restoration, immutable
study identity, reentrant callbacks, missing approvals, live leases, audit rollback,
ready cancellation, original call/output charges, no replay, and legacy/schema/
symlink/source-drift denials. The CLI fixture initially attempted a private Store
method and was corrected to use its own SQLite connection; a lint correction
made its test-lock scope explicit without weakening production checks.

Jev+ and Jev-R remain Partial. General custom reconciliation, executable policy
attestation/provisioning, automatic policy catalogs, global GPU scheduling and
independent utility studies remain open. There is no new model grant or claimed
quality improvement. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**574 tests across 27 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. The full
test run required local socket access outside the restricted sandbox. Tests use
scripted/loopback backends; no fresh real-model quality or speed result is claimed.
Schemas were unchanged; `flatc` remains unavailable, so fresh binding generation
was not run.

### Empirical retry evaluation and executive cost gate — 2026-10-16

**Implemented (bounded host API and operator inspection); Experimental (utility
policy).** `src/escalation/empirical.rs` aggregates reviewed single-retry outcomes,
including failures, and uses exact success-rate/cost-per-success thresholds.
Task requests bind mode, cycle, compiled criterion, effective configuration,
role prompts and runtime-source fingerprint. Matching evidence can veto or admit
one solver/checker retry under the existing deterministic kernel limits; unknown
scope, unverified prose, higher cost or insufficient time decline further work.
No model/task permissions, model loads, resource ceilings or recovery approvals
are widened. `retry-study-evaluate` inspects bounded operator-selected files
before any state initialization; only the operator receives `evaluation.retry`.

Study/threshold content fixes the immutable policy identity. Build-time source
hashing includes Rust, schemas/bindings and locked build inputs. Hashes establish
local byte consistency, not executable/weight attestation. Review and inclusion
of failures remain operator-attested. Public workflow reports stay frozen and
are not relabeled as held-out studies. No real utility gain is established.
Custom policy recovery still refuses unavailable policies rather than replacing
them. Jev+ remains Partial: general GPU scheduling, independently measured
routing/utility and safe executable provisioning remain open. See
`docs/EMPIRICAL_ESCALATION.md` for all six component statuses and exact bounds.

Regressions: `src/escalation/empirical/tests.rs`, `tests/retry_study.rs` and
`tests/runtime_fingerprint.rs` cover exact threshold edges, all failure costs,
scope and source drift, budget denial, ambiguous/unreviewed data, immutable
identity, actual Engine admission with independent rechecking, CLI no-state
inspection and symlink/actor denials. Formatting and workspace all-target check
passed. All 562 tests across 26 targets passed with local TCP/Unix fixture access;
the restricted-sandbox attempt first failed 99 socket tests with EPERM (397 passed).
The reviewed FlatBuffers bundle/pin checks, migrations, malformed frames, policy
denials, cancellation and rollback regressions are included. Native flatc is
unavailable, so exact regeneration was not repeated; protocol files are unchanged.
Warnings-denied workspace Clippy and final whitespace review passed. No fresh
live-model study, neural training, GPU attribution or quality gain is claimed.

### Replaceable executive escalation policy — 2026-10-16

`src/escalation.rs` introduces a trusted versioned `EscalationPolicy` installed
through `Engine::with_escalation_policy`. After hard resource admission, typed
hash/metadata requests can allow or decline a revised solver or an unverified
critic/reflection/solver sequence. A policy cannot grant tools, raise limits or
change evidence. Future independent verification must fit the tool allowance
before another rejected-answer solver is bought. No new model, external probe or
always-resident inference backend is introduced.

Reasoning snapshots identity and audits intent before callbacks and validated
metadata decisions before extra inference. Store reentry is lock-free; failures
and invalid decisions fail closed. Independent success, repeated-answer stopping
and baseline profile isolation precede custom policy use. Recovery refuses
unavailable custom or altered/spoofed policy snapshots; matching built-in and
legacy built-in tasks remain eligible under normal admission. Native callbacks
are trusted, nonblocking host code, not sandboxed or implementation-attested.
See `docs/ESCALATION.md`, `src/engine.rs`, `src/reasoning.rs` and recovery tests.
Jev+ remains Partial: measured utility, custom-policy restoration and general
resource scheduling are not established by this interface.

Verification: 551 workspace tests (490 library including server, 61 integration),
formatting, workspace all-target check and warnings-denied Clippy passed with
`FLATC` unset. Seven new regressions cover policy and recovery boundaries. Early
test-fixture corrections supplied the proper Store search argument, used explicit
non-demo inference status, and isolated checker budget from Full-profile retrieval.
Production permission checks and acceptance thresholds were not weakened.
Prior source `4974acb` and installation-evidence source `ad00a3e` passed hosted CI;
this new feature is not yet hosted/optimized-install/live-model verified.

### Capacity-aware admission to module routes — 2026-10-16

New inference and arithmetic/SQL pins exclude modules whose bounded queue is
observed full. Among candidates with room, operator rank still dominates sampled
queued/processing load and ID breaks ties. A shared queue-pressure sampler reads
available slots once per candidate; selection never reserves a slot. Existing
pins retain their exact module/version: inference still rejects a full queue,
while tools retain deadline-bounded waiting. No dispatch is replayed or refunded.

Regressions fill four buffered slots behind a gated active call, select an approved
higher-rank alternative for new pins, refuse selection when all available routes
are full, and restore preferred routing after draining. Both arithmetic and SQL
are exercised, including cancelled waiting on an old handle. Existing rank,
pin, failure, permission and callback-lock tests remain. This advances local
scheduling; it does not establish global provider/GPU capacity, fairness or a
general module scheduler. The first tool test incorrectly waited for the full
30-second dispatch deadline; it was corrected to observe/cancel the existing wait
without changing production backpressure or limits.

Verification: 544 workspace tests (483 library including server, 61 integration),
formatting, all-target workspace check and warnings-denied Clippy passed with
`FLATC` unset. Sources: `src/fabric.rs`, `src/fabric/tools.rs`; regressions:
`src/fabric/tests.rs` and tool tests. Previously installed `df4cf05` evidence remains
source-scoped; this new routing source is not installed/live-model proven yet.

### CLI and managed-worker revision recovery — 2026-10-16

`tests/resume_worker.rs` now exercises approved `recover-revision-result`, read-only
preview and resume through actual CLI processes for saved solver, critic and
reflector phases. Each phase starts exactly one bounded same-image inference
worker, preserves original `reasoning_effort=none` despite changed current settings,
performs only missing calls, and reaps its child. Successful outcomes remain
unverified with five total calls / 1280 reserved outputs. Three SIGINT cases retain
one unknown result and all charged reservations, close the fixture connection,
reap the child and leave no unfinished worker audit session. No provider/GPU stop
or live-model quality is established by a loopback fixture.

The suite can target an explicitly installed executable via
`COGFABRIC_TEST_INSTALLED_BINARY=/absolute/path/to/cogfabric cargo test --test resume_worker`.
The override applies only to integration-test process launches, requires an
existing absolute file, and adds no production setting or authority. Earlier
Core-only revision evidence is now supplemented with real CLI/worker coverage.

Verification: all 542 workspace tests (481 library including server, 61
integration), formatting, all-target workspace checking and warnings-denied Clippy
passed with `FLATC` unset. The first test build referenced a private role allowlist;
the fixture was corrected to explicit known role names without changing visibility.
Optimized installation verification is the next separate evidence step.

### Recover a recorded whole-goal solver before critique — 2026-10-16

Approved `recover-revision-result` now reconciles a recorded cycle-positive solver
answer into `ready_critic` for unstructured DEEP/RESEARCH tasks. Normal admission
checks original snapshots, stopping hash, schedules, same-boot deadline and enough
remaining capacity for critic/reflection/revised solver (three calls and 640
reserved outputs, plus bounded prompts). No prior inference or cycle is replayed.
The answer remains unverified; a non-admitting or invalid critic stops with the
saved answer and only one new critic charge. An admitting critic proceeds through
the missing reflection and solver under original limits.

HTTP regressions cover the three-call path (five total / 1280 reserved outputs)
and both critic stop cases (three total / 768 reserved outputs). Additional tests
cover exact call/output/input budget refusals without writes, and another crash
after a recovered critic: a recorded result can form `ready_reflector`, whereas
an absent result retains unknown outcome and denies recovery. Other-boot,
unknown inference outcomes, and arbitrary backend/subtask recovery remain partial.

Verification: 540 workspace tests (481 library including server, 59 integration),
formatting, all-target workspace check and warnings-denied Clippy passed with
`FLATC` unset. Initial stop-test failure was an incorrect trace-field lookup;
assertions now read the persisted nested task answer and pass for both stop cases.
Hosted run `38827055986` passed for prior source `71f5e1f`; run `38827410095`
for `44706cd` was still running when checked. New source is not yet hosted-proven;
no new optimized installation or live-model claim is made.

### Continue after a recorded critic — 2026-10-16

`recover-revision-result --human-approve` now also reconciles a completed
same-cycle solver/critic pair into `ready_reflector`. Strict critique content
must match the checkpoint. Continuation revalidates that pair and reserves no
new allowance: at least two original calls, 512 output tokens and bounded prompt
capacity must remain for reflection plus a revised solver. Original settings,
module/model pins, stopping hash, schedule and aging same-boot deadline still
apply. Resume invokes only the missing reflector and solver; no prior inference
is replayed and the outcome remains unverified.

Tests cover actual HTTP (two new calls, five total / 1280 reserved outputs),
insufficient call/output/input allowance, changed critique reason, and a second
crash after recovered reflection. A recorded reflection can form `ready_solver`;
a reserved reflection without its result still fails closed without writes.
This extends the completed-reflection path; arbitrary within-cycle positions,
unknown provider outcomes and other-boot recovery remain incomplete.

Verification: 536 workspace tests (477 library including server, 59 integration),
formatting, all-target workspace checking and warnings-denied Clippy passed with
`FLATC` unset. The previous runtime's hosted run `38827055986` was still running
when checked; no new hosted success, installation or live-model claim is made.

### Reconcile a recorded revision handoff — 2026-10-16

`recover-revision-result TASK --human-approve` links a fully recorded same-cycle
solver/critic/reflector sequence to the next solver boundary without inference
replay. The prior boundary, consecutive calls, strict admitting critique and
original stopping-state hash are checked. Normal continuation admission validates
all original snapshots, remaining budget and same-boot deadline before committing
the new boundary and operator audit together. Missing, altered or unresolved
results, non-admitting critiques, live leases and terminal tasks fail closed.
Structured independently checkable objectives retain their separate result path.

The report omits answer/strategy text and claims no verification or resume grant.
Actual local HTTP continuation uses the recorded reflection with exactly one new
solver call, five total calls and 1280 reserved outputs. The answer stays
unverified. CLI denial precedes state initialization; audit-write failure rolls
back the boundary. The first broad test pass exposed missing original stopping
hash validation; that check was added before final verification. General unknown
provider outcomes, recovery after only a critic, and other-boot/consumed-cycle
positions remain unsupported. No permission, call, deadline or cycle is renewed.

Verification: all 533 workspace tests (474 library including server, 59
integration), formatting, workspace all-target checking and warnings-denied Clippy
passed with `FLATC` unset. The preceding graph commit's hosted run `38826523808`
failed the existing paused-clock idle-worker/socket test; test-only commit
`5c41eed` resumes real time after simulated idle, preserving production limits.
The focused idle test and full suite were rerun with that fix. Hosted verification
of the new source remains pending; no fresh live-model or installation claim.

### Recover interrupted graph initialization — 2026-10-16

Approved `resume-task` now initializes missing pending nodes after a saved planner
result and graph, including a partially persisted initialization. Preview reports
`initial_nodes_to_record` and leaves state untouched. Admission still validates
original model/module/context/settings, charged planner identity, committed
schedule, same-boot deadline and remaining budgets. Missing nodes alongside any
advanced node state remain rejected. Missing transitions and the continuation
start commit in one lease-protected transaction before worker/backend setup.

Regression coverage includes no-approval refusal, read-only preview, write failure
after an earlier initialization write (full rollback), missing state after prior
execution, and actual local HTTP continuation of both crash windows with custom
schedule. Exactly three new solver calls finish the task; the original planner is
not replayed and total reservations stay four calls / 1024 output tokens. No
permissions, deadline or resource allowance is renewed. Unknown provider outcomes,
other-boot continuation and general consumed-cycle recovery remain incomplete.

Verification: all 529 workspace tests (471 library including server, 58
integration), `cargo fmt --all -- --check`, workspace all-target check and Clippy
with warnings denied passed with `FLATC` unset. Checks used locked cached
worktree dependencies; no new live-model or installed-binary result is claimed.

### Include executing arithmetic and SQL work in routing — 2026-10-16

Equal-rank tool candidates now compare buffered requests plus the one currently
executing request before the deterministic ID tie-break. A drop guard clears the
processing observation on completion, cancellation, errors, shutdown and worker
unwinding. Both arithmetic and SQL regressions verify empty-buffer busy routing,
unchanged old handles, cancellation/completion cleanup and return to the original
tie-break; existing rank-priority/queued-load tests remain. Worker-panic tests also
assert cleanup. All 527 workspace tests (469 library including server, 58
integration), formatting, all-target checking and warnings-denied Clippy pass with
`FLATC` unset. No protocol, permission or queue ceiling changed. This is local
sampled scheduling, not reserved capacity, provider/GPU occupancy or a general
module scheduler. Previous installed and live evidence remains source-scoped.

This is the implementation snapshot, not the target specification. The complete
French vision is preserved in `ARCHITECTURE.md`. The branch is `develop/v1.0`,
package version `1.1.0-alpha.1`; it remains a pre-release despite its historical
version numbering. Rust 2021 and Tokio own the runtime; Python is offline only.

The seven responsibilities currently map to these modules:

| Layer | Implementation |
|---|---|
| Language Interface | `language.rs`, `backend.rs`, `config.rs`, `agent/`: advisory intent, explicit history, synthesis, loopback inference and bounded native conversation turns |
| Jev+ | `engine.rs`, `planning.rs`, `routing.rs`: task lifecycle, replaceable trusted expert selection, capability gaps, bounded dependency scheduling |
| Jev-R | `reasoning.rs`: shared-budget model roles, revisions, stagnation detection and independent verification status |
| Expert Network | `registry.rs`: immutable reference manifests, dependencies, lifecycle and task version snapshots |
| Cognitive Fabric | `wire.rs`, `service.rs`, `store.rs`, `agent/protocol.rs`: typed owner-local IPC, durable state/evidence and SQLite/FTS5; optional `crates/cogfabric-server` HTTP transport builds on the core |
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

Current verification (2026-10-14): 526 workspace tests, formatting, all-target
checking and Clippy pass. Bundled generated protocol sources and the Rust runtime
are pinned to FlatBuffers 25.12.19; default builds require no installed compiler.
Current CLI/service arithmetic and real Qwen generation smokes pass. Public graph
and Rust diagnostics measure particular workflows; no independent held-out quality,
weight attestation, hot-swap, energy or model-superiority claim follows.
See `IMPLEMENTATION_MATRIX.md` for every remaining engineering requirement and
`QUALITY_REVIEW.md` for actual command evidence.

Optional managed worker mode now launches the same Rust executable lazily per
task, provisions authenticated typed settings and closes/reaps it on completion
or cancellation. Three actual CLI integration tests cover demo role execution,
zero-process arithmetic and Ctrl-C during pending HTTP inference. This is process
coordination, not an executable sandbox or model-loading proof. See `WORKERS.md`.

Persistent workers also hold shared runtime leases. Operator-only recover-workers
marks open audit sessions interrupted under exclusive ownership, without task
result changes, PID killing or inference replay. Tests include an actual child
crash and transactional rollback; stale temporary-directory cleanup remains open.

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

Operator `diagnostics` reports storage/schema/registered-selection checks without
creating a task. Optional bounded local model discovery reports unavailable or
missing model IDs and produces a nonzero CLI exit; it does not perform inference
or attest weights. See `DIAGNOSTICS.md`.

Recorded capability gaps now support bounded operator inspection/preparation and
quarantined gap-guided RAG imports. Candidate metadata and its evidence link commit
atomically after revalidating the recorded gap hash; source tasks are protected from
retention. Automatic source discovery/cleaning remains absent. See `EVOLUTION.md`.

Module brokers measure per-call queue waiting independently of backend reports.
Optional queue timing crosses module/owner IPC and accumulates in task/comparison
resources; cancelled partial totals remain unavailable. GPU/load timing is absent.

Expert manifests now include bounded prompt/workflow checklists. Full/experts
execution can invoke one routed, version-pinned workflow through the shared solver
backend, reserving budget for whole-goal reasoning. Advice is unverified and grants
no permissions. Bounded multi-step and registered-model advice are implemented; executable
experts and measured domain usefulness remain incomplete.

Operator SQLite backups now use a pinned read view, bounded copy/hash loops,
private files and serialized admission. They preserve committed WAL state and
publish a checksummed read-only standalone database. Full artifact/configuration
restore and automatic replay are not implemented. See `STORAGE.md`.

Offline training preflight now has nine standard-library tests, frozen input hashes,
strict provenance/deduplication, a no-supervised-token termination guard, disjoint
new output and bounded adapter inventories. This improves the experimental script;
no actual ML training, isolation, neural evaluation or promotion is proven.

Explicit `sql:` tasks now execute bounded read-only queries over fresh task-local
inline tables without model calls. Task permissions, worker/VM/data/output bounds,
denied external access and hashed verification evidence are covered by tests.
This built-in capability is distinct from dynamically loaded external experts;
see `SQL.md` for its limited verification scope.

Workflow experts now accept bounded acyclic advice-step graphs as an alternative
to legacy text checklists. They execute dependencies with the shared backend and
task budgets, record step identities/hashes and preserve pinned versions. Results
remain unverified advice; measured model usefulness and executable workflows are
not proven.

`restore-database` now restores a saved-hash-verified schema-6 SQLite snapshot into
a new private directory and records provenance. Existing workspaces, corrupt or
live-WAL inputs and incompatible schemas are refused. Running tasks are preserved
without replay; external artifacts/configuration and older-schema migration are
not included.


Comparison reports disclose per-case bound expert model snapshots and flag the
loss of the same-model configuration control, including refused/failed expert
advice. This prevents separately configured experts being described as ordinary
same-model ablations; backend weights remain unattested.

Offline training also requires reviewed hashed train/validation/test split
metadata and rejects normalized prompt overlap before ML imports. Split identity
is recorded, but semantic independence and neural acceptance remain unverified.

SQL comparison cases can use structural JSON result scoring against reviewed
expected labels, preserving cell types and row/column order while ignoring object
field order and whitespace. Status alone cannot make a wrong answer pass.

The evaluation directory now supplies a provisional 100-case authored corpus and
hashed inventory. Ten categories include deterministic tools and ambiguity/policy
reasoning. Independent review, held-out status and live performance remain unproven;
no benchmark accuracy or security result is inferred from format/oracle tests.

Optional decomposition/exploration now preserves the first whole-goal solver's
call, output-token and prompt-byte capacity. Conservative dependency escaping
bounds and audited skips prevent advisory stages consuming its reserved budget.

Optional operator GPU diagnostics use one fixed bounded NVIDIA query and report
whole-device observations separately from task metrics. The actual driver is
unavailable here; live GPU values, energy and task attribution remain unverified.

Operator diagnostics can inspect enabled registered model endpoints with bounded
deduplicated discovery, including potential workflow experts. Advertised IDs are
not loaded-weight or successful-generation evidence. Local model files were found,
but LM Studio's local API startup timed out; real inference remains unverified.

Comparison CLI supports an optional reviewed dataset hash pin, rejecting changed
bytes before execution and recording the operator pin separately from actual
content identity. This does not establish held-out independence or label quality.

A later host-access check succeeded where sandboxed checks failed: actual framework
GPU telemetry sampled a 12 GiB device, and LM Studio loaded an existing quantized
model for a bounded test. Its authenticated HTTP server returned 401 for discovery
and generation; no successful model answer is claimed. The owned test model was
unloaded and the server returned to its prior stopped state. Protected explicit
operator API credentials remain an integration gap; none were accessed or stored.

Opt-in local bearer authentication now binds the dedicated operator environment
credential to one local API scope, including managed workers and discovery.
Synthetic direct/managed generation and escaped-echo regression tests pass.
Live authenticated generation remains unverified; see `AUTHENTICATION.md`.

New database backups include bounded effective configuration under the same
checksum. Restore defaults to database-only; `--restore-configuration` explicitly
validates and creates private settings for the new workspace. External artifact
files and older-schema migration remain separate recovery work. See `STORAGE.md`.

Optional `backup --include-rag-artifacts` and restore `--restore-rag-artifacts` now
preserve checked RAG candidate files with private quarantine and rebased paths.
Candidate status and promotion boundaries are preserved. Neural and unrelated
external artifacts remain excluded; see `STORAGE.md` for scope and failure handling.

Recognized schema-1–5 snapshots now support explicit `--migrate-schema` restoration
through bounded existing migrations on a new staged copy. Source and destination
schema versions are recorded; original files and task state are preserved. Unknown
and unversioned snapshots remain rejected. See `STORAGE.md`.

Trusted embedding hosts can register versioned event-module descriptors and
subscribe to selected progress stages for one task. Handles pin filters/versions
and share bounded FlatBuffers queues with owner observers. Registry state is
process-local; cross-process actor provisioning remains incomplete. See `EVENTS.md`.

Full task trace reads now have event/raw-byte ceilings and strict JSON validation.
`task-events` exposes bounded complete audit pages with durable cursors for larger
histories. Comparison identity reporting avoids full-trace allocation. These changes
support safe recovery inspection without replay; see `RECOVERY.md`.

New reasoning results carry their charged call identity. `inference-audit` reads
bounded durable reservations/results without exposing prompts/answers, and keeps
legacy or unrecorded usage unavailable. It grants no resumption authority; see
`RECOVERY.md` for limits and remaining checkpoint requirements.

Independent structured verification now also supports integer sorting, in addition
to dependency ordering, arithmetic and inline read-only SQL. `sort:` goals declare
one to 256 signed 64-bit integers and optional descending order; Rust checks exact
multiplicity and ordering before verified completion. Failed candidates can be
revised within the existing cycles/tool/call ceilings. This is a bounded domain
checker, not a general correctness evaluator or a measured live-model improvement.

For exact explicit `sort:` and `order:` goals, Full/Memory execution can reuse an
operator-published episode only after a fresh independently charged Rust check.
A valid retrieved answer completes without inference; rejected answers remain
advisory and normal reasoning continues within remaining limits. Unrelated goals,
revoked entries, explicit conversation references and other ablation profiles do
not gain this shortcut. A memory's prior status is never accepted as proof.

`recover-verified-result` now reconciles abandoned explicit sort/order tasks whose
saved solver answer passes a fresh independent check. It requires exclusive
runtime ownership, a fully correlated bounded ledger and remaining verification
allowance, then atomically commits proof/status without repeating inference.
Unsupported or unresolved tasks remain interrupted; general continuation is still
incomplete. See RECOVERY.md for failed-check charges and transactional behavior.

The Expert Network now supports checker manifests binding reviewed expert metadata
to compiled integer-sort/dependency-order verifiers. Existing immutable lifecycle,
dependency validation and task pinning apply; actual proof events record the
selected manifest version separately from the compiled criterion version. No
imported code or new permissions are accepted. See EXPERTS.md.


Explicit `resume-task --human-approve` now resumes validated pre-solver advisory
work using original resource charges and same-boot deadline evidence. Completed
nodes remain advisory; pending nodes and whole-goal verification use fresh kernel
authorization. Default direct Full/Collective tasks are tested through local HTTP.
Managed/registered/custom backends and general multi-phase/repeated resumption
remain unsupported. See `RECOVERY.md`; current mandatory suite has 232 tests.


Registered direct models are now supported by advisory continuation when the
original enabled registry version matches its saved manifest and configuration.
Changed/missing/disabled versions are refused without substituting a newer model.
HTTP fixtures verify the requested model identity; loaded weights remain unattested.


Managed same-binary inference workers now support advisory continuation. Admission
converts exclusive ownership to shared execution inside its write transaction,
then provisions only remaining worker budgets and reaps the fresh child on success
or cancellation. Actual CLI/process/HTTP tests cover both paths. Host enforcement
retains the precise original deadline; worker shutdown has a bounded cleanup grace.
Custom backend versions and later-phase recovery remain incomplete.


Solver cycles now persist bounded reasoning boundaries with previous answer,
revision strategy, repeat hash and cumulative reservation anchors. An unconsumed
boundary following a completed advisory graph can resume without resetting cycles
or repeat detection. Tampered/consumed boundaries and unresolved calls are refused.
No-graph, within-cycle and general repeated resumption remain incomplete.


Unconsumed solver-boundary recovery now also covers FAST tasks, malformed-plan
whole-goal fallback and explicitly budget-skipped graphs. Valid planner output
without its graph record is refused. Actual HTTP and budget/stopping regression
tests pass; within-cycle and repeated recovery remain incomplete.


Repeated recovery at later unconsumed solver boundaries now preserves the original
context/settings/deadline and cumulative counters. Each new context assembly must
match the original bytes and its chronological call/cycle anchors; history is
bounded to 17 assemblies. Tests cover two resumptions, unchanged settings, exhausted
cycles and tampering. Arbitrary within-cycle repair remains incomplete.


Failed structured saved-answer recovery can now report `revision_ready` and retain
an admitted next-solver boundary after a fresh charged independent check. It never
repeats the saved inference or treats rejection as success. Repeated failed answers,
unsupported state and unavailable budgets do not receive a revision; checkpoint
write failures roll back verification charges. Arbitrary within-cycle recovery
and unknown inference outcomes remain unsupported.


Host embedding APIs now deliver task-scoped progress to authenticated separate
processes using versioned COGS/COGE FlatBuffers, pinned module/stage grants and
bounded queues/timeouts. Actual child-process and negative transport tests pass.
Socket/child provisioning remains host-owned; general executable registration,
command routing and automatic IPC replay are still incomplete.


Authenticated process module streams now support COGS v2 durable replay followed
by deduplicated live delivery. Saved cursors advance through filtered pages and
host-retained grants restore task/version/stage scope after registry changes.
Version-one live output is preserved. Receiver frame/time limits and version/cursor
validation are tested; provisioning and general command routing remain incomplete.


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

SQL Unix RPC is now implemented as a trusted-host embedding API; see
`PROTOCOL.md` and `src/fabric/tools/sql/unix.rs`. Structured COGQ exchanges enforce
UID/PID, task/version/call/session grants, replay rejection and audit-before-work.
An actual child-process test executes SQLite. Cancelled/malformed exchanges close
permanently and metadata excludes query/result bodies. Automatic SQL process
launch/bootstrap/reaping from CLI/service is still not implemented; the installed
smoke evidence above predates this transport. Current mandatory checks pass with
301 tests (281 library and 20 integration); real inference remains unverified.

Managed SQL launch/cleanup is now implemented for direct CLI and socket service
through opt-in `sql_process` / `configure-sql-worker`. It reuses owned startup and
bounded reaping, adds SQL-only COTB v2 setup, and keeps arithmetic-only v1 intact.
Custom SQL registries and exhausted tool budgets cannot silently launch a default
worker. Actual CLI/service tests verify SQL execution and unsafe-query cleanup;
independent host SQLite verification remains mandatory. This supersedes the SQL
provisioning limitation above. Crash cleanup and executable sandboxing remain open.

Inference worker startup now owns the child/socket/lease before handoff awaits,
records launch intent and owned PID separately, and reaps failed/cancelled startup
through normal bounded cleanup. Retry of an unfinished startup is refused. A real
child regression covers cancellation, ownership-audit failure, retry refusal and
socket-directory removal. Crash cleanup and external GPU cancellation remain unproven.

Worker audit recovery now includes arithmetic and SQL sessions in the same
exclusive-lease transaction as inference. Mixed batches preserve task results and
charges, validate bounded metadata and record interruption without replay/PID
killing. Live tool sessions block recovery; malformed metadata and audit failures
roll back the batch. Legacy unversioned tool audit starts remain supported.
Filesystem cleanup and broader module lifecycle remain incomplete.

Journaled managed-worker socket cleanup now has verified operator preview/apply
commands and actual host-death coverage. It removes only identity-matched sockets
and empty private directories under exclusive runtime ownership; extra files,
symlinks, stale/tampered plans and malformed metadata fail closed. All 318 tests
and mandatory checks pass. See `RECOVERY.md` for pagination, partial audit/filesystem
failures, Linux requirements and unjournaled/hostile-owner limitations. Automatic
crash recovery and broader module lifecycle remain incomplete.

Current verification: 321 Rust tests (300 library, 21 integration), formatting,
all-target checking and Clippy pass. Trusted tool registries now support immutable
version rollback, including SQL, without changing existing task pins. Capability
identity survives unregistration; inactive backend references remain count-bounded
until shutdown. Real direct and managed-worker Ollama generation succeeded, with
independent Rust sorting verification and confirmed child reaping. See
`LIVE_INFERENCE.md` for measured limits; broader live studies and module lifecycle
remain incomplete. Earlier unavailable-inference statements are historical.

Inference brokers now also support host-only rollback to retained immutable
backend versions. Original queues and in-flight role snapshots are preserved;
active role conflicts fail before mutation. All mandatory checks pass with 324
Rust tests (303 library, 21 integration). See `MODELS.md` for retained-resource,
expired-transport and external weight identity limitations.

Transport-bound inference descriptors are now preserved by initial broker
construction and validated during registration. Changed module IDs, versions or
roles fail before inference; actual child-process discovery now reports its fixed
connected scope. All mandatory checks pass with 325 tests (304 library, 21
integration). This is trusted metadata consistency, not loaded-weight attestation.

Event module registrations now retain immutable metadata for host rollback and
reactivation. Future observers use the selected filter; existing live/replay and
process grants retain pinned versions. Active/history ceilings remain enforced.
All mandatory quality checks pass with 327 tests (306 library, 21 integration).
This adds process-local metadata lifecycle, not executable loading or persistence.

Structured goal interpretation now has an operator-selected bounded output
reservation (`interface_tokens`, default 128, 1–4096), configurable through
`configure-local --interface-tokens`. The original cumulative call/token/time
limits still govern admission. All mandatory checks pass with 329 tests. Live
interpretation quality remains separately evaluated; this option does not turn
empty/invalid model responses into success or grant permissions.

Local inference now supports an optional strict provider `reasoning_effort`
override, configurable with `configure-local --reasoning-effort`; default requests
are unchanged. Managed workers preserve it through explicitly versioned COGB v2,
with legacy v1 and malformed pairing tests. All mandatory checks pass with 330
tests. This requests provider behavior; it does not attest hidden reasoning or
prove successful structured interpretation.

Planner admission now preserves the first whole-goal solver against prior
interface/expert charges, checking remaining calls, output reservations and prompt
bytes and auditing skipped planning. All mandatory checks pass with 331 tests.
This corrects the two-call live trial's scheduler defect; valid real-model
structured interpretation is still not established by that correction.

The language interface accepts one exact Markdown `json` fence around the same
strict bounded object, preserving unknown-field rejection and the user's original
goal. Multiple objects, surrounding prose and malformed wrappers remain rejected.
All mandatory checks pass with 333 tests. This handles an observed provider format,
without making advisory intent a permission or proof of an answer.

The corrected bounded real language/solver path now also succeeds in a recorded
STANDARD-mode trial: two calls, accepted advisory structured intent, planner skip,
whole-goal explanation and confirmed managed-child reaping. The explanation stays
unverified. Optimized installation/smoke also pass for this source. See
`LIVE_INFERENCE.md` and `RELEASE_AUDIT.md`; robustness and benchmark gates remain open.

The six-arm comparison runner now supports structural `integer_sort_v1` scoring,
checking exact integer multiplicity/direction without trusting runtime verdicts
or answer whitespace. All mandatory checks pass with 334 tests (313 library,
21 integration). Public sorting examples are demonstration data; live reviewed
comparison remains outstanding. See `BENCHMARKING.md` for shared-verifier limits.

Interface/planner/critic JSON now shares one bounded exact-fence parser, preserving
strict schemas, graph checks and advisory status. Planner graphs and critic revisions
accept the same observed model wrapper as interpretation. All mandatory checks
pass with 335 tests; live recursive quality and gain remain separate research evidence.

Public comparison workflow checks now use the explicit `--workflow-check` flag.
The runner rejects missing/conflicting purposes and reports public results without
held-out review claims. All 336 tests and mandatory quality checks pass. The
existing held-out benchmark gate is preserved; independent datasets and intended
small-model quality measurements remain missing.

Inference version inventories now expose local adapter availability without remote
probes. Permanently closed Unix connections cannot be reactivated or routed;
pinned requests fail before dispatch. Unknown/connected/in-use observations are
not remote health guarantees. All 337 tests and mandatory quality checks pass;
optimized installation must be refreshed after this runtime change.

Arithmetic and SQL brokers now mirror inference local availability handling:
closed Unix streams remain diagnosable but cannot be pinned, reactivated or
queued. Actual malformed-response regressions cover both capabilities. All 337
tests and mandatory checks pass; no remote-health, grant-renewal or reconnection
claim is introduced.

Optimized installation and the full installed smoke workflow passed for source
`bb3c987`, closing the pending installed-source gap for inference/arithmetic/SQL
availability handling. See `RELEASE_AUDIT.md` for binary hash and actual evidence.
Grant expiry and call allowances remain independently enforced; present streams
are not health or renewable-authority attestations.

Unix arithmetic/SQL availability now includes expired/exhausted grants, excluding
these adapters from pinning, activation and pinned dispatch without refund or
renewal. Both actual two-call and paused-clock zero-call regressions pass. All
339 tests and mandatory quality checks pass; optimized installation proof awaits
refresh after this runtime change.

Managed worker wrappers now propagate initialized adapter availability; expired
unlaunched tool deadlines are visible without starting children. Actual socket and
same-binary cleanup regressions cover inference, arithmetic and SQL. All 341 tests
and mandatory quality checks pass; optimized installation refresh remains pending.

Optimized installation and installed smoke passed for source `aa95552`, refreshing
proof after grant-aware routing and managed availability propagation. Exact build
and evidence identities are recorded in `RELEASE_AUDIT.md`. Automatic residency
and general plugin provisioning remain partial; no stable release is declared.

Managed shutdown is now terminal even before first use, with initialization/handoff
coordination before cleanup inspection. Later valid requests cannot launch stopped
inference/arithmetic/SQL workers. All 343 tests and mandatory checks pass; the
optimized installation evidence needs refreshing for this runtime change.

Optimized installation and installed smoke passed for source `99c939f`, refreshing
proof for terminal managed shutdown and its concurrency assertions. See
`RELEASE_AUDIT.md` for exact binary/evidence identities. General lifecycle,
residency ownership and independent held-out evaluation remain unfinished.

Trusted hosts can now register explicitly ranked alternative inference modules for
the same role. New pins choose deterministically among available candidates;
existing pins never migrate or silently retry. Ordinary exclusive registrations
retain overlap denial. All 345 tests and mandatory checks pass; ranks are declared
policy, not learned cost estimates. General provisioning/residency remains partial,
and installed verification must be refreshed after this runtime change.

Optimized installation and installed smoke passed for source `d66dc07`, refreshing
proof after ranked inference routing; see `RELEASE_AUDIT.md` for exact identities.
Arithmetic/SQL capabilities still allow one active module each; explicit ranked
tool alternatives and their active-capacity policy remain to be implemented.

### Ranked tool alternatives (2026-10-12)

Trusted hosts can explicitly approve multiple arithmetic or SQL adapters with
bounded immutable ranks. New task pins select lowest rank then module ID; existing
pins retain original versions. Exclusive registration, transport identity checks,
authorization and rollback remain enforced. Active modules are capped at 32 and
historical versions at 128. Three new regressions pass; the full 349-test suite
passes sequentially. Two default-parallel runs exposed different existing failures
(SQL execution and recovery lease contention); see QUALITY_REVIEW.md. Automatic
plugin provisioning and adaptive cost scheduling remain incomplete.

### Reference instruction separation (2026-10-12)

Nonempty solver reference contexts are JSON-quoted and followed by the original
objective; all inference roles receive fixed untrusted-evidence advice. A live
Qwen attack contaminated answers before the change and with system advice alone;
the combined prompt resisted the same attack once. Answers remain unverified.
Empty-context cost/checkpoint hashes are preserved; earlier nonempty-context
checkpoint hashes fail closed under changed prompt serialization. Full tests now
pass 351 sequentially and in the final default-parallel run; see QUALITY_REVIEW.md
for intermediate failures and LIVE_INFERENCE.md for actual security trial scope.

### SQL concurrency admission (2026-10-12)

Inline SQL now provides bounded backpressure: six admitted requests per process,
two execution slots, one-second queue timeout and cancellation-safe waiting.
Blocking work retains permits after caller cancellation until actual completion.
Input bounds precede admission; read-only inline-data restrictions and existing
SQLite execution limits are unchanged. Managed workers have separate process-local
limits. Dedicated fixtures verify exhaustion, deadline, cancellation and queued
completion without relying on global parallel-test scheduling.

### Typed host tool discovery (2026-10-12)

Local and socket `tool-modules` expose immutable arithmetic/SQL versions, active
selection, routing ranks and availability through append-only typed FlatBuffers.
Legacy replies default to an empty tool inventory. Unknown states, conflicting
registries and excessive inventories fail closed. Borrowed string bounds precede
copying and envelope verification bounds shared-reference traversal. Discovery
starts no workers and grants no permissions. All 358 tests pass, including real
socket discovery; expanded CLI/service smoke passes on the debug binary.

### Inference lifecycle observation (2026-10-13)

`inference-modules` and its socket variant now expose retained immutable adapter
versions, active selection, local availability and ranks through typed, bounded
FlatBuffers. Existing active-only `modules` remains compatible. All 361 tests and
quality checks pass, including real socket and debug CLI/service smoke. Discovery
does not launch inference, grant permissions or attest physical model residency.

### Compiled prompt consistency on continuation (2026-10-13)

New execution settings capture the default compiled role/system prompt fingerprint.
Continuation requires it to match, in addition to the existing context, model,
budget and module checks. Legacy missing or changed fingerprints are not migrated
automatically; read-only inspection and independent result recovery remain
available. All 364 tests and mandatory checks pass; this is version consistency,
not provider or executable attestation.


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

The optional `cogfabric-server` workspace crate now builds on the core library
for authenticated loopback OpenAI-compatible Chat Completions. `src/agent/`
provides a transport-independent bounded native solver turn; OpenCode owns its
file/tool loop and approvals. Collective profile aliases retain Engine execution.
Actual OpenCode v2.0.26 with Qwen3 8B passed a disposable read/edit/read workflow.
See `HTTP_API.md` for setup, buffered SSE semantics and remaining native worker
and API limitations. Current workspace verification: 391 Rust tests, formatting,
all-target checking and Clippy with warnings denied. Earlier counts above are
historical milestones, not the current suite.

Native conversations now support opt-in same-image managed workers, including
registered models, scoped upstream credentials and one-task FlatBuffers grants.
Authenticated Unix exchange, audit-before-dispatch, cancellation, worker capacity
and normal/drop-path child reaping are verified. Actual OpenCode + Qwen3 8B also
passed a managed read/edit/read workflow. This supersedes earlier native-worker
limitations; role overrides and general independent executable isolation remain
unsupported. See HTTP_API.md, WORKERS.md and the latest quality evidence.

### Collective dropped-future cleanup

Collective inference/arithmetic/SQL process owners now retain child/socket/lease
ownership through asynchronous reaping after a task future drops. A shared
collective ownership drain is integrated into HTTP shutdown, including expert
inference owners. Cleanup failures disable new collective process admission
and fail shutdown for that host lifetime. Native and collective drains remain
separate; no new tool permissions or third-party sandboxing are introduced.
WORKERS.md describes deadlines and host-termination limits.

### Current installation verification

Source `a9933b3` passes a fresh Arch setup with Rust/Cargo 1.99.0 and FlatBuffers
25.12.19: 417 tests, all quality checks, optimized installations of both packages
and offline installed CLI/socket/HTTP workflows. Hosted CI also passed the new
HTTP smoke. Host managed OpenCode/Qwen evidence is separate from this clean-OS
run, which exercised no native inference or GPU. See INSTALLATION_VALIDATION.md
and RELEASE_AUDIT.md for exact scope and reproduction.

### Residency operation observation

`residency-inspect TASK_UUID --api-base NATIVE_BASE` now compares recorded
operator intent with fresh native inventory through read-only startup. It
reports matching instance IDs and observed desired state without resolving the
unknown outcome, changing task state or authorizing cleanup. Malformed/ambiguous
intent and endpoint mismatches fail before HTTP. Ownership-aware automatic
residency scheduling remains incomplete; MODELS.md specifies the limits.

### Basic benchmark preflight

`bench` now validates every dataset row before any task executes, rejects unknown
fields and empty/oversized cases, and records dataset identity plus runtime status
and charged model/tool task counts. Zero-call exhaustion remains distinguishable
from legitimate zero-model deterministic tool execution. Expected labels stay
outside inference prompts; scoring and comparison behavior are unchanged.

### Basic benchmark run cancellation

`bench --max-run-seconds` now bounds the run as well as each task. Ctrl-C or expiry
returns a partial report, durably cancels active inference and stops later cases;
the CLI exits unsuccessfully. Executed/unexecuted counts and run stop reason
distinguish cancellation from score failure. Existing Rust `benchmark` calls
retain a default 1800-second ceiling; `benchmark_until` accepts cancellation.
Cleanup grace and external provider continuation remain outside strict GPU-stop
guarantees. BENCHMARKING.md documents partial accuracy semantics.

### Benchmark identity disclosure

Basic benchmark reports now retain declared configuration and its hash, package
version and compiled role-prompt hash. Both runners share a bounded identity
reader for backend labels, expert bindings and primary registration snapshots,
without copying reasoning text. Configuration/weight identity remains explicitly
unattested; hashes and registrations describe metadata, not neural proof.

### Replaceable Jev+ expert selection

`routing::ExpertRouter` now separates executive skill selection from Engine
orchestration. The default tag heuristic remains unchanged. Trusted replacements
receive pinned skill metadata outside the SQLite lock; at most two distinct
enabled snapshot IDs are accepted, with policy version/audit disclosure. Old
Engine clones and admitted decisions retain their original handles/snapshots.
This adds no model/tool permissions or model calls. General native isolation,
policy catalogs, scheduler replacement and calibrated utility remain incomplete.

### Skill inventory bounds

Legacy and manifest registration now share a 4,096 retained-ID capacity under
immediate transactions. Skill snapshot/inventory reads cap rows and SQL field
bytes before Rust copies, strictly parse tags and reject invalid state/versions.
Valid legacy empty tags remain readable; malformed metadata is not silently
converted or repaired. The router uses the same inventory ceiling. EXPERTS.md
documents compatibility and the independent active/version limits.

### Manifest read integrity

Manifest snapshot/activation/idempotent reads now guard SQL field sizes before
Rust copies and verify bounded rows incrementally. Duplicate active identities
fail instead of silently overwriting a graph entry. Existing-version registration
checks stored body integrity before returning idempotent success. Corrupt data
remains unchanged and cannot be implicitly repaired or promoted by these reads.


### Explicit atomic expert disable sets

`skill-disable-set ID...` / `registry::disable_set` accepts 1–256 distinct
canonical IDs under the existing human-only `skill.disable` permission. The
operator explicitly selects the damaged expert and any dependents to contain.
Remaining enabled manifests, projections and dependencies must validate. No
cascade is inferred; unknown/already-disabled targets and late audit failures
roll back every enable change and audit in the immediate transaction. Bodies,
hashes and retained inventory capacity are preserved. Normal activation stays
strict; this is not executable isolation or automatic recovery. Invalid retained
inventory metadata can still require reviewed repair/restoration.


### Independent directed shortest-path verification

Explicit `path:` objectives declare `nodes`, directed unweighted `edges`, `start`
and `goal`. The compiled `shortest_path/v1` checker computes minimum distance
with bounded breadth-first search, then validates the candidate path. Any optimal
path is accepted; unreachable goals require `[]`, and identical endpoints require
`[start]`. Bounds: 64 unique ASCII node IDs (1–32 bytes), 256 unique non-self edges,
10,000 criterion bytes and 4,096 answer bytes. Unknown fields, missing endpoints,
invalid edges and duplicate IDs/edges fail closed before model dispatch. Cycles
are allowed; distance discovery visits each node once.

Example:

```text
path:{"nodes":["a","b","c"],"edges":[["a","b"],["b","c"],["a","c"]],"start":"a","goal":"c"}
```

The solver returns `["a","c"]`. Each check consumes a tool invocation and requires
`path.verify` for the original reasoning task's Jev+ actor. Failed checks feed the
existing bounded revision loop; checked answers retain hashes and cannot be
rewritten by optional synthesis. Criteria originate from the original objective;
model output, retrieved text and checker manifests cannot alter them or grant
permissions. Checker manifests may bind the built-in implementation as
`shortest_path/v1`; they do not install executable code. Recovery rechecks use the
same criterion and original durable tool accounting. Agent HTTP turns have no
checker grant. This verifies only the supplied finite graph, not external map
accuracy, weighted routing, real-world feasibility or general reasoning quality.


### Revision-chain resource admission

Before unverified deep/research answers enter critique, Jev+ now checks that an
additional cycle, three calls and 640 output-token reservations remain for the
critic (128), reflector (128) and revised solver (384). It also checks cumulative
prompt-byte upper bounds and each backend prompt's 32,000-byte ceiling. Bounds
use the exact candidate truncations, the validated 1,000-byte critique reason,
and a 4,000-character strategy bound of 16,000 UTF-8 bytes. No new allowance is
granted or charged by this advisory admission; each subsequent call still uses
normal durable reservations, authorization, deadline and cancellation checks.
Elapsed provider time can still prevent later stages after initial admission.

The durable `schedule` event precedes critique and records `admit_revision` or
`skip_revision`, required costs, uncharged prompt bounds and
`expected_utility_measured: false`. A denied chain preserves the current
unverified candidate with `insufficient_revision_budget`, avoiding critique or
reflection that cannot support another answer. Independent executable-criterion
revision retains its direct checker-to-solver path. This is feasibility-based
resource control, not calibrated expected utility, general scheduling, semantic
verification or evidence of improved model accuracy.


### Replaceable, recovery-pinned dependency scheduling

**Implemented (trusted Rust host scope):** `src/scheduling.rs` provides a versioned
`SubtaskScheduler` and default stable topological order. Engine installation
captures policy metadata; sessions validate full dependency-respecting selections,
audit intent before callback and persist order/hash before any subtask events.
`src/recovery/graph.rs` validates the committed schedule and sequential execution;
continuation prefix checks and actual graph continuation use that order without
reinvoking a policy. Legacy unmarked graphs retain default behavior; marked graphs
cannot silently fall back. Benchmark metadata discloses policy and no attestation.
See SCHEDULING.md. General parallel/module/GPU scheduling, untrusted policy
provisioning and measured expected utility remain incomplete. No grants, budgets,
protocol schemas or model-loading authority expand.


### Optional advice wall-time containment and shared timeout fallback

Optional legacy/structured workflow advice now has one cooperative phase deadline:
one third of the task's remaining original wall time, capped by the configured
inference timeout. The `expert_phase_budget` event records that ceiling before
advice execution. This is a deterministic feasibility heuristic, not measured
expected utility or a guarantee that later inference fits. Structured steps share
one phase allowance rather than resetting it per step. Original task deadline,
call/output/prompt/tool ceilings and individual authorizations remain in force.

A typed runtime inference timeout permits shared-backend advice to return control
to whole-goal reasoning, matching the existing bound-model fallback intent.
The spent call/output reservation remains charged; partial advice is discarded,
reported usage/queue totals become unavailable, and `expert_failure` must persist
before any main-solver work. Failure-audit errors stop execution. Other shared
backend errors, malformed results and kernel/audit denials remain fatal. Existing
bound-model failure fallback semantics are unchanged. Provider computation may
continue after client cancellation; `provider_stop_proven` remains false. This
adds no retries, new model loads, tool grants or fresh task allowances.

The real partial Qwen study at `22dbc5a` remains frozen and is not rescored or
rewritten. Its six expert timeouts motivated containment; this change has not
established improved Qwen accuracy or resolved its transport failure. Shared
backend recovery after a dropped managed exchange still depends on that adapter's
fail-closed session lifecycle; no automatic worker restart is introduced.


### Bound-advice failures remain fail-closed

Fallback now accepts only the private typed runtime timeout for shared and bound
advice. Previously the bound-model path caught every error, allowing policy/audit
or result-integrity failures to be treated as optional provider unavailability.
Unknown bound-provider errors now visibly fail the task; no primary inference
follows. Error text is never used as a classifier. Missing/disabled bindings still
skip before dispatch, and typed timeout fallback retains original spent charges,
unknown usage, mandatory failure auditing and remaining task limits.

This intentionally narrows availability fallback until a trustworthy typed error
contract can distinguish provider faults from kernel/transport integrity failures.
There is no retry, permission expansion, new process or model-loading authority.
Operator-trusted Rust remains outside an untrusted-code sandbox guarantee.


### Durable inference reservation accounting

`Session::call_with_backend` now records `model_dispatch` before mutating call,
output-reservation or prompt-byte counters and before taking prior usage metrics.
A rejected database insert therefore leaves existing session charges and reported
usage intact and cannot invoke the backend. A committed reservation still counts
if progress publication fails: `Store::event` marks that post-insert failure with
a private `PersistedEventError`, allowing the session to retain the durable charge
while returning the error before inference. Error text is not the classifier.

No committed reservation is refunded or replayed. Terminal task failure and
recovery audits retain their conservative unknown-usage semantics; preserving
prior in-session metrics does not fabricate a complete failed-task usage report.
The event schema, ceilings, authorization and backend execution order are otherwise
unchanged. A test-only observer poison hook exercises the post-commit boundary;
it is absent from production builds. Observer failure is not provider availability
and cannot enter the typed-timeout advice fallback.


### Unambiguous JSON at agent boundaries

Agent HTTP request bodies, native provider response objects, decoded native tool
schemas and tool-argument objects now reject duplicate decoded JSON keys at every
nesting level. This includes escaped spellings of the same name and duplicate
objects inside arrays. Byte caps remain 512 KiB for HTTP request bodies, 256 KiB
for provider responses and 32,000 bytes for schemas/arguments; ordinary JSON
recursion limits and trailing-data rejection remain enforced. The reusable Rust
`strict_json::value` helper preserves accepted JSON values without echoing the
rejected key. Raw valid tool argument strings are not rewritten.

Duplicate HTTP bodies fail before native inference. Ambiguous provider objects
cannot supply a completion; ambiguous tool proposals and correlated history are
also rejected. This eliminates a class of parser disagreement, not all prompt
injection or schema violations. Tool JSON Schema semantics and action permissions
remain the client's responsibility; Fabric proposes tools and executes none through
this API. No remote reference resolution, host capability, new tool or model
permission is introduced. Internal FlatBuffers framing/schema is unchanged.


### Strict provider and advisory JSON

Ordinary OpenAI-compatible reasoning responses now use the bounded duplicate-key
parser before selecting answer/model/usage fields. Structured advisory parsing
applies the same rule after validating its optional JSON fence, including nested
objects inside arrays. Native residency responses and inventory decoding also
reject duplicate decoded names before their values can be normalized or observed.
Existing response/role byte caps, JSON recursion limits, trailing-data checks,
credential echo checks, loopback-only HTTP and redirect/proxy denials remain.
Unique unknown native metadata stays forward-compatible; duplicate metadata fails.

Ambiguous initial inventory cannot authorize a residency POST. If an approved
POST has already occurred and its reply is ambiguous, the task remains interrupted
with `outcome_unknown`; no confirmation request, retry, inferred ownership or
cleanup follows. Model/provider content still confers no permissions or factual
attestation. Internal FlatBuffers and stored benchmark results are unchanged.


### Versioned declared capabilities and discovery

Expert manifests may now declare up to 16 distinct capability identities with
independent canonical semantic versions:

```json
"capabilities": [{"id":"domain.sort","version":"2.1.0"}]
```

This field is optional and omitted from canonical serialization when empty,
preserving legacy manifest hashes. Each expert's own ID implicitly provides its
manifest version; redeclaring that same ID is rejected. Capability identities use
the existing bounded project identifier syntax. Duplicate IDs, invalid versions,
unknown capability fields and excessive entries fail manifest validation. Aliases
are immutable with their manifest version and copied into task snapshots. Expert
dependencies still refer to concrete expert IDs/versions; aliases do not rewrite
or relax dependency constraints.

`skill-lookup domain.sort --requirement '^2'` lists active manifest providers whose
declared capability version matches the requested semantic range. The command
uses human-only `skill.inspect`; it grants no runtime authority. Results explicitly
say `declaration_only`, `permissions_granted: false` and `provider_health_proven:
false`. Unversioned legacy categories remain listed by existing skill inventory,
not fabricated as versioned lookup matches. Multiple compatible providers remain
visible rather than being silently activated or loaded.

Advisory language interpretation can name a declared capability separately from
an expert ID. The engine resolves it against the pinned enabled snapshot and
adds its provider ID within the existing four-refined-route ceiling. Resolution
prefers an exact active manifest, then the lexically first reviewed alias provider,
then legacy category metadata. This prevents a legacy placeholder from hiding a
reviewed provider. Current gap preparation reads enabled declarations and manifests
in one transaction, so aliases count as available metadata without creating a
candidate or grant. Capability versions are exposed by lookup; language intent
still names capabilities without version requirements.

Large interface prompts exceeding 32,000 bytes explicitly audit
`skip_interpretation` and preserve whole-goal reasoning rather than dispatching an
invalid optional prompt. The catalog is not silently truncated. These changes do
not prove operational expertise, document retrieval quality, provider availability,
weights, executable isolation or calibrated expected utility. Declaring a sensitive
name such as `shell.execute` cannot grant that permission. No model/tool loader,
FlatBuffers schema change, database migration or neural promotion is introduced.
Public Rust struct-literal callers must provide the new `capabilities` vector;
legacy serialized manifests remain compatible.


### Pinned capability route bindings

`route_refined` now records one bounded binding for each requested advisory
capability: requested ID, resolved provider, provider manifest version, declared
capability version, route-selection state, declaration-only status and no granted
permissions. Values come exclusively from the task's pinned snapshot. Replacing
an active declaration during interpretation cannot change the recorded old
binding. Legacy unversioned declarations retain null versions; missing capabilities
have null providers/versions and remain explicitly unselected with a gap record.

A compatible provider can be available but unselected when the existing four-route
ceiling is reached. `selected` describes route membership, not executed advice,
provider health, factual correctness or granted authority. Existing dispatch,
budget and verification records remain authoritative for those separate claims.
No route ceiling, permission, model call or protocol schema changes.


Ordinary builds no longer require an installed flatc: verified pinned Rust bindings
are included for all nine unchanged FlatBuffers schemas. Explicit FLATC still
validates exact 25.12.19 regeneration. Schema/binding drift fails; see PROTOCOL.md
for updating the bundle and INSTALLATION_VALIDATION.md for package evidence.


### Exact native tool-choice and text-part controls

Named `tool_choice` objects accept exactly
`{"type":"function","function":{"name":"declared_tool"}}`. Unknown outer or
nested fields, non-object function controls and undeclared names are rejected.
String choices `auto`, `none` and `required` retain existing semantics. This avoids
forwarding extra control fields in direct mode that native managed transport would
otherwise drop. Strict-schema declaration belongs to the tool definition, not the
named choice; the client still validates schemas and approves actions.

HTTP text parts accept exactly `type: text` and a string `text`. Extra image or
other fields are explicitly rejected rather than silently removed while flattening
text. These unsupported shapes return 400 before inference. Valid named choices
and ordinary text arrays remain supported. No tool execution, permission, budget,
conversation role or FlatBuffers schema is added.

### Fresh planning/decomposition phase ceiling

Fresh collective planning plus advisory subtask execution now share one phase
ceiling: one-third of the remaining original task wall time, capped by the
configured inference timeout. `planning_phase_budget` persists the ceiling before
planner dispatch. No subtask resets it. A typed timeout drops pending local work,
accounts elapsed inference, retains reservations, marks usage/queue totals unknown,
marks unfinished subtasks skipped and records `planning_failure` before proceeding
with the original objective and remaining budgets. Partial strategy is discarded;
completed subtask audit results remain explicitly unverified. Policy, scheduler,
provider and audit failures other than typed inference timeouts stay fatal.
Cancellation remains distinct and follows existing terminal task handling.

This leaves time for subsequent whole-goal work, not a guarantee of provider
completion, remote inference termination or measured expected utility. Trusted
synchronous callbacks cannot be preempted by this Tokio phase timeout. Saved graph
continuation keeps its existing original-task ceilings; this new fresh-phase
ceiling is not a general persistent phase-deadline recovery implementation.
Research exploration has its existing separate admission and is not covered by
this planning ceiling. General cost-calibrated scheduling remains Partial.

Paused-clock regressions cover a stalled planner, a completed subtask followed by
a stalled dependent step, retained calls/outputs, discarded partial strategy and
fatal failure-audit denial. Final integer-sort answers still require compiled
verification. Live quality/latency after this change is not yet measured; previous
installed binaries and graph reports retain their original source scope.

### Research exploration phase ceiling

The fresh RESEARCH explorer call now receives a separately audited ceiling of
one-third of remaining original task wall time, capped by configured inference
timeout. `exploration_phase_budget` is persisted before dispatch. Only a typed
inference timeout may become `exploration_failure` and continue the original
whole-goal solver within remaining budgets. Charged calls/output reservations are
retained, elapsed inference is accounted and reported usage/queue totals remain
unknown after interruption. No alternative is appended on timeout. Successful
exploration is still unverified strategy, not evidence or permissions.

Policy/provider/audit errors remain fatal, including a backend error whose text
says "Inference timeout" without the typed timeout condition. Failure auditing
must succeed before the final solver can start. This supersedes the earlier
fresh-planning note that excluded research exploration; it does not claim general
persistent phase-deadline recovery, synchronous-callback preemption, physical
provider cancellation or calibrated expected utility. Original task deadlines,
calls, tokens and permissions remain unchanged. Live effect and current optimized
installation after these phase changes are not yet verified.

The optional Wasm experiment now has a Core task audit bridge (`module_tasks` and
`experiments/wasm-experts/src/core_audit.rs`, feature `core-audit`). Trusted operator
embeddings can create fresh bounded tasks with pinned identities, original clocks,
recovery leases and transactional allocation/dispatch/outcome records. Duplicate
executor wrappers cannot reuse a recorded allocation. Cancelled or expired tasks
cannot acknowledge further results. Abandoned tasks recover as interrupted with
uncertain dispatch records preserved; guest resumption is unsupported. The bridge
adds no model capability, production interpreter dependency or execution endpoint.
An in-memory Store is useful for tests but does not provide disk durability.

Explicit `recover-advisory-result TASK_UUID --human-approve` now handles the
crash window between durable advisory inference and its graph-state write. It
links only the latest charged cycle-zero result after validating the complete
prospective continuation checkpoint. Advice remains unverified; task status,
original budgets/deadline and permissions remain unchanged. Unknown outcomes,
expired clocks and unsupported phases still cannot resume through this operation.

An operator-approved native LM Studio load/unload workflow now has one real
embedding-model lifecycle result in addition to fixtures. The Nomic instance was
loaded and unloaded through Core, preserving the already-loaded Qwen instance and
context. The script defaults to inspection and stops without cleanup on unrelated
residency drift or uncertainty. This is not generative model switching or an
ownership-aware automatic scheduler; see `MODELS.md` and the frozen native report.

Expert scheduling now distinguishes refusal before admission from an attempted
workflow. It can skip an unaffordable routed workflow in favor of the next routed
candidate while admitting at most one workflow. An admitted timeout/failure never
adds another expert attempt or resets budget. This addresses budget selection;
calibrated usefulness and general model/module scheduling remain incomplete.

Workflow admission now precedes backend setup. A refused model-bound candidate
creates no expert model snapshot, worker handle or advice-phase allocation. Actual
admitted workflows retain their original prompt construction and shared resource
ceilings. Diagnostic phase counts no longer include newly refused candidates;
historical reports retain the semantics of their recorded source revision.

Equal-rank inference routing now counts a currently processed request as well as
queued requests, allowing a newly pinned task to choose an idle peer when the
first module's queue is empty but busy. Operator rank and existing pins are
preserved. Typed inventory carries optional queued/processing observations;
legacy frames retain unknown values. Processing state is cleared on cancellation
and shutdown. This is local broker scheduling, not GPU ownership or global
provider scheduling.

The inference broker now excludes a closed local worker queue even if the backend
advertises Connected. Existing pins fail without rerouting, while future pins can
select another approved candidate. Registry health callbacks run outside locks;
activation revalidates retained identity so retirement during observation cannot
restore a removed version. This is local failure containment, not a proof of
external provider termination or isolated native execution.

Arithmetic and SQL brokers now share the inference broker's local failure rule:
closed queues override optimistic backend health, existing handles fail without
rerouting, and future handles may select another approved candidate. Tool health
callbacks can inspect registry metadata outside locks; activation revalidation
prevents restoration of a version retired during that observation.
