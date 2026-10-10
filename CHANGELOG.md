# Changelog

All notable changes are documented here in English.

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
