# Cognitive Fabric 1.0

**Author:** DrapNard  
**Status:** functional research runtime, not a pretrained Jev model or AGI  
**License:** Apache-2.0 (framework only)

Cognitive Fabric is a local-first, resource-bounded, modular AI coordination
runtime inspired by Jev-style decision engines. It separates language models,
routing, reasoning roles, expert skills, evaluation and security controls.

## Features

- Versioned **binary MessagePack** envelopes and local Unix socket transport.
- Persistent SQLite task logs, skill registry, text retrieval and trace events.
- Single-backbone logical roles: planner, solver, explorer, critic.
- FAST / STANDARD / DEEP / RESEARCH budgets with bounded model calls.
- Local inference through LM Studio / llama.cpp-compatible chat-completions API.
- Explicitly labelled demo mode which runs without an LLM.
- Quality benchmarks, integrity checks and human-gated promotion of artifacts.
- Manual offline LoRA/QLoRA training recipe (optional dependencies).
- Zero automatic shell command execution, crawler access or weight promotion.

**Important:** v1 implements *orchestration roles*, not trained Jev-R neural
models. Passing unit tests does not establish intelligence or deployment safety.
It uses MessagePack instead of FlatBuffers because the latter runtime compiler
is not available in the delivery environment. A future `.fbs` schema is included.

## Installation

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e '.[dev]'
python -m cogfabric init
python -m cogfabric self-test
pytest -q
```

If dependencies are already installed and network access is unavailable:

```bash
PYTHONPATH=. python -m cogfabric --home ./.local init
```

## Example commands

```bash
# Deterministic verification; does not use an LLM.
cogfabric ask 'calc: (7 + 5) * 3'

# Run reproducible functional benchmarks.
cogfabric --json bench examples/arithmetic_bench.jsonl

# Enable a LOCAL inference server (LM Studio: http://127.0.0.1:1234/v1).
export COGFABRIC_BACKEND=openai-local
export COGFABRIC_MODEL=your-loaded-model-id
export COGFABRIC_API_BASE=http://127.0.0.1:1234/v1
cogfabric ask 'Plan a minimal Rust async service' --mode deep

# Ingest a human-reviewed file, then retrieve it.
cogfabric ingest examples/skill_document.md --skill rust.s1 --source original-demo --license CC0-1.0
cogfabric search 'Rust async mutex'

# Extend the registry without modifying the Python source.
cogfabric skill-add rust.async.tokio.s1 --description 'Tokio async specialization' --tags tokio,async,rust

# Start the Unix-socket service in terminal A.
cogfabric serve
# Request from terminal B.
cogfabric ask 'calc: 10 * 12' --socket

# Create and evaluate a candidate artifact. Neither step activates it.
cogfabric candidate create --skill rust.s1 --kind rag \
  --artifact examples/skill_document.md --source original-demo --license CC0-1.0
cogfabric candidate list
cogfabric candidate evaluate CANDIDATE_ID --suite integrity
cogfabric candidate evaluate CANDIDATE_ID --suite quality --dataset examples/retrieval_bench.jsonl
# Human review is mandatory. This command records that approval.
cogfabric candidate promote CANDIDATE_ID --human-approve
```

Note: retrieval quality uses candidate-only FTS5 tests on the specified
artifact, not generative model quality or semantic safety. Use a genuinely
held-out domain-specific benchmark in practice. Approved RAG artifacts become
searchable after promotion. LoRA promotion remains blocked until a neural
candidate evaluator and separate security review are implemented.

## Optional LoRA trainer

```bash
pip install -e '.[training]'
python training/train_lora.py \
  --base-model /path/to/local/hf-model \
  --dataset examples/training_examples.jsonl \
  --output /path/to/new/adapter \
  --dry-run
```

Remove `--dry-run` to train. For `--qlora`, install `bitsandbytes` and use a
supported CUDA setup. No automatic fine-tuning or deployment is performed.
Model safety, license provenance and compatibility must be independently reviewed.

## Limitations

- The `demo` backend is a deterministic smoke-test stub, not an intelligent LLM.
- No automatic dataset harvesting, neural model training or live self-modification.
- LoRA trainer exists as an unverified optional script, not a guaranteed end-to-end neural deployment pipeline.
- The included integrity evaluation is NOT a security evaluation of weights.
- Latency/call budgets are measured; energy and VRAM instrumentation are TODO.
- The daemon handles one task at a time to protect the SQLite store and VRAM.
- The runtime is single-user and local-only, not a multi-tenant API.

See [Architecture](docs/ARCHITECTURE.md) and [Threat Model](docs/THREAT_MODEL.md).
