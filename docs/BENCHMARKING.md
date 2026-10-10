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
isolated retrieval and cumulative approved-version regressions; see `EVOLUTION.md`.

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

Every comparison captures one read-only, consistent SQLite snapshot before the
first arm. Skills, active reference/candidate versions, documents and published
memories remain fixed for the whole run even if their live source changes. Later
revocations/updates affect normal tasks and future runs; cancel a comparison if
its frozen evidence should no longer be used. Each evaluation task records the
same snapshot metadata while its own result/trace stays durable in the original
database. Expected test outputs remain outside model contexts.

Snapshots cap the complete allocated database at 64 MB and temporarily require
a second serialization buffer plus SQLite overhead. Oversized state fails closed
rather than silently dropping references; use a smaller reviewed evaluation store.
Reports include snapshot bytes, SHA-256 and capture time, and the run deadline
includes snapshot capture. The checksum identifies the complete SQLite image,
including unrelated stored history, not merely a semantic corpus or a portable
content-only digest. No database content is exported in the report.

Live model availability, immutable weights, provider state and hardware are not
frozen. Registered-model disable still applies to new task starts through the
live operator policy. Use identical reviewed corpus/configuration across repeated
runs; model identifiers do not establish actual parameter counts.

The canonical initial suite still requires 100–200 diverse technical/reasoning
cases, including ambiguity, unseen cases and objectives requiring tools. That
curated dataset and measured live-model study remain incomplete. The arithmetic
example validates runner mechanics only. No performance-superiority or larger-model
equivalence is claimed.

Dispatch duration includes broker queueing/coordination and HTTP time. It does not
measure GPU compute alone. Task traces include pinned inference module versions
separately from provider model labels.

### Constraint scoring in comparisons

`compare` additionally accepts explicit versioned dependency-order criteria:

```json
{"input":"order:{\"nodes\":[\"a\",\"b\",\"c\"],\"before\":[[\"a\",\"c\"]]}","verification":"dependency_order_v1"}
```

Omit `expected` for this criterion. Every valid permutation satisfying all declared
constraints is accepted, including alternative orders from the alone baseline.
The evaluator independently rechecks the returned array; a runtime `completed`
label cannot make an invalid answer pass. Only completed/unverified outcomes are
eligible, preserving the existing exclusion of demo, rejection and failed runs.
All criteria are validated before any tasks start. Unknown criterion names,
malformed graphs and mixed expected-output/constraint cases fail closed.

Existing rows with `input` and `expected` retain their normalized exact-match
behavior. They may explicitly specify `verification: expected_output_exact_match`.
Reports identify the criterion per case and use null expected output for constraint
cases. Criteria are user-supplied objectives, not hidden answer labels. The simpler
`bench` command continues to require exact-match datasets. These bounded checks do
not establish general reasoning quality or replace diverse held-out evaluation.
