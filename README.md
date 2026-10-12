# Cognitive Fabric — Rust Port (1.1.0-alpha.1)

**Author:** DrapNard  
**Language:** Rust (runtime), Python (optional offline model training)
**Status:** **Verified Rust alpha baseline** — builds and 343 regression tests pass; real local Ollama generation and independently checked sorting have succeeded. v1.0 integration requirements remain incomplete.

Optional managed inference workers: use `configure-worker --enabled true` to run
each task's inference adapter in a host-authenticated child process. See
[managed workers](docs/WORKERS.md) for limits, cancellation and remaining gaps.
**License:** Apache-2.0 (framework only)

This branch corrects the original v1.0 implementation's technology mismatch:
the production runtime is Rust with Tokio, typed FlatBuffers IPC, SQLite,
a local inference adapter, bounded cooperative roles and an explicit security
policy. Python is **not** used by the runtime.

This is an **orchestration framework**; Jev+ is a deterministic router and
Jev-R is a multi-role orchestration loop, not a trained neural reasoning model.
It does **not** claim to outperform an 8B or Terra-class model.

## Requirements

- Rust stable (`rustup` or `pacman -S rust`)
- FlatBuffers compiler (`pacman -S flatbuffers`)
- SQLite (bundled via `rusqlite`)
- A local OpenAI-compatible inference endpoint, optional

> FlatBuffers Rust generated sources are produced by `flatc` at build time.
> This branch pins FlatBuffers v25.12.19. If your `flatc --version` is
> different, install a matching compiler or update both compiler and Rust crate
> in a reviewed dependency change.

## Install on Arch / CachyOS

```bash
sudo pacman -S --needed rust flatbuffers
cargo build --release
cargo test
./target/release/cogfabric --home ./.local init
./target/release/cogfabric --home ./.local self-test
```

## Install a standalone binary

With Rust and the matching `flatc` 25.12.19 already installed:

```bash
cargo install --locked --path . --root /tmp/cognitive-fabric-install
/tmp/cognitive-fabric-install/bin/cogfabric --home /tmp/cognitive-fabric-state init
/tmp/cognitive-fabric-install/bin/cogfabric --home /tmp/cognitive-fabric-state self-test
bash scripts/installed-smoke.sh /tmp/cognitive-fabric-install/bin/cogfabric
```

Choose new install/state directories. `FLATC=/absolute/path/to/flatc` selects a
matching compiler when the system compiler differs. Use `--offline` on the install
command only when all locked dependencies are cached; this is not a clean-machine
package installation. The workflow script requires Bash, jq, coreutils and Unix
sockets. It creates fresh temporary state, retains binary/script hashes and JSON evidence, and checks tools,
IPC, skill registration, example RAG promotion/rollback and backup restoration.
It also enables managed arithmetic, verifies audited child reaping after successful
and rejected tasks, checks SQL capability routing, and runs worker tasks after
configuration restoration and through the socket service.
It uses demo inference and operator-attested example datasets; it does not test
live model quality, independent held-out performance or neural safety.

## Commands

```bash
# No GPU, deterministic demonstration.
cargo run -- ask 'calc: (7 + 5) * 3'
cargo run -- ask 'sql:{"tables":[],"query":"SELECT 2 + 3 AS total"}' --mode fast
cargo run -- bench examples/arithmetic_bench.jsonl
cargo run -- --json security-eval

# Local LLM (e.g. LM Studio):
export COGFABRIC_BACKEND=openai-local
export COGFABRIC_MODEL=your-loaded-model-id
export COGFABRIC_API_BASE=http://127.0.0.1:1234/v1
cargo run -- ask 'Design a Tokio service' --mode deep

# Typed FlatBuffers Unix socket transport:
cargo run -- serve
# In another terminal:
cargo run -- ask 'calc: 10 * 12' --socket

# Approved knowledge ingestion and retrieval:
cargo run -- ingest examples/skill_document.md --skill rust.s1 --source demo --license CC0-1.0
cargo run -- search 'Rust async mutex'

# Candidate evaluation and human review:
cargo run -- candidate create --skill rust.s1 --kind rag \
  --artifact examples/skill_document.md --source demo --license CC0-1.0
cargo run -- candidate list
cargo run -- candidate evaluate CANDIDATE_ID --suite integrity
cargo run -- candidate evaluate CANDIDATE_ID --suite quality \
  --dataset examples/retrieval_bench.jsonl \
  --dataset-source project-example --dataset-license CC0-1.0 --held-out
cargo run -- candidate promote CANDIDATE_ID --human-approve
```

The optional offline `training/train_lora.py` remains Python/PyTorch, but
cannot hot-load weights into the Rust runtime. Neural candidate promotion is
blocked; it would require architecture-specific evaluation and an independent
security review.

## Security boundaries

The runtime does not grant models shell execution, arbitrary network access,
permission modification, dataset crawling or automatic weight deployment.
Local inference is restricted to loopback HTTP. IPC messages are verified
FlatBuffers frames capped at 256 KiB; the socket uses owner-only permissions.
Any RAG candidate requires an explicit source, license, integrity test,
held-out retrieval tests and an operator promotion command.

**Caution:** this software is research-stage; the gates are limited. In
particular, retrieval tests and file hashes do not establish semantic safety.

## Version history

- `v1.0.0` (branch `main`): original Python + MessagePack proof of concept.
- `develop/v1.0` (this branch): Rust/Tokio + FlatBuffers development; see `docs/QUALITY_REVIEW.md` for local verification.

See `docs/RUST_PORT.md` for the port's status and known limitations.

RAG rollback is available with
`cargo run -- candidate rollback OLD_CANDIDATE_ID --human-approve`.
It restores an approved older durable snapshot after integrity checks. See
[the evolution lifecycle](docs/EVOLUTION.md) for migration and scope details.

While `serve` runs, use `service-status`, `cancel-active`, or `stop` with the same `--home`. These controls use authenticated local FlatBuffers connections. One inference executes at a time; additional asks receive a busy error.

For same-model ablations, use `compare DATASET --source SOURCE --license LICENSE
--held-out --code-revision FULL_GIT_SHA`, optionally with `--larger-model ID`.
See [benchmark methodology](docs/BENCHMARKING.md); the arithmetic example is only
a runner smoke test, and a live-model advantage remains unmeasured.

Use `modules` or `modules --socket` to inspect registered inference adapter
versions and roles. The bounded internal FlatBuffers broker pins adapters per task;
this does not attest immutable neural weights or load external executable plugins.

Reusable task snapshots: see [memory publication and revocation](docs/MEMORY.md).

After a terminated process, see [interrupted task recovery](docs/RECOVERY.md).

Manage [registered local model configurations](docs/MODELS.md).

Observe [typed task progress and durable catch-up](docs/EVENTS.md).

Inspect [database limits and reviewed retention](docs/STORAGE.md).

Local servers requiring bearer authentication can use opt-in, endpoint-scoped
operator credentials. See [authentication setup](docs/AUTHENTICATION.md); secret
values are supplied through the launching environment and never saved in config.

Independent numeric-sorting verification accepts an explicit structured goal:

```bash
cogfabric ask 'sort:{"values":[3,-1,3,0],"descending":false}' --mode deep
```

A configured local model supplies the answer; Rust verifies a JSON array containing
all integers in the requested order, including duplicate counts. Ascending is the
default. Inputs allow 1–256 signed 64-bit integers. A rejected candidate may be
revised within the mode budget; a correct result records `integer_sort/v1` evidence.
The demo backend is not a sorting model and is not evidence of live-model quality.

Release gate evidence and remaining work are tracked in
[the release audit](docs/RELEASE_AUDIT.md). This is still a pre-release.

Operator-approved interrupted advisory tasks can use
`cogfabric --home STATE resume-task TASK_UUID --human-approve`. Admission preserves
original same-boot deadlines and budgets, refuses unresolved calls and only
supports default Full/Collective advisory graphs and unconsumed solver boundaries,
including FAST, whole-goal fallback and skipped-plan tasks, with direct or managed
inference and validated original model registrations. See
[recovery documentation](docs/RECOVERY.md) before using this command.


Optional arithmetic workers: `cogfabric configure-tool-worker --enabled true`
launches one task-scoped built-in Rust worker for `ask 'calc: 6*7' --mode fast`,
including socket-service asks after restart. Disable with `--enabled false`.
The host retains budget checks and independent verification; this is process
separation, not an executable sandbox. See [worker lifecycle](docs/WORKERS.md).

Optional SQL workers: `cogfabric configure-sql-worker --enabled true` runs
task-local SQL in a separate built-in Rust process. For example,
`cogfabric ask 'sql:{"tables":[],"query":"SELECT 42 AS answer"}' --mode fast`
returns independently checked results without model calls. Disable with
`configure-sql-worker --enabled false`; restart an existing service after changing
the setting. This preserves the inline-only SQL restrictions and task tool budgets.

To review abandoned managed-worker sockets on Linux, run
`cogfabric --json worker-files-preview > cleanup-plan.json`, inspect its entries,
then run `cogfabric worker-files-apply cleanup-plan.json --human-approve`.
Only journaled socket identities are eligible; active runtime leases block cleanup.
For paginated previews, pass the returned `cursor` as `--after` while `more` is
true. See [recovery](docs/RECOVERY.md) for partial-failure and filesystem limits.
