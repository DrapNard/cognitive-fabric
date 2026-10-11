# Changelog

All notable changes are documented here in English.

### Interrupted inference-worker startup cleanup

- Retain child, socket and execution-lease ownership before connection/setup awaits.
- Persist launch intent before spawning and owned PID before handoff; fail closed
  when audits fail and refuse implicit retry of an unfinished initialization.
- Reap failed/cancelled startups through the normal bounded cleanup path.
- Test real child cancellation, failed ownership audit, retry refusal and socket
  cleanup. Host crashes and stale-resource recovery remain separate work.

### Managed SQL task workers

- Add opt-in `configure-sql-worker` for lazy task-scoped built-in SQL processes
  in direct CLI and socket-service execution.
- Reuse the bounded tool broker and owned startup/reaping lifecycle; retain
  independent result checking and original task budget admission before launch.
- Add SQL-only COTB v2 setup while preserving arithmetic-only v1; reject
  capability/version mismatches and custom registry substitution.
- Test real CLI/service SQL, unsafe-query cleanup, exhausted-budget refusal and
  negative setup/scope boundaries. Crash cleanup and executable sandboxing remain open.

### Authenticated SQL process transport

- Add task-scoped structured COGQ Unix RPC with approved UID/PID peers,
  fixed session deadlines, cumulative call grants and replay rejection.
- Fail closed before backend execution when authorization audit persistence fails;
  close cancelled or malformed exchanges and omit query/result bodies from audits.
- Bind SQL registration to transport identity and reject cross-capability grants.
- Verify actual child-process SQLite execution and negative transport boundaries.
  Automatic SQL worker provisioning and executable sandboxing remain incomplete.

### Installed tool lifecycle verification

- Extend installed smoke checks to managed arithmetic authorization/reaping,
  rejected-task cleanup, SQL routing and restored worker configuration.
- Verify arithmetic/SQL service asks and retain binary/script hashes with evidence.
- Validate a fresh optimized installation and all 292 Rust regression tests;
  keep clean-OS setup and live-model release gates explicitly open.

### Typed SQL capability modules

- Route SQL through separately pinned, versioned tool modules and bounded queues.
- Add structured COGQ FlatBuffers tables/cells/results with decoded allocation limits.
- Independently rerun the constrained SQLite checker before accepting module output;
  preserve task admission, cancellation and arithmetic protocol compatibility.
- Keep SQL process provisioning and arbitrary executable tools explicitly unfinished.

### Managed arithmetic task workers

- Add opt-in `configure-tool-worker` and same-binary lazy arithmetic workers for
  direct CLI and socket service tasks, preserving bounded broker dispatch.
- Deliver an authenticated typed one-call task grant without inherited credentials.
- Retain ownership through interrupted startup, reap children and clean sockets;
  refuse custom registry substitution and exhausted tool allowances.
- Test real CLI/service execution, invalid arithmetic cleanup and unfinished-startup
  reaping. General executable lifecycle and crash cleanup remain incomplete.

### Authenticated arithmetic process transport

- Carry task-scoped arithmetic commands over mutually authenticated Unix sockets.
- Enforce immutable task/version/call/session grants and audit before execution.
- Reject duplicate correlations, forged responses, malformed frames and pipelining;
  close cancelled exchanges and drop pending worker futures on disconnect.
- Verify actual arithmetic in a separate process; automatic tool-worker provisioning
  and executable lifecycle remain incomplete.

### Typed arithmetic module dispatch

- Route arithmetic through bounded version-pinned FlatBuffers module queues.
- Retain task permission/budget enforcement and independently reject wrong answers.
- Cancel active and retired module versions during service shutdown.
- Keep general process command routing explicitly incomplete.

### Durable module event replay

- Add explicit COGS v2 catch-up with filtered cursor advancement and deduplicated
  replay/live delivery, preserving live-only v1 compatibility.
- Restore host-retained task/version/stage grants after registry changes; reject
  backwards cursors, version changes and receiver frame/time exhaustion.
- Keep automatic provisioning and general module commands outside this observer API.

### Authenticated process event modules

- Deliver filtered task progress to host-approved local processes using versioned
  FlatBuffers envelopes, mutual UID/PID checks and pinned module/stage grants.
- Bound frames, queues, lifetimes and writes; reject upstream commands and report
  loss explicitly while preserving task execution during observer cancellation.
- Test real child-process delivery, malformed identities/frames, overflow and
  deterministic deadlines. General module command routing remains incomplete.

### Failed-answer revision reconciliation

- Prepare an admitted revision boundary after a fresh independent rejection of a
  fully recorded whole-goal solver answer, without repeating inference.
- Preserve original allowances and repeat stopping; roll back proof, tool charge
  and boundary together on checkpoint persistence failure.
- Report revision readiness separately from verified success and require explicit
  continuation before generating a new answer.

### Repeated boundary recovery

- Admit later unconsumed solver boundaries while retaining the original context,
  deadline, settings and cumulative resource ledger.
- Validate each repeated context against chronological reservations and immutable
  bytes; bound assembly history and reject changes or exhausted allowances.

### Whole-goal boundary recovery

- Resume validated unconsumed boundaries for FAST and whole-goal fallback tasks
  without requiring an advisory graph.
- Preserve explicitly skipped plans and reject missing valid graph history;
  verify HTTP revision, original budgets and consumed-boundary denials.

### Durable revision recovery

- Persist exact bounded solver-boundary state before charging each reasoning cycle;
  stop dispatch if checkpoint persistence fails.
- Resume unconsumed boundaries after completed advisory graphs with original cycle,
  resource and repeat-detection state; reject altered or consumed checkpoints.
- Verify resumed independent checks through local HTTP without extra planner or
  advisory calls. Arbitrary within-cycle and no-graph recovery remain incomplete.

### Managed worker task recovery

- Continue validated advisory graphs through a fresh authenticated inference worker
  with remaining resource grants and the original host deadline.
- Convert admission ownership to shared execution under the database transaction;
  reap resumed children on completion or cancellation without replaying unknown calls.
- Verify actual CLI/process/HTTP behavior and exclusive recovery refusal.

### Registered model recovery

- Continue interrupted advisory graphs with their original enabled registered
  model version after verifying saved manifest and configuration identity.
- Reject missing, disabled or changed registrations without model substitution;
  verify actual HTTP model selection while retaining weight-attestation limits.

### Explicit interrupted advisory continuation

- Resume validated pending advisory nodes without repeating completed steps or
  resetting original resource/deadline ceilings; require operator approval.
- Correlate planner graphs, saved context and solver results with charged calls;
  refuse unsupported or unresolved checkpoints and preserve fresh authorization.
- Atomically persist terminal reports and typed outcomes; disclose partial timing
  and backend identity limits. General multi-phase recovery remains incomplete.

### Boot-bound recovery deadline evidence

- Persist original pre-setup Linux boot-time snapshots and reconstruct remaining
  wall allowance across processes on the same boot.
- Reject malformed/backwards clocks and preserve unavailable time after reboot
  or for legacy settings, without authorizing task replay.

### Measured advisory subtask costs

- Record per-node reservation and timing deltas, preserving unknown provider
  usage and distinguishing completed inference from verified results.
- Audit cancelled/skipped work and stop further advisory dispatch on cost-audit
  failure without changing task budgets.

### Installed binary workflow verification

- Add a repeatable fresh-state CLI workflow for installed binaries, covering
  deterministic tools, socket lifecycle, skills, RAG rollback and restore.
- Document and verify isolated optimized source installation; include the
  workflow in CI without claiming live-model or independent benchmark results.

### Atomic native lifecycle outcomes

- Commit final residency audit metadata and task status together, rolling back
  both on audit or status-update failure without retrying external effects.

### Explicit native model lifecycle

- Add operator-approved load/unload with exclusive workspace leases, exact native
  inventory preflight, durable intent and fresh inventory confirmation.
- Preserve uncertain external outcomes without automatic retries or unloads;
  disclose that native instance IDs do not establish exclusive ownership.

### Native local model residency inspection

- Add bounded operator-only LM Studio native inventory with installed keys,
  loaded instance IDs and reported context capacities.
- Require an explicit native endpoint and separate opt-in credential; reject
  redirects, malformed/oversized inventories and protected credential echoes.

### Service-owned broker cleanup

- Stop the shared inference broker after service handlers finish cancellation,
  revoking retained inference pins when the service exits.
- Verify typed STOP and host shutdown alongside durable task cancellation.

### Host inference broker shutdown

- Stop current and retired task-pinned inference modules with one permanent
  host-controlled signal; cancel active futures and discard queued work.
- Refuse new registration, pinning and dispatch after shutdown while preserving
  ordinary replacement and unregistration behavior.

### Release evidence reconciliation

- Track all 18 goal acceptance criteria with current evidence and remaining proof.
- Refresh the matrix test count and superseded recovery limitation; add the
  offline security command to CI and the README.

### Reproducible offline security evaluation

- Add `security-eval` with 24 fixed boundary probes, positive controls, fixture
  hashes, detailed results and unsuccessful exit status on mismatches.
- Explicitly report unmeasured security risks and exclude neural safety claims.

### Bounded interrupted-task reconciliation

- Bound and validate recovered task identities before updating durable state.
- Apply a five-second SQLite progress deadline with atomic rollback and callback
  cleanup, including when stored triggers perform expensive work.

### Reviewed comparison dataset pins

- Allow comparison runs to require an operator-reviewed SHA-256 before creating
  tasks, rejecting malformed pins and changed bytes without inference.
- Distinguish prior dataset pinning from the observed hash in reports while
  retaining independent-review and held-out limitations.

### Registered endpoint discovery

- Add operator-only discovery across enabled registered model endpoints, including
  expert configurations, with deduplicated bounded concurrent requests.
- Report exact registration versions/checksums and missing advertised IDs; skip
  disabled models and refuse oversized endpoint sets before network requests.
- Keep advertisement separate from generation, loaded weights and residency claims.

### Optional operator GPU diagnostics

- Add opt-in whole-device NVIDIA sampling with a fixed read-only command, bounded
  output/deadline and child termination/reaping on failure.
- Parse memory units, optional utilization/power and distinct device identities;
  retain unsupported or failed measurements as unavailable.
- Keep GPU observations outside task accounting and deny the capability to models.

### Preserve whole-goal reasoning budgets

- Include worst-case dependency prompt bytes and transport bounds in decomposition
  admission, preserving capacity for the first whole-goal solver.
- Skip research exploration when its cost would consume that solver's remaining
  call, output-token or input-byte budget; audit admission decisions.
- Cover four constrained-budget combinations without expanding operator limits.

### Provisional v1 evaluation corpus

- Add 100 original self-contained synthetic tasks in ten categories with stable
  IDs, hashed provenance inventory and explicit incomplete review flags.
- Validate corpus bounds/uniqueness and recompute thirty deterministic tool cases.
- Fix column-less SQLite reads for declared inline tables, with regressions retaining
  schema, unknown-table, temporary-database and attach denials.
- Document independent review and live evaluation gates without claiming held-out
  independence, neural safety or measured model performance.

### Structured SQL comparison scoring

- Add strict SQL result JSON comparison against operator-provided expected labels,
  preserving types and ordering while ignoring JSON object order and whitespace.
- Validate expected shapes before task creation and exclude failed/demo outcomes.
- Cover six-profile inline SQL execution and wrong-value/type/status denials.

### Offline dataset split manifests

- Require reviewed versioned train/validation/test identities before offline LoRA
  training; verify hashes and reject cross-split normalized prompt reuse.
- Record split provenance and hashes while keeping held-out rows out of optimization.
- Add small format examples and negative preflight tests; neural evaluation and
  promotion remain disabled.

### Ranked expert fallback

- Select workflow experts in relevance order instead of registry ID order.
- Skip unavailable model bindings without inference, allowing the next routed
  workflow while retaining one attempted workflow and shared task limits.
- Cover ranking, version replacement, disable fallback and admission audits.

### Expert model comparison disclosure

- Include per-case registered expert configurations and checksums in comparisons.
- Flag separately bound expert configurations as confounding same-model ablations,
  including failed/refused advice, without treating metadata as weight attestation.

### Registered-model workflow experts

- Bind workflow advice to immutable enabled local model registrations while sharing
  task budgets, permissions and bounded typed inference dispatch.
- Preserve in-flight bindings, honor live disable for new frozen-corpus tasks and
  retain failed-call charges without fabricating missing usage measurements.
- Cover independent HTTP selection, failure fallback and managed expert cleanup.

### Worker crash audit reconciliation

- Hold runtime execution leases through host startup and persistent worker serving;
  refuse launching inference for terminal tasks.
- Add operator-only recover-workers with bounded transactional interrupted-session
  records, preserving task results and never replaying inference or killing PIDs.
- Test live-worker exclusion, real child crashes, durable idempotency, malformed
  metadata and transactional audit failures.

### Task-bound worker lifetime

- Pin worker lifetime to the approved task wall ceiling through an additive
  bootstrap field, retaining the legacy 300-second default.
- Permit idle gaps within that lifetime while limiting incomplete frames to ten
  seconds from the first byte; verify expiration with deterministic virtual time.

### Managed same-binary inference workers

- Add opt-in configure-worker mode with lazy per-task child launch, authenticated
  typed bootstrap and pinned generation/database ceilings.
- Reuse one child across role calls, explicitly close/reap on task completion or
  cancellation, and retain kill-on-drop fallback plus identity-checked cleanup.
- Record launch handoff/reaping metadata; test actual CLI process execution,
  zero-process arithmetic, pending HTTP cancellation and custom-registry refusal.

### Typed single-task worker bootstrap

- Define bounded COGB FlatBuffers settings for a worker's approved task and pinned
  generation configuration, with no filesystem paths or executable grants.
- Validate version/role/configuration bounds and derive matching worker ceilings;
  test decoded bootstrap through the adapter/worker exchange.

### Durable worker decision auditing

- Require a trusted SQLite audit anchor for worker sessions; record bounded
  metadata-only decisions, reservations and outcomes without prompts or answers.
- Refuse dispatch on authorization-audit failure and atomically reject missing
  anchors; test database reopen, privacy and injected write failures.

### Authorized inference worker sessions

- Add worker-side host credential checks and immutable task/role grants.
- Enforce cumulative task budgets, bounded sessions and active cancellation on
  host disconnect/shutdown; reject forged identities before inference.

### Host-connected inference process transport

- Add a Unix inference adapter with host-approved OS peer UID/PID checks,
  bounded FlatBuffers exchanges and immutable task/module correlation.
- Close cancelled or invalid exchanges permanently; verify communication with a
  separate local fixture process without exposing process spawning to models.

### Task identity propagation correction

- Preserve caller task UUIDs in direct broker inference dispatch, matching the
  pinned engine path; reject malformed IDs before invoking the backend.
- Test concurrent distinct tasks through both entry points.

## [1.0.0] - 2026-10-09

### Added

- Local binary MessagePack protocol with versioned, bounded frames.
- Unix socket service and standalone CLI with English messages.
- SQLite event traces, FTS5 memory and dynamically registerable text skills.
- Pluggable deterministic demo and localhost OpenAI-compatible inference adapters.
- Resource-bounded Jev+-inspired routing and Jev-R logical cooperative roles.
- Retrieval candidate isolation, reproducible held-out retrieval evaluations,
  checksum verification and explicit human-gated RAG promotion.
- Optional, manually invoked offline LoRA/QLoRA experimental trainer.
- Security policy boundary and formal threat-model documentation.
- Automated regression suite, mutation test and CI configuration.

### Scope limitations

This is a **working orchestration reference runtime**, not a novel pretrained
neural reasoning model. FlatBuffers runtime, automatic training, LoRA deployment,
GPU-specific resource scheduling and performance parity with frontier LLMs are
not part of this release. See `docs/IMPLEMENTATION_MATRIX.md`.

## [1.1.0-alpha.1] - 2026-10-09

### Rust migration

- Ported the cognitive runtime to Rust 2021 with Tokio.
- Replaced MessagePack IPC with a typed, bounded FlatBuffers schema.
- Added Rust CLI, SQLite-backed task/skill/RAG candidate lifecycle, local model
  adapter, bounded Jev-R roles and independent capability gating.
- Preserved the original Python runtime in Git history on the `main` branch.
- Retained Python only for the optional offline LoRA experiment.

### Verification status

- **Build and tests not run**: rustc, cargo and flatc are unavailable in the
  authoring environment. This is an alpha port, not a verified replacement.

## Unreleased — 2026-10-10

- Compile the Rust alpha against the pinned FlatBuffers generator: fix required
  field accessors, alloc visibility and SQLite query lifetimes.
- Format the Rust workspace and record a reproducible Cargo.lock.
- Support an explicit FLATC path and reject nonmatching compiler versions.
- Verify 14 tests, Clippy, compilation and the CLI self-test; reconcile the
  implementation matrix with remaining v1.0 requirements.
- Constrain inference HTTP to literal loopback, disable redirects and proxies,
  bound request parameters and cap response bodies at 256 KiB. Add local HTTP
  boundary regressions (17 total tests passing with socket permission).
- Migrate unversioned SQLite state transactionally with future-schema rejection
  and bounded lock contention. Add candidate snapshot links, active-version
  retrieval, integrity-checked operator rollback and lifecycle audit records.
  Verify 19 tests, formatting, compilation and Clippy.
- Cancel tasks durably on disconnect, Ctrl-C and service shutdown; join bounded
  socket handlers before cleanup, authenticate peer UID and preserve replaced
  socket paths. Count dispatched inference calls and return failed CLI outcomes
  with nonzero exit codes. Add Unix socket and cancellation integration tests.
- Add bounded Jev-R critique/reflect/re-solve cycles with persisted role dependency
  nodes, replaceable inference backends and no-progress stopping. Keep generated
  results unverified without executable evidence. Measure optional token usage,
  runtime inference duration, prompt bytes, output reservations and tool counts;
  carry resource metrics through backward-compatible FlatBuffers response fields.
- Add immutable versioned reference manifests, dependency and cycle validation,
  staged activation, metadata rollback, disable, health checks and registry audit
  records. Pin task routing/reference snapshots transactionally and validate
  retrieved content hashes. Add trainer.s1 metadata without a neural backend.
- Persist validated loopback inference settings and independent role profiles
  through atomic owner-only config saves. Add configure-local/config-show and
  bounded no-follow regular-file reads across reviewed input boundaries.
- Parse bounded structured goals with a separate interface role, preserve original
  user instructions, handle clarification and missing capabilities, isolate
  explicit conversation references and optionally synthesize labelled results.
  Carry conversation/synthesis options through additive typed IPC fields.
- Copy reviewed candidate artifacts into bounded content-addressed read-only
  quarantine, verify deduplicated content, enforce aggregate storage limits and
  allocate candidate versions transactionally under concurrent imports.

- Require immutable held-out dataset provenance and active-baseline retrieval regression gates before RAG promotion; migrate evaluation storage to schema 3.

- Add typed service-status, cancel-active and stop controls that remain available during inference, with bounded connections and busy responses.

- Execute validated advisory subtask dependencies under the shared reasoning budget, persist graph transitions, and preserve a final solver for the original goal.

- Bind inference and tool dispatch authorization to immutable per-task actor grants, with persisted scope snapshots and denial decisions.

- Reconcile the current implementation matrix with verified code; document security boundaries and reproducible benchmark limitations.

- Add reviewed, reproducible same-model comparison profiles with optional larger baseline, per-arm resource totals, expected-answer isolation and durable run cancellation.

- Route inference through a bounded FlatBuffers module broker with trusted registration, discovery, version pinning, identity checks, backpressure and cancellation.

### Development: independently verify dependency-order answers

- Validate explicit original-request ordering constraints with a bounded Rust checker.
- Feed independent failure diagnostics into bounded solver revisions, stop on success
  or repeated answers, and preserve the exact verified result through presentation.
- Audit and budget every verification invocation; document the limited proof scope.

### Development: preserve rejected verification outcomes

- Keep failed checked candidates unchanged and mark them rejected through task results,
  typed progress, explicitly published history and reviewed retention.
- Fix progress reporting for rejected arithmetic expressions and test zero-call malformed
  criteria, single-call rejection and explanation requests after failed checks.

### Development: compare independently checked dependency orders

- Accept versioned dependency-order comparison criteria and score alternative valid
  answers fairly across all baseline and collective profiles.
- Validate the complete dataset before creating tasks and identify criteria in reports.
- Preserve existing exact-match datasets and test the full six-arm loopback HTTP path.

### Development: diagnose local storage and inference configuration

- Add operator diagnostics with optional bounded loopback model discovery, explicit
  missing-model reporting and meaningful failure exits without launching a task.
- Document initialization effects and distinguish model advertisement from inference,
  loaded weights, residency and GPU validation.

### Development: prepare reviewed RAG candidates from recorded gaps

- Inspect bounded capability-gap evidence and report reuse/review steps without granting permissions.
- Import reviewed gap-guided RAG artifacts with atomic candidate/provenance records
  and evidence hash revalidation; protect source-task evidence from retention.

### Development: measure bounded inference queue waiting

- Measure broker queue delays independently of backend claims and retain optional
  timing through compatible module/socket schemas, task traces and comparison totals.
- Preserve unavailable metrics on cancellation and legacy frames; test contention
  and reject invalid metric values without increasing runtime budgets.

### Development: invoke bounded versioned prompt/workflow experts

- Register and activate immutable reviewed workflow checklists alongside reference skills.
- Invoke at most one routed checklist under shared budgets and task permissions,
  retaining version/checksum audits and reserving whole-goal reasoning capacity.
- Keep workflow advice unverified and isolate it from baseline comparison profiles.

### Development: back up consistent SQLite state

- Publish bounded, private, checksummed SQLite snapshots including committed WAL
  data using the backup API and serialized operator admission.
- Verify copy/reopen behavior, permissions, contention and capacity failures while
  keeping configuration/artifacts separate and never replaying saved tasks.

### Development: stream bounded task progress over local sockets

- Add owner-authenticated `watch-task` with durable cursor catch-up and typed live
  events, JSONL output, terminal states and resumable timeouts.
- Bound watcher admission, queues and writes; report loss explicitly and preserve
  task execution when an observer disconnects.

### Development: report optional Rust process memory

- Sample bounded Linux procfs RSS and process-lifetime high-water RSS at task
  completion and in diagnostics; preserve unavailable values through IPC.
- Aggregate comparison memory as maxima and document scope, warm-up effects and
  exclusion of inference-server/GPU memory.

### Development: bound and audit the offline training experiment

- Fail when truncation leaves no supervised examples instead of looping forever.
- Freeze and hash validated dataset input, reject conflicting/ambiguous provenance,
  reserve separate output and record seeded configuration/dependency versions.
- Hash bounded inactive adapter bundles; add standard-library preflight CI tests
  while keeping neural evaluation/promotion disabled.

### Development: execute bounded SQL over task-local inline data

- Add deterministic SELECT tasks with isolated in-memory tables, audited task
  permissions and no inference requirement.
- Deny external access, mutations, unsafe functions and multiple statements;
  bound VM work, cancellation, data sizes and results, with hashed evidence.

### Development: run dependency-ordered expert advice workflows

- Extend immutable manifests with bounded acyclic advice steps while preserving
  legacy hashes and shared-backend behavior.
- Execute declared dependencies under shared budgets, audit step/result identities,
  and keep advice unverified and isolated from baseline profiles.

### Development: restore verified database snapshots into new workspaces

- Add operator-only checksum/integrity-checked database restoration with private
  staging, no-overwrite publication and a durable provenance record.
- Refuse existing destinations, unsupported schemas and live WAL inputs; retain
  task state without automatic replay or configuration/artifact substitution.

### Scoped operator inference authentication

- Support opt-in environment bearer credentials for one local API scope across
  generation, discovery and managed workers without persisting secret values.
- Preserve backward-compatible typed bootstrap and reject literal or JSON-escaped
  provider credential echoes before task results are recorded.

### Authentication boundary regressions

- Verify scoped primary and registered discovery with actual CLI requests, including
  absence of authorization on other scopes and safe escaped-echo failures.
- Verify missing credentials prevent direct and managed connections and model
  actors cannot authorize authentication configuration.

### Paired comparison changes

- Report per-arm case improvements and regressions against small-model-alone,
  with paired coverage, explicit denominators and unavailable empty rates.
- Exclude unexecuted deadline/cancellation cases from comparisons and expose their
  count instead of treating missing outcomes as successes or failures.

### Checked configuration restoration

- Include bounded effective runtime configuration in checksummed SQLite backups
  without persisting workspace paths or environment credentials.
- Add explicit restore-configuration opt-in with validated settings, private
  no-overwrite output and backward-compatible database-only defaults.

### RAG artifact recovery

- Add explicit RAG payload capture and restoration with hashes, bounded deduplication,
  private read-only quarantine and transactional candidate path rebasing.
- Reject missing/tampered/oversized payloads and candidate triggers; preserve candidate
  status and keep neural weights outside backups. Default backups omit payloads.

### Explicit snapshot schema migration

- Restore recognized schema-1–5 snapshots only with migrate-schema opt-in, applying
  bounded transactional migrations to staging without modifying the source.
- Record source/result schemas and preserve running tasks without replay; reject
  unknown versions and publish nothing on migration failure.

### Versioned event module routing

- Register/discover bounded host event modules and pin task/stage subscriptions to
  immutable versions while retaining in-flight subscriptions across replacement.
- Filter before shared bounded queue admission and preserve explicit overflow and
  durable catch-up without granting model observation or execution permissions.

### Event observer cancellation and recovery

- Close pending/buffered observations explicitly without cancelling task execution,
  releasing queue capacity while handles remain alive.
- Replay durable progress through the module's pinned task/stage filter, preserving
  pagination cursors across replacement, overflow and cancellation.

### Bounded complete audit inspection

- Add task-scoped task-events pagination with durable event sequences and stable
  cursors, preserving full histories without unbounded trace allocation.
- Refuse oversized/malformed full traces explicitly and read comparison model
  identities from bounded selected audit records.

### Durable inference reservation correlation

- Record charged call identity with reasoning results and expose a bounded,
  transactional inference-audit summary for recovery inspection.
- Validate result identity/hashes and preserve unknown legacy or unrecorded usage
  without refunding reservations, exposing answers or authorizing replay.

### Durable task execution settings

- Validate embedded runtime configuration before task creation and persist settings,
  effective reasoning ceilings and execution options before model dispatch.
- Fail closed on snapshot writes and preserve original settings without archiving
  workspace paths, environment credentials or restorable permission grants.

### Validated advisory graph recovery inspection

- Add operator `graph-audit` inspection of durable plans and subtask histories.
- Reject changed definitions, invalid transitions and premature dependent execution;
  report hashes and recorded states while keeping model results unverified.
- Preserve missing state explicitly without granting resumption permissions.

### Original inference budget reconstruction

- Validate durable execution settings and mode ceilings during inference auditing.
- Report remaining inference admission allowances while retaining all charges for
  unrecorded calls; reject histories exceeding their original saved limits.
- Keep unproven time/cycle/tool allowances unavailable and resumption unauthorized.

### Durable tool and reasoning-cycle charges

- Persist versioned reservations before tool admission and reasoning cycles;
  stop execution when reservation writes fail.
- Reconstruct remaining tool/cycle allowances conservatively for versioned ledgers,
  rejecting forged, discontinuous or over-budget charges.
- Keep older histories unknown and preserve deny-by-default permissions.
- Enforce the original cycle ceiling across repeated solve calls on one Session.

### Shared task deadlines

- Charge setup time to the original task wall budget and share its monotonic
  deadline with reasoning sessions.
- Reject expired tool/cycle/inference admission and cap inference timeouts by
  remaining task time, preserving cancellation priority.

### Model registry read integrity

- Reject model discovery entries whose embedded identities differ from stored row
  identities, even when their manifest checksums are valid.
- Bound stored manifest, identifier, version and checksum reads before Rust
  allocation and add tampered/oversized registry regression coverage.

### Independent integer sorting verification

- Add bounded `sort:` goals with deterministic ordering and multiplicity checks.
- Route checker failures through existing bounded Jev-R revision and charge each
  verification under an explicit task-scoped capability.
- Record the actual successful criterion in Engine evidence instead of assuming
  every structured verifier is dependency ordering.

### Independently checked structured-memory reuse

- Reuse exact structured sorting/ordering answers from operator-published episodes
  only after a fresh budgeted deterministic check.
- Record current verification evidence and memory provenance, completing valid
  matches without inference while retaining incorrect answers as untrusted data.
- Preserve revocation, profile isolation and verification tool-budget denials.

### Durable assembled reasoning inputs

- Preserve exact objective/reference/advice context and hashes before planning or
  solving, independent of later source-document changes.
- Bound raw/serialized snapshots and prevent subsequent inference when persistence
  fails; keep saved external data separate from permissions.

### Checked saved-result recovery

- Add operator recovery of abandoned structured tasks from a saved solver answer,
  with a fresh independent check and no inference replay.
- Validate and charge the original resource ledger transactionally; reject live
  leases, unresolved calls, missing allowances and terminal-task rewrites.
- Preserve failed-check charges while rolling back audit-write failures atomically.

### Versioned compiled checker experts

- Support reviewed checker manifests for existing integer-sort/dependency-order
  implementations using the registry's immutable lifecycle and dependency checks.
- Pin routed checker metadata and record its version/checksum in actual proof
  events, keeping compiled criterion identity separate from manifest version.
- Reject unsupported checker identities, workflows, model bindings and permissions;
  preserve existing manifest hashes and fixed kernel authorization.
