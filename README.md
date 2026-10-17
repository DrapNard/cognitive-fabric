# Cognitive Fabric

[![CI](https://github.com/DrapNard/cognitive-fabric/actions/workflows/ci.yml/badge.svg?branch=develop%2Fv1.0)](https://github.com/DrapNard/cognitive-fabric/actions/workflows/ci.yml?query=branch%3Adevelop%2Fv1.0)
[![Rust 2021](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0--only-blue.svg)](LICENSE)

Cognitive Fabric is a local, resource-bounded framework for coordinating language models, deterministic tools, and versioned skills. The runtime is written in Rust and uses Tokio, FlatBuffers, SQLite, and replaceable OpenAI-compatible inference backends.

> **Status: pre-release alpha.** The project is an orchestration framework, not a newly trained reasoning model. Jev+ is deterministic routing and Jev-R is a bounded multi-role reasoning loop. The architecture's research claims remain hypotheses; this repository does not claim general reasoning superiority or production readiness.

## What it does

- Keeps routing, task budgets, permissions, persistence, and local IPC in the Rust runtime.
- Uses typed FlatBuffers messages over owner-restricted Unix sockets for internal process communication.
- Connects to local OpenAI-compatible inference servers through a replaceable adapter, with an offline demonstration backend for deterministic workflows.
- Provides bounded FAST, STANDARD, DEEP, and RESEARCH task modes, optional expert workflows, structured checking, and task recovery.
- Stores task state, versioned skill metadata, and reviewed knowledge in SQLite.
- Offers an optional loopback HTTP server for OpenAI-compatible clients. It proposes client tool calls; the connected client remains responsible for executing and authorizing those tools.

The design retains separate language interface, Jev+, Jev-R, expert, memory/fabric, evolution, and security/evaluation responsibilities. See the [architecture](docs/ARCHITECTURE.md) and the [current implementation snapshot](docs/CURRENT_IMPLEMENTATION.md) for the target design and what exists today.

## Quick start

### Requirements

- Rust stable (the repository pins a toolchain in `rust-toolchain.toml`)
- A C toolchain and linker for SQLite and native dependencies
- Linux or another supported Unix-like system for Unix socket workflows
- An OpenAI-compatible local inference server for model-backed tasks; optional for the demo backend

FlatBuffers Rust bindings are committed and checked against the pinned runtime version. `flatc` is optional for ordinary builds; see [protocol generation](docs/PROTOCOL.md) to regenerate them.

### Build and try the demo backend

```sh
cargo build --locked
cargo run -- ask 'calc: (7 + 5) * 3'
cargo run -- --json security-eval
```

### Connect a local model

Configure the endpoint and model ID reported by your local server:

```sh
cargo run -- configure-local \
  --model your-loaded-model-id \
  --api-base http://127.0.0.1:1234/v1
cargo run -- ask 'Explain Rust ownership briefly.' --mode standard
```

For OpenAI-compatible clients, see the [HTTP API and client setup](docs/HTTP_API.md). For credentials, managed workers, socket service use, and complete installation workflows, follow [configuration](docs/CONFIGURATION.md), [authentication](docs/AUTHENTICATION.md), [workers](docs/WORKERS.md), and [installation validation](docs/INSTALLATION_VALIDATION.md).

## Security and deployment scope

The default runtime is local and single-user. The optional HTTP server binds to loopback and does not provide multi-tenant isolation. Do not expose it directly to an untrusted network. Built-in process workers provide coordination and bounded permissions, not a general sandbox for hostile code. The runtime cannot prove that a provider stopped GPU work after cancellation, and local policy can be removed by a fork.

Prompts, retrieved content, model outputs, datasets, and skill artifacts are treated as untrusted. Model output does not grant permissions. RAG retrieval checks do not establish neural safety. The threat model records a live reference-content prompt-injection failure and the limits of its later mitigation; see [security](docs/SECURITY.md) and the [threat model](docs/THREAT_MODEL.md).

## Evaluation

The project includes fixed security-boundary probes, deterministic integration tests, and public model workflow reports. The authored corpora and reports are diagnostic evidence, not independent held-out validation or proof of reasoning gains. Review the [benchmark methodology](docs/BENCHMARKING.md), [evaluation corpus notes](evaluation/README.md), and [release audit](docs/RELEASE_AUDIT.md) before interpreting results.

Run the CI checks locally with:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

The [GitHub Actions workflow](.github/workflows/ci.yml) also validates generated bindings, CLI workflows, the optional HTTP workflow, and the offline training preflight.

## Project status

This repository is an actively developed alpha. Neural adaptation is optional and experimental; no trained Jev+ or Jev-R networks are included. General module lifecycle, model residency scheduling, independent held-out evaluation, and stronger isolation remain incomplete. The [implementation matrix](docs/IMPLEMENTATION_MATRIX.md) and [release audit](docs/RELEASE_AUDIT.md) track the current gaps.

Optional [empirical Jev+ retry evaluation](docs/EMPIRICAL_ESCALATION.md) lets trusted hosts apply reviewed success/cost evidence within the existing runtime limits. It remains experimental and requires reviewed measurements.

Trusted Rust hosts can share [inference resource pools](docs/RESOURCE_SCHEDULING.md) across backends to bound concurrent operations and declared memory allocations. Configure normal CLI and service use with `configure-resources FILE`. Optional resident-backbone declarations reserve shared model memory separately and reject unlisted configured inference targets. Physical GPU enforcement and measured residency remain incomplete. The [reviewed residency planner](docs/RESIDENCY_PLANS.md) reuses loaded contexts and proposes explicit model-set changes for operator approval.

## License

Current project source is licensed under **GNU GPL version 3 only**. See [LICENSE](LICENSE). Project-authored evaluation datasets and some examples carry their own explicit license metadata; third-party models and imported data remain under their respective licenses.

This license change applies to the current source tree. Earlier commits and tags retain the license terms recorded at those revisions; this change does not withdraw permissions already granted for them.

Optional [reviewed workflow routing](docs/EMPIRICAL_WORKFLOW_ROUTING.md) ranks
expert workflows using bounded paired evidence; no real quality gain is claimed.

Use `ask` or `serve` with `--workflow-study STUDY --workflow-thresholds LIMITS`
to explicitly pin a reviewed workflow policy for that host.
