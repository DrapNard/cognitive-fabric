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

## Comparison runner

Use `compare DATASET --mode deep --source SOURCE --license LICENSE --held-out
--code-revision FULL_GIT_SHA --max-run-seconds 1800`, optionally adding
`--larger-model MODEL_ID`. Supply the same `--home` and local inference configuration
as normal execution. Use `--json` and redirect stdout to retain the report.

The six small-model profiles use one configured model and temperature; per-role
model overrides are cleared to avoid confounding the same-model comparison.
Operator call/cycle/token/byte/tool/time ceilings remain common. Actual consumption
differs and is reported, rather than artificially forcing every arm to spend the
same amount. The optional larger baseline uses the same single-solver path.

| Profile | Behavior |
|---|---|
| alone | One solver, no tools, routing, references, interface or planner |
| tools | One solver plus deterministic arithmetic when applicable; no references |
| recursive | Solver-only generation, self-critique and reflection under shared ceilings; arithmetic available |
| collective | Interface and distinct logical reasoning roles with capability routing; no retrieved documents |
| memory | Collective reasoning with directly ingested semantic references and operator-published episodes; versioned candidate documents excluded |
| experts | Collective plus routed active reference experts and candidate snapshots |

Each case rotates which arm executes first. There is no automatic warmup or claim
that model loading costs are known. Expected answers remain outside all model
requests and persisted task traces. Per-case task IDs allow trace inspection;
reports include configured and provider-reported model labels, independent expected
matches, runtime statuses and resources. Labels do not attest immutable weights.

Reports record dataset hash/provenance, operator-attested held-out split and code
revision, effective configuration, exact execution order, per-arm totals and actual
per-case consumption. Token totals become unavailable when any constituent usage
is missing. GPU/peak RAM/energy/load cost and general security-failure metrics remain
explicitly unavailable. A run deadline or Ctrl-C cancels the active task durably,
returns `complete:false` with partial results, and makes the CLI exit unsuccessfully.

Run without concurrent document/registry updates; the corpus is operator-managed,
while each individual task pins its retrieved versions. Use a fresh state directory
and identical reviewed corpus for repeated runs. This runner does not freeze an
external model's weights or infer a true model size from its identifier.

The canonical initial suite still requires 100–200 diverse technical/reasoning
cases, including ambiguity, unseen cases and objectives requiring tools. That
curated dataset and measured live-model study remain incomplete. The arithmetic
example validates runner mechanics only. No performance-superiority or larger-model
equivalence is claimed.

Dispatch duration includes broker queueing/coordination and HTTP time. It does not
measure GPU compute alone. Task traces include pinned inference module versions
separately from provider model labels.
