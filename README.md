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
- Includes operator-approved task model lifecycle, native response-instance checks, versioned typed modules, bounded Wasm experts and secondary verifiers, and immutable project profiles. See the implementation snapshot for verification evidence and remaining limits.
- Offers an optional loopback HTTP server for OpenAI-compatible clients. It proposes client tool calls; the connected client remains responsible for executing and authorizing those tools.

See [typed modules](docs/TYPED_MODULES.md), [optional Wasm experts](docs/WASM_EXPERTS.md), [project profiles](docs/PROJECT_PROFILES.md), [managed residency](docs/MANAGED_RESIDENCY.md), and [verification](docs/VERIFICATION.md) for the new interfaces and trust boundaries.

The design retains separate language interface, Jev+, Jev-R, expert, memory/fabric, evolution, and security/evaluation responsibilities. See the [architecture](docs/ARCHITECTURE.md) and the [current implementation snapshot](docs/CURRENT_IMPLEMENTATION.md) for the target design and what exists today.

## Quick start

### Requirements

- Rust stable (the repository selects the stable channel in `rust-toolchain.toml`; it does not pin an exact compiler release)
- A C toolchain and linker for SQLite and native dependencies
- Linux or another supported Unix-like system for Unix socket workflows
- An OpenAI-compatible local inference server for model-backed tasks; optional for the demo backend

FlatBuffers Rust bindings are committed and checked against the pinned runtime version. `flatc` is optional for ordinary builds; see [protocol generation](docs/PROTOCOL.md) to regenerate them.

### Install the command-line tools

The repository does not currently publish binary release assets. Download and
review the installer and selected revision before running it. This inspect-first
command builds the standard CLI, chat CLI, and optional HTTP server into
`$HOME/.local` without `sudo`:

```sh
curl -fsSLo setup.sh https://raw.githubusercontent.com/DrapNard/cognitive-fabric/develop/v1.0/scripts/setup.sh
less setup.sh
sh setup.sh --ref develop/v1.0 --prefix "$HOME/.local"
```

The development branch can move; for a reproducible build, use the same full
reviewed commit ID in the script URL and `--ref`. Existing binaries are
preserved unless `--replace` is explicitly supplied. See the complete
[installation guide](docs/INSTALL.md) for prerequisites, revision pinning, and
verification steps. If you trust the current development branch and want the
one-line form, see that guide for the `curl | sh` command.

To install and set Qwen as the language interface while Jev-R uses only compiled
solvers, pass `--language-model MODEL_ID --jev-r-only` to `setup.sh`. The
language route is written to the local config after the binaries install; the
installer does not load models.

### Chat alongside an agentic client

Use `cogfabric-chat` for text-only conversation while OpenCode or another agentic
client uses the same local server. It keeps bounded multi-turn context and never
executes tools. See [chat setup and limits](docs/CHAT.md) and
[OpenCode configuration](docs/HTTP_API.md#opencode-v2-configuration). After a reboot, start your
model provider, then run `cogfabric-opencode`; see the
[startup and troubleshooting guide](docs/OPENCODE.md).

Inside the chat, use `/effort` to show the current reasoning setting, or
`/effort low`, `/effort medium`, `/effort high`, `/effort none`, and
`/effort default` to change it for subsequent turns. The model stays the same.
OpenCode uses per-request reasoning effort through its configured variants.

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

For OpenAI-compatible agentic clients, see [client-independent integration](docs/AGENT_CLIENTS.md) and the [HTTP API](docs/HTTP_API.md). For credentials, managed workers, socket service use, and complete installation workflows, follow [configuration](docs/CONFIGURATION.md), [authentication](docs/AUTHENTICATION.md), [workers](docs/WORKERS.md), and [installation validation](docs/INSTALLATION_VALIDATION.md).

## CPU and GPU execution

Jev+ coordination, Jev-R orchestration, deterministic verification, memory and IPC
run on CPU. CUDA is not required for the default build or offline workflows.
For model-backed reasoning, the configured inference server owns CPU/GPU
execution and offloading; the Rust runtime does not implement CUDA inference.

`diagnostics --probe-gpu` and `compare --observe-gpu` provide optional NVIDIA
device observations. These do not prove GPU inference, task-attributed energy or
physical GPU quota enforcement. Build optional NVML support with
`--features nvml-gpu` or install using `setup.sh --nvml-gpu`. See
[GPU observations](docs/GPU_OBSERVATIONS.md). Approved Engine
[lifecycle policies](docs/NATIVE_ON_DEMAND.md) can load models on first use, reuse
validated instances and release only instances loaded by that lifecycle.
CPU execution support is distinct from the still-unproven live reasoning and
efficiency claims.

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

See the [delivery roadmap](docs/ROADMAP.md), including optional image, audio and
tentative video understanding after core completion. The opt-in [client
collective](docs/CLIENT_COLLECTIVE.md) now persists bounded reasoning state across
correlated client tool turns; broader Engine expert and project-profile integration
remains incomplete.

This repository is an actively developed alpha. Neural adaptation is optional and experimental; no trained Jev+ or Jev-R networks are included. General module lifecycle, model residency scheduling, independent held-out evaluation, and stronger isolation remain incomplete. The [implementation matrix](docs/IMPLEMENTATION_MATRIX.md) and [release audit](docs/RELEASE_AUDIT.md) track the current gaps.

Optional [empirical Jev+ retry evaluation](docs/EMPIRICAL_ESCALATION.md) lets trusted hosts apply reviewed success/cost evidence within the existing runtime limits. It remains experimental and requires reviewed measurements.

Trusted Rust hosts can share [inference resource pools](docs/RESOURCE_SCHEDULING.md) across backends to bound concurrent operations and declared memory allocations. Configure normal CLI and service use with `configure-resources FILE`. Optional resident-backbone declarations reserve shared model memory separately and reject unlisted configured inference targets. Optional Linux [worker kernel limits](docs/WORKER_LIMITS.md) constrain CPU time and virtual address space of managed Cognitive Fabric processes. Physical GPU enforcement and measured residency remain incomplete. The [reviewed residency planner](docs/RESIDENCY_PLANS.md) reuses loaded contexts and proposes explicit model-set changes for operator approval.

## License

Current project source is licensed under **GNU GPL version 3 only**. See [LICENSE](LICENSE). Project-authored evaluation datasets and some examples carry their own explicit license metadata; third-party models and imported data remain under their respective licenses.

This license change applies to the current source tree. Earlier commits and tags retain the license terms recorded at those revisions; this change does not withdraw permissions already granted for them.

Optional [reviewed workflow routing](docs/EMPIRICAL_WORKFLOW_ROUTING.md) ranks
expert workflows using bounded paired evidence; no real quality gain is claimed.

Use `ask` or `serve` with `--workflow-study STUDY --workflow-thresholds LIMITS`
to explicitly pin a reviewed workflow policy for that host.

Verifier-R supports [finite-domain constraint assignments](docs/VERIFICATION.md#finite-domain-assignments)
through explicit `csp:` objectives, alongside sorting, ordering and path checks.

Native lifecycle policy and uncertainty handling: [operator guide](docs/NATIVE_LIFECYCLE.md).

Optional [exact input-token admission](docs/INPUT_TOKEN_ADMISSION.md) supports
llama.cpp native text inference with persistent per-task input allowances.

Operator-approved [remote source acquisition](docs/SOURCE_ACQUISITION.md) fetches
hash-pinned HTTPS text into unreviewed local preparation bundles.
