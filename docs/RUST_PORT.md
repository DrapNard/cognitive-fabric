# Rust Port Audit

## What changed

- Replaced `cogfabric/*.py` runtime with a Rust binary/library.
- Replaced MessagePack transport with a versioned, typed FlatBuffers schema.
- Added Tokio Unix socket server/client with bounded frames.
- Migrated SQLite knowledge retrieval, task traces, skills, RAG candidates,
  candidate integrity/quality checks and human-gated promotion.
- Preserved optional PyTorch LoRA training under `training/`, offline only.
- Kept the v1.0 Python implementation in immutable Git history / original tag.

## Verification status

The generation environment lacked `cargo`, `rustc` and `flatc` and could not
reach a package repository. Therefore **this port has not been compiled or
executed** here. It is tagged as alpha rather than an audited v1.0 release.
The GitHub Actions workflow is configured to build, lint and test once the
repository is run in a Rust-enabled CI environment.

## Known limitations

- No trained Jev+ or Jev-R model, only orchestration roles.
- No hardware usage metering or energy model.
- No neural adapter hot swap, autonomous training or self-reconstruction.
- Local-only, one-user runtime; no hardened multi-tenant authentication.
- Candidate evaluation checks RAG retrieval quality only.
- The project pins FlatBuffers crate and CLI to v25.12.19.
- Port parity and all compilation errors need verification on a Rust runner.
- FlatBuffers is used only on IPC; HTTP LLM APIs require JSON by design.

## First required quality gates

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo run -- --home ./test-state self-test
```

Failing any command blocks promotion from alpha to stable.

## Verification update (2026-10-10)

The earlier environment limitation has been resolved locally. Rust 1.97.1 and
flatc 25.12.19 build the workspace; 14 tests, Clippy and the CLI self-test pass.
`FLATC` may name a locally built compiler. The version check requires an exact
match. See `QUALITY_REVIEW.md`; the implementation remains alpha because
integration and architectural release requirements are incomplete.
