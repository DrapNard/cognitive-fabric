# 1.0 code review

## Correctness

- Automated protocol, routing, math-sandbox, retrieval, candidates, daemon, and
  trainer-input validation tests are present.
- Release smoke benchmark uses ten deterministic arithmetic expressions. These
  assess the plumbing of verifiable tool calls, NOT LLM reasoning quality.
- Neural inference and LoRA training were not executed against a real pretrained
  model in the build environment. They need external integration tests.

## Maintainability

- Dedicated modules for transport, persistence, policy, routing, inference,
  evaluation, orchestration and CLI.
- Direct explicit APIs rather than a general-purpose plugin engine that cannot
  be validated. New skills can be registered by the CLI.
- User-visible documentation and all source code are in English.

## Architecture

- Shared LLM backend, bounded logical multi-role reasoning, local operator CLI.
- MessagePack selected as FlatBuffers-equivalent binary serialization. No false
  FlatBuffers runtime claim.
- Neural Jev-like training and weight switching remain future research.

## Security

- Data from storage is treated as references; no shell or remote execution tools.
- Candidate artifact must be present, checksum matching and explicitly approved.
- Promotion of neural LoRA candidates is disabled.
- Local socket is `0600`, state directory `0700`; same-user compromise is outside
  this minimal threat boundary.
- Needs outside audit before any sensitive production usage.

## Performance

- Total output-token allocations, role call count, recursion cycles and
  wall-clock duration are bounded.
- Accuracy, model calls and wall-time can be measured by the benchmark CLI.
- No GPU utilization, energy, true FLOPs or Pareto comparison included.

## Must-fix before claiming a neural 1.0 system

- Real-world model benchmark and matched-budget ablation comparisons.
- Tested offline LoRA training with held-out datasets and evaluation of the exact
  candidate weights, not only RAG retrieval.
- Stronger sandbox and human security review for dynamic model/tool execution.
- Correct candidate-specific version pinning across in-flight tasks.

## Verification performed on 2026-10-09

- Test suite: 30/30 passing on Python 3.13.
- Python bytecode compilation: passed.
- End-to-end CLI skill promotion and retrieval: passed.
- Local Unix socket and fake OpenAI-compatible HTTP endpoint: tested.
- Editable installation with `--no-build-isolation` in offline environment:
  passed; normal isolated build requires access to package indexes.
- Optional LoRA trainer: dataset validation dry run only; no GPU training.
- Mutation experiment: inverted capability check caused the privilege separation
  test to fail; source restored and full suite passed again.
