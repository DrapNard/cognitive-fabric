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
