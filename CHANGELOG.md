# Changelog

All notable changes are documented here in English.

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
