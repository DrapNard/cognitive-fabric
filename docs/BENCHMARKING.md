# Reproducible evaluation

Run `cogfabric --home STATE --json bench DATASET --mode MODE`. DATASET is UTF-8
JSONL with `input` and `expected` strings, at most 1,000 cases / 2 MB. Each case is
bounded before execution. The evaluator compares whitespace-normalized,
case-insensitive exact output with the supplied expected result; model confidence
is never the acceptance criterion. Keep test data separate from training data.

The report includes per-case task IDs, answer, runtime status, expected-output
result, latency, calls, optional provider token usage, measured inference time,
reserved token/prompt/tool resources and stop reason. Missing provider usage is
unavailable rather than zero. Benchmark acceptance does not rewrite a task's
runtime verification status. `task ID` retrieves persisted traces.

`examples/arithmetic_bench.jsonl` is a deterministic parser smoke benchmark. It
uses no model inference, so its accuracy/latency cannot establish collective
reasoning quality or advantage over a language model. RAG held-out tests exercise
isolated retrieval and active-baseline regressions; see `EVOLUTION.md`.

The required controlled study remains incomplete: small-model alone, same-model
with tools, recursive reasoning, collective, collective with memory, specialized
experts, and an optional larger baseline. Use the same held-out technical tasks,
models/temperatures, explicit resource ceilings and independent expected criteria;
record unequal resource consumption rather than hiding it. Record dataset hashes,
source/license/split, configuration, code revision and backend identity with each
run. Avoid training/evaluation contamination and repeated benchmark tuning.

The canonical initial suite requires 100–200 diverse technical/reasoning tasks,
including ambiguity, unseen cases and objectives requiring tools. This dataset and
multi-arm runner are not yet implemented. GPU memory, peak process memory, energy,
model-load costs and robust comparative regression/security metrics are currently
unavailable. No performance-superiority or larger-model equivalence is claimed.
