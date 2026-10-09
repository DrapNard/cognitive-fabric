# Cognitive Fabric — Agent Engineering Instructions

Project owner: **DrapNard**  
Primary implementation: **Rust**  
Current development branch: **`develop/v1.0`**  
Target: **a working, measurable, security-conscious v1.0 framework**, not a claim of having trained a new general-purpose reasoning model.

## 1. Read the project's source of truth first

Before designing, coding, or refactoring a feature, read:

1. `docs/ARCHITECTURE.md` — **authoritative French project vision and target architecture**. Read the full document, not only its diagram or roadmap.
2. `docs/CURRENT_IMPLEMENTATION.md` — snapshot of the existing Rust alpha; this is **descriptive**, not the target specification.
3. `docs/IMPLEMENTATION_MATRIX.md`, `docs/RUST_PORT.md`, and `docs/THREAT_MODEL.md` if present — existing progress, known issues, and security assumptions.
4. Relevant source, tests, and `Cargo.toml` before proposing changes.

**Do not replace the complete French architecture with a short implementation diagram.** Preserve it verbatim unless the project owner asks to change the vision. The existing implementation may lag behind the specification. Treat such gaps as work to track, not as permission to silently delete requirements.

If the French vision and these engineering instructions appear to conflict, preserve the vision's core intent and hard security boundaries; document the issue and choose the least risky, testable implementation. Distinguish *v1.0 scope* from *future neural research* explicitly.

## 2. Language and project conventions

- All **source code, identifiers, comments, commits, tests, ordinary developer documentation, and CLI output** must be in **English**.
- `docs/ARCHITECTURE.md` is deliberately **in French**: it is the original canonical specification and is an exception to the English-documentation rule. Do not automatically translate, overwrite, or truncate it.
- Prefer Rust 2021 conventions unless a deliberate, tested edition migration is approved.
- Use small, cohesive modules with explicit ownership and interfaces. Avoid gratuitous abstraction and large monolithic files.
- Never commit secrets, model weights, large datasets, or generated files without an explicit reason.

## 3. Preserve the seven subsystems

The architecture has **seven replaceable layers**; do not collapse them into a generic chat agent:

| Subsystem | Required responsibility |
| --- | --- |
| Language Interface | Parse human goals and constraints; explain structured results using a small local LLM |
| Jev+ Collective | Route work, schedule tasks, allocate budgets, discover capabilities, decide escalation |
| Jev-R Collective | Planner, Solver, Explorer, Critic, Verifier, Reflector; bounded multi-step reasoning |
| Expert Network | Versioned, dynamically discoverable skills and domain-specialized backends |
| Cognitive Fabric | Typed messaging, task state, shared evidence, persistent memory, traces |
| Evolution Engine | Discover gaps; build and evaluate candidate RAG skills; optional offline neural adaptation |
| Security & Evaluation Kernel | Independent tool permissions, isolation, limits, review gates, reproducible evaluations |

Jev+ and Jev-R are **conceptually separate** even when early implementations reuse one inference backend. Their roles, interfaces, and measurements must stay distinct.

Do not claim these modules are new trained neural networks when they are heuristics, tools, or repeated LLM calls. Future TRM/HRM-inspired or structure-agnostic neural modules are **research goals**, not v1.0 proof obligations.

## 4. Non-negotiable technical architecture

- **Rust + Tokio** own runtime orchestration, concurrency, task lifecycle, routing, protocol handling, memory, policies, and CLI/service entry points.
- **FlatBuffers** is the preferred **typed internal IPC and event protocol**. Keep versioned `.fbs` schemas and test compatibility, length bounds, validation, and malformed frames. Do not silently replace the internal protocol with JSON or MessagePack.
- **Unix domain sockets** are the initial local process transport. Enforce safe socket permissions and limits; remote IPC requires a separate security design.
- **SQLite** stores task state, model/skill registry, audit metadata, and retrievable knowledge, with migrations and safe concurrent access.
- External **OpenAI-compatible LLM HTTP APIs may use JSON**, because that is the provider's interface. Internal IPC is still FlatBuffers.
- **Python is restricted to optional isolated/offline model training and research** (for example PyTorch/PEFT). Never migrate the Rust orchestrator back to Python.
- Integrate local, configurable inference backends (e.g. LM Studio or llama.cpp-compatible servers) through a replaceable adapter. Provide an offline mock backend for deterministic tests.
- Reuse existing model weights and lightweight skills before proposing training or additional always-resident models.

## 5. Reasoning and resource discipline

- Implement **FAST**, **STANDARD**, **DEEP**, and **RESEARCH** modes with explicit, enforced limits.
- Each task has bounded model calls, tokens, recursive iterations, tool invocations, wall time, and queue capacity; track resource metrics where supported.
- Jev-R may revise a failed answer, but every recursion must have an evidence-aware stopping rule and a hard cap.
- An additional solver or critic is invoked only when its expected utility justifies its actual inference and coordination cost.
- Share backbones, configurations, and caches when feasible. Hot-swap skills without breaking in-flight tasks; pin each task to the versions it started with.
- Compare performance with a **single small LLM baseline**, the same model with tools, recursive reasoning, and larger models under comparable budgets. Never invent benchmark results or equivalence to GPT Terra.

## 6. Learning and evolution policy

- Prefer progression: **reuse existing skill → retrieval/memory → RAG candidate → offline LoRA experiment → architecture research**.
- Treat datasets, repositories, prompts, and model output as **untrusted**. Track licensing, hashes, provenance, deduplication, and evaluation splits.
- Store trained adapters as versioned, immutable candidates. Do not mutate active model weights in place.
- Evaluate candidates using independent, held-out tests, regression tests, and security checks.
- **A RAG retrieval benchmark is not a neural safety evaluation.** Do not use it to approve neural weights.
- No model may rewrite its own test suite, alter acceptance thresholds, grant new capabilities, or self-promote sensitive models.
- Support rollback and auditable operator approvals. Neural promotion remains gated until independent evaluators genuinely exist.

## 7. Security and threat model

Security policy is enforced by deterministic Rust services **outside trainable models**.

- Deny-by-default capabilities: each tool invocation is individually authorized for its task and actor.
- No arbitrary shell execution, unrestricted network access, filesystem traversal, secret access, or production modification by default.
- Use strict parsing, validated FlatBuffers, bounded buffers, timeouts, cancellation, and backpressure. Do not trust an in-process model just because it uses local IPC.
- Keep external documents and retrieved memories separated from instructions and permissions.
- Isolate untrusted code and training processes. Containers are not a complete sandbox; higher-risk workloads require stronger isolation.
- Log policy decisions and changes without storing credentials or leaking sensitive content.
- A fork can remove built-in policies; **never claim that local restrictions guarantee safety after open-source distribution**.
- Any material increase in autonomy or sensitive capability needs a separate review before public release.

## 8. Development workflow and Git

Work on **`develop/v1.0`** (or a feature branch based on it). Preserve history, existing data, and uncommitted human changes.

For each logical feature:

1. Identify the corresponding specification section and record its implementation status.
2. Inspect existing interfaces and tests; prefer a small vertical slice.
3. Implement functionality with explicit error handling and bounded resource use.
4. Add meaningful unit, integration, negative, and regression tests.
5. Run the relevant checks, fix failures, and review correctness, security, architecture, readability, and performance.
6. Update `docs/IMPLEMENTATION_MATRIX.md` with **Implemented / Partial / Experimental / Not implemented**, relevant file paths, tests, and known blockers.
7. Commit the completed change with an **English imperative commit message** that explains the behavioral improvement.

Do not squash away historical commits, force-push, publish a remote repository, or create a final release tag without explicit authorization. Do not reformat unrelated files or remove tests to hide failures.

## 9. Mandatory checks before claiming readiness

From the repository root, run (or truthfully report why impossible):

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Also validate the exact `flatc`/Rust FlatBuffers generator pairing, socket integration, database migrations, mock inference, malformed frames, policy denials, cancellation, and candidate rollback. Run real-backend inference separately when a local server is available.

A passing mock test suite does **not** imply GPU integration, neural reasoning quality, real model switching, or safety is proven. Record what was actually run.

## 10. v1.0 release gate

Only mark v1.0 as complete when the Rust workspace builds, mandatory tests and quality checks pass, the CLI/service run end-to-end, typed transport is verified, bounded reasoning and skill registry work, persistence is reliable, and permission checks have negative tests. Provide reproducible documentation and honest benchmarks.

Optional offline LoRA integration may remain experimental if it is clearly separated and not misrepresented as automatic secure model training. Novel Jev-R neural architectures, self-rebuilding weights, and general superiority to large LLMs are **research beyond proven v1.0 functionality**.

If a requirement is incomplete: **do not fake it, stub it as success, or tag it as complete**. State what works, what is untested, what is blocked, and what remains to build.

## 11. Output expectations for every substantial coding run

Report concisely in English:

- Implemented features and files changed.
- Commands executed and their actual results.
- Security and performance implications.
- Documentation / implementation matrix updates.
- Git commit identifiers (if created).
- Remaining blockers and next verifiable engineering milestone.

**Guiding rule:** many available capabilities, few active parameters, measurable reasoning, and permissions that are independent from model intelligence.
