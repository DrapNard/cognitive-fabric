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

The six profiles share one configured primary model and temperature; per-role
model overrides are cleared. Explicit registered workflow bindings can still
select a separate expert model in the experts arm.
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
cases, including ambiguity, unseen cases and objectives requiring tools. A provisional 100-case authored corpus now lives in `evaluation/`; independent
curation, held-out review and the measured live-model study remain incomplete. The arithmetic
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

### Queue timing

Task resources and per-arm comparison totals include optional `queue_wait_ms`.
The runtime measures time from insertion into a module queue until worker receipt,
separately from the adapter's reported inference duration. The existing aggregate
`inference_ms` remains total caller time around dispatch, including queue/transport
and backend activity; queue timing is a component, not an additional GPU charge.
Per-call queue measurements are preserved in durable reasoning events. No-call
tasks have zero queue time. A cancelled dispatched call or unavailable legacy metric
makes the task total unavailable, and comparison totals preserve that uncertainty.
This does not measure GPU execution, model loading or energy.

Active routed workflow manifests add separately budgeted advice calls only to the
experts profile (and normal full execution). Other ablation profiles exclude that
call. Inspect expert_dispatch audit decisions and actual model-call/token usage;
workflow registration does not imply a measured performance benefit.

### Process memory scope

On Linux, task resources include optional `process_rss_bytes` sampled at execution
completion and `process_lifetime_peak_rss_bytes` from `/proc/self/status`. The
bounded read is at most 16 KB; unavailable or malformed data remains null.
These measure the entire calling Rust process, including caches, SQLite snapshots
and concurrent work. The lifetime high-water mark includes earlier tasks and
warm-up; it is not a peak attributable to the current task or comparison arm.
The external inference server, its model weights, GPU memory and energy are
excluded. Do not use these fields to claim end-to-end model memory efficiency.

Comparison totals retain the maximum completion RSS sample and lifetime peak,
rather than summing memory across tasks. Any unavailable sample makes the
corresponding aggregate unavailable. For independently attributable memory
comparisons, run arms in separately measured processes with backend telemetry.
Diagnostics exposes the same optional runtime-process measurements without
creating tasks. Non-Linux platforms report unavailable values.


### Expert model configuration disclosure

Each case includes `bound_expert_models`, copied from its durable task snapshots:
expert identity, exact model registration, manifest checksum, requested model and
`weights_attested:false`. This records configuration even when advice is refused
or provider inference fails; successful provider labels remain in `reported_models`.
No provider success or weight identity is inferred from a snapshot.

`small_model_ablation_configuration_controlled` is false whenever any executed
case snapshots a separately bound expert configuration. Treat such a report as a
multi-model comparison, even if the declared model string matches the primary
model. True describes the absence of observed bound expert configurations, not
proof of immutable weights, equal consumption, complete execution or statistical
validity. Inspect `complete`, actual usage and per-case snapshots independently.
Live model disable overrides frozen knowledge for later task starts.


### Structured SQL result scoring

Comparison cases may use `verification: sql_result_json_v1` with an explicit
`sql:` objective and an operator-supplied expected result JSON string:

```json
{"input":"sql:{\"tables\":[{\"name\":\"items\",\"columns\":[\"amount\"],\"rows\":[[2],[3]]}],\"query\":\"SELECT sum(amount) AS total FROM items\"}","expected":"{\"columns\":[\"total\"],\"rows\":[[5]]}","verification":"sql_result_json_v1"}
```

The evaluator parses expected and actual results independently of the runtime
status. Object field order and JSON whitespace do not affect equality. Column
order, row order and cell values/types remain significant; use explicit `ORDER BY`
when the intended answer requires deterministic ordering. SQL booleans are integer
results, not JSON boolean cells. Strings are case-sensitive, including labels.

Results require exactly `columns` and `rows`, 1–16 column labels of at most 128
bytes, at most 64 rows and matching row widths. Cells are numbers, strings or null;
objects, arrays and booleans are rejected. Duplicate/unknown fields and malformed
or oversized JSON fail. Expected results are validated before tasks start and
remain outside inference prompts. Only completed/unverified outcomes can pass;
incorrect results cannot pass merely by claiming success.

All arms use the same supplied labels. Tool-enabled arms execute the existing
bounded SQLite capability; the alone baseline must produce its own result.
This criterion checks equivalence to operator-reviewed expected data. It does not
independently establish that those labels are correct, prove arbitrary SQL safety,
or measure general reasoning quality. `bench` retains its existing text scoring.

The provisional corpus spans ten self-contained categories and includes arithmetic,
ordering and SQL criteria. Its manifest explicitly withholds independent-review,
held-out and live-evaluation claims. Thirty deterministic cases are checked by
existing framework tools; all labels still need independent review. See
`../evaluation/README.md` before attempting performance or safety conclusions.


### Pin reviewed dataset bytes

After review, pass `--dataset-sha256 LOWERCASE_SHA256` to `compare` to require the
exact reviewed file bytes. The runner hashes its bounded read-once snapshot and
rejects malformed or mismatching pins before creating any tasks or making model
calls. Even semantically equivalent whitespace changes need a new reviewed hash.
The file is not reread during execution.

Reports record `reviewed_dataset_checksum_operator_attested` and
`dataset_checksum_operator_pinned` separately from the actual dataset checksum.
Without this optional flag the actual checksum is still recorded, but no prior
byte identity is enforced. Pinning establishes content identity only: it does not
verify source/license declarations, reviewer independence, contamination checks,
held-out status or expected-label correctness. It does not make the provisional
repository suite reviewed automatically.

### Paired improvements and regressions

`paired_changes_against_small_model_alone` compares each other arm (including the
optional larger baseline) with arm zero on jointly executed case indices only.
It lists improved and regressed zero-based case indices, both-pass/both-fail
counts, paired/unpaired counts, and paired accuracy delta. Match indices to
`details` to inspect actual task traces and resources.

`regression_fraction_of_paired_cases` divides regressions by all jointly executed
cases. `regression_fraction_of_baseline_passes` divides by baseline successes in
that same paired subset. A zero denominator yields null, never zero. Deadline or
cancellation omissions are unpaired rather than failed cases. An executed outcome
still uses the normal independent expected-label/constraint criterion; failed
execution statuses cannot pass merely because their answer matches.

These are descriptive changes on the supplied corpus, not statistical significance,
held-out robustness, causal proof or general security failure rates. Partial runs
can have a biased paired subset; inspect coverage before drawing conclusions.
No extra inference, permissions or task executions are added by aggregation.

### Integer sorting criteria

Use `verification: integer_sort_v1` with an explicit bounded `sort:` objective:

```json
{"input":"sort:{\"values\":[3,-1,3,0]}","verification":"integer_sort_v1"}
```

Omit `expected`. The comparison recomputes the sorted multiset from the supplied
signed 64-bit integers, including duplicates and optional `descending: true`.
Whitespace in the JSON array does not matter; wrong values, missing duplicates,
floats, wrong order, oversized answers and extra prose fail. A runtime
`completed` claim alone never makes an incorrect answer pass. A correctly formatted
unverified generative answer may pass the explicit comparison criterion; failed
or exhausted tasks do not pass. Reports retain criterion identity and null expected
output, and the ordinary six-arm resource/paired comparison accounting applies.

Ordering and sorting reuse the runtime's compiled Rust checker but execute fresh
checks against the supplied objective rather than trusting runtime proof/status.
This is independent of the model and status assertion, not a second independently
implemented verifier. It establishes only bounded domain correctness.

`examples/sorting_comparison.jsonl` supplies three public demonstration cases
covering negative/duplicate values, descending order and integer limits. It is not
an independently reviewed or unseen held-out benchmark. Do not use the example's
availability as grounds to attest independence. No live comparative performance
or model-superiority result is inferred from the fixture tests.

### Public workflow checks

Use `compare ... --workflow-check` instead of `--held-out` to exercise all profiles
on public examples without declaring independent held-out review. Exactly one
flag is required; combining them is rejected by both CLI and Rust preflight.
Provenance, revision, dataset pins, ceilings, cancellation and scoring remain
mandatory and unchanged. Reports identify `run_purpose: workflow_check`,
`held_out_run: false` and `held_out_operator_attested: false`; a dataset pin in
this mode does not become a reviewed-dataset attestation. Scores describe only
these public cases and cannot support general quality or superiority claims.
Repository examples use Apache-2.0 licensing. Held-out comparisons still require
`--held-out`; operator attestation does not prove independence.

### Structured output contract parity

Model-alone execution receives the same deterministic JSON output instructions
for explicit sorting/ordering objectives as verification-enabled profiles. These
instructions disclose neither expected answers nor verification results. The
baseline still executes no checker and remains unverified at runtime. Independent
comparison scoring remains separate. Earlier runs at `da2a8b9` and before omitted
these instructions from the baseline and cannot establish comparative reasoning
gains from strict-format scores; inspect raw answers and rerun with the fix.

Explicit arithmetic and inline SQL objectives also receive answer-format
instructions in every reasoning profile: numeric-only output and the SQL
columns/rows JSON contract, respectively. No expected result or executed tool
output is supplied to the alone baseline. Pre-fix arithmetic/SQL scores have the
same format-fairness limitation; the correction does not relax any scorer.

### Rust compiler-backed diagnostic cases

`evaluation/rust_ownership_cases.jsonl` supplies 20 balanced Rust 2021 compilation
questions with labels independently checked by actual `rustc` metadata compilation
and expected negative diagnostic codes in the fixture regression. No snippets or
model output are executed, and no runtime compiler capability is added. See
evaluation/README.md and the separate manifest for provenance and scope.
These public authored cases supplement the existing corpus; they are not unseen
or independently reviewed held-out data. A correct yes/no answer establishes
only performance on that concrete question, not broad Rust expertise.

The first public Qwen3 8B Rust diagnostic at source `8f7964e` scored 6/20 exact
yes/no matches in 95.053 seconds with 20 calls and zero tools. Several answers
contained explanatory prose despite the required format; others were incorrect.
Raw outputs and source/configuration metadata are retained in evaluation/results.
A prior 128-token admission preflight dispatched no calls and exhausted all tasks;
it is retained separately and is not a model-performance result. The measured
run used a 512-token task ceiling, admitting the solver's 384-token reservation.

The solver role prompt now honors explicit user output-format constraints and
asks for fact/guess explanation only when the requested format permits it. This
removes an instruction tension without changing labels, scoring, permissions,
token reservations or the external-reference boundary. Model compliance is not
guaranteed. The compiled role-prompt identity changes, so old saved continuation
boundaries cannot silently resume under these changed instructions.

### Frozen Rust diagnostic rerun

After the solver-format clarification, source `b04b5b5` matched 8/20 strict labels
in 51.657 seconds, with 20 calls, zero tools, 4,327 reported input tokens and 780
reported output tokens. The before run matched 6/20 in 95.053 seconds with 3,987
input and 1,452 output tokens. Dataset bytes, generation/resource configuration,
exact scoring and question order were unchanged; each used fresh isolated state.
All runtime answers remained unverified. Zero-based exact-match indices 1, 7 and
16 improved, while index 14 regressed. Extra prose and incorrect Rust judgments
remain. Both raw reports, summaries, source/dataset/binary hashes and the no-call
admission preflight are retained under evaluation/results.

This is a single public before/after diagnostic, not a randomized controlled
study, held-out robustness result, proof of reasoning improvement or Rust expert
readiness. Output-contract adherence remains imperfect; no labels or scorer were
relaxed to raise scores. Timings include host dispatch and are not GPU compute or
energy measurements. No compiler capability was granted to the model.
