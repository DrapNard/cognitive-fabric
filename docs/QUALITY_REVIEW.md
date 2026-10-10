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
