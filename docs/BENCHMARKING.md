# Reproducible evaluation

Run `cogfabric --home STATE --json bench DATASET --mode MODE`, optionally adding
`--max-run-seconds SECONDS` (default 1800; allowed range 1–86400). DATASET is UTF-8
JSONL with `input` and `expected` strings, at most 1,000 cases / 2 MB. Each case is
bounded before execution. Every case is validated before the first task starts;
empty fields and unknown JSON fields (including unsupported `verification` fields)
are rejected. Use `compare` for versioned non-text criteria. The evaluator compares whitespace-normalized,
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

### Basic benchmark execution disclosure

`bench` reports the SHA-256 of its read-once dataset bytes, mode, format version,
runtime status counts, `tasks_with_model_calls` and `tasks_with_tool_calls`,
retaining all prior totals/details. Model-call counts reflect charged task calls,
not proof of successful provider computation. Tool counts reflect charged tool
invocations, not arbitrary external actions. Missing token metrics remain unchanged.

The runner preflights all cases and the 1,000-case ceiling before task admission.
A later invalid case cannot consume earlier-case inference/tools or create task
records. Labels remain outside model prompts. A fully processed file can still
contain exhausted/failed outcomes and no inference; inspect status/call counts.
Conversely, deterministic arithmetic can pass with zero model calls and a real
tool invocation. Dataset hashes prove content identity, not independent review,
correct labels or held-out status. Existing historical reports are not rewritten
to add fields they did not originally contain.

### Basic benchmark deadlines and partial results

`bench` now has a run-wide deadline in addition to each task's existing ceilings.
The run clock includes bounded dataset reading/preflight; it is checked before
each new task. Deadline expiry or Ctrl-C cancels the active task through the
Engine and retains its durable cancelled outcome and charged reservations.
Completed prefix results stay in the report and later cases are not admitted.
A cancellation already ready before admission returns no task records.

Reports add `complete`, run `stop_reason` (`completed`, `run_deadline`, `cancelled`),
`max_run_seconds`, `executed` and `unexecuted`. `total` still means dataset cases.
Accuracy uses attempted/executed cases (including a cancelled attempt), excludes
unexecuted cases and is null when none were admitted. Partial subsets can be
biased. A complete run can still have failed/exhausted outcomes or zero model
calls; completion is not task success or model-quality proof.

The CLI prints partial JSON to stdout and exits unsuccessfully on interruption;
stderr carries the error. Keep stdout redirected to retain results. Rust callers
may use `evaluation::benchmark_until` with an explicit cancellation future; the
existing `benchmark` wrapper uses the default deadline and returns the same
partial report shape, so callers must check `complete`.

Cancellation awaits existing task/worker cleanup, which can extend response time
beyond the execution deadline. Synchronous bounded file/preflight work and OS
scheduling are not preempted by a timer. Disconnect does not prove an external
model server stopped GPU work. No new retries, resource refunds, permission grants
or expected-label disclosure are introduced. Historical reports are unchanged.

### Configuration and task model identity

Basic benchmark reports now include package version, declared effective
`configuration`, its SHA-256 (serde_json encoding of the reported JSON object),
and `compiled_role_prompts_sha256`. Configuration validation precedes reporting,
including already-cancelled runs. Home paths are omitted by Config serialization
and credentials are never read into these fields. Package/prompt hashes alone
do not identify every source change; pin source revision externally or use the
comparison runner's explicit revision/provenance fields.

Each executed case in `bench` and `compare` includes backend `reported_models`,
`bound_expert_models` and `primary_model_snapshot` from its durable trace. Repeated
labels preserve call order. Failed/cancelled/no-call tasks may have no reported
label despite recorded configuration or charged reservations. Deterministic
tool-only tasks report no inference model labels. Local adapters may fall back
to the requested model when a provider omits its label; these are backend labels,
not attestation of provider weights.

Both runners share one bounded, content-minimized identity reader: at most 128
identity events, 256-byte labels, 16 KiB snapshots and 256 KiB aggregate projected
metadata. SQL projects model labels without copying reasoning bodies into Rust.
Invalid/oversized labels, malformed snapshots or duplicate primary snapshots fail
closed; full reasoning remains in ordinary task traces, outside identity fields.
The compiled prompt hash identifies this package's local instructions, not proof
a trusted replacement backend used them. `backend_configuration_attested` and
`weights_attested` stay false. Metadata does not grant permissions or establish
review independence, correct labels, immutable weights or model quality.

### Executive policy disclosure

`bench` records its Engine's cached `expert_router` declaration; `compare` records
the default router used by its constructed arms. `router_implementation_attested`
remains false. Task `route` audits distinguish configured policy metadata from
actual invocation through `router_invoked`; excluded profiles do not call it.
Default selection behavior, scoring and task budgets are unchanged. Custom native
policies are trusted host code, not sandboxed or automatically model-budgeted.
See ROUTING.md before comparing replacement policies.


Comparison cases support `verification: shortest_path_v1` with an explicit
`path:` objective and no `expected` field. Scoring independently accepts any
shortest directed unweighted path, including `[]` only for unreachable goals.
The alone arm receives the same output contract without runtime checker authority.
This is structured diagnostic coverage, not an independent held-out quality study.


### Public shortest-path diagnostic corpus

`evaluation/shortest_path_cases.jsonl` contains 16 original CC0-1.0 directed,
unweighted graph objectives: direct/chain paths, alternative optima, directed
reverse traps, disconnected graphs, cycles, unreachable goals, identical endpoints,
dead ends, nonlexical labels and branch merging. Inputs contain only criteria and
`verification: shortest_path_v1`, with no expected path fed to inference. The
separate `evaluation/shortest_path_manifest.json` pins dataset/objective hashes,
minimum distances and one accepted optimum per case. Dataset SHA-256:
`0fedaaf6bef27f45ae0a13debcd5c6e391a32637c84891b0f38a19d9cba93edc`.

`tests/shortest_path_corpus.rs` independently enumerates all simple paths in these
small fixed graphs to check labels, alternative optima and provenance metadata.
The runtime checker uses breadth-first distance computation instead. Comparison
regressions prepare every frozen row and recheck reference outputs while rejecting
forged verification objects and rejected runtime states. This is public diagnostic
data, not a held-out or independently human-reviewed study. Contamination has not
been assessed; no live model result is recorded. On 2026-10-13 the configured
LM Studio endpoint `127.0.0.1:1234` refused the read-only model-list connection
(curl exit 7). No provider was started, no weights loaded/unloaded and no model
quality score is inferred from that failure.

When the authorized Qwen endpoint is available, use a fresh workspace configured
for `qwen/qwen3-8b` and run a bounded `compare` with this dataset, its pinned hash,
`--source "Public synthetic shortest-path diagnostics" --license CC0-1.0`,
`--code-revision` set to the tested revision, and `--workflow-check`. Use the
same configuration and budgets across the six profiles, retain raw reports and
usage, and report incomplete/failed runs. Do not label this public corpus held out.
Independent unseen tasks and equal-budget model studies remain release work.


### Real Qwen graph diagnostics: bounded partial deep comparison

On 2026-10-13, source `22dbc5acbfb55df7c9d8f55c103fa0af28cfcd21` ran
`qwen/qwen3-8b` through LM Studio on the frozen 16-case public graph corpus.
The server was initially off and no model was loaded. Its CLI started the server
on `127.0.0.1:1234` without changing authentication; the existing configuration
allowed access. Default GPU loading failed (cause not established). With 6,905 MiB
of whole-device memory already occupied out of 12,288 MiB, a 35% GPU-offload load
succeeded in 4.05 seconds as reported by the CLI. Context was 8,192 tokens,
parallelism one, idle TTL 600 seconds. Other workloads were not stopped. This is
operator CLI setup, not proof of framework-managed residency ownership, native
REST lifecycle switching, immutable weights or GPU attribution. The loopback
LM Studio server remains started; the loaded model has the configured idle TTL.

A fresh isolated state registered `evaluation/shortest_path_workflow.json` as
`graph.path.s1` version 1.0.0. It is an advisory breadth-first-search checklist,
not an executable or verification grant. All six arms used the same Qwen backend
and original per-task ceilings: eight calls, three cycles, 3,000 reserved outputs,
eight tools, 128,000 input bytes, 90 seconds wall time, 60-second inference timeout,
temperature 0.15, reasoning effort none, interpretation/synthesis disabled. Profiles
may further narrow these common operator ceilings. The memory arm started without document evidence. No larger or 1–3B baseline ran.

The 1,200-second run cap produced **56/96 task outcomes**, with **40 unexecuted**,
`complete: false`, `stop_reason: run_deadline`, and one cancelled memory task.
CLI exit 1 explicitly reported the incomplete comparison. Raw evidence is
`evaluation/results/qwen3-8b-shortest-path-deep-partial.json`; its separately
hashed summary records load context, workflow/binary hashes and limitations.
Temporary execution state/logs remain at `/tmp/cogfabric-qwen-path-study.0f37ok`.
The initial real FAST probe independently verified `["a","c"]` in one call;
its temporary evidence is `/tmp/cogfabric-qwen-path-probe.GcHvdD`.

| Profile | Passed / executed | Passed / 9 common cases | Charged model calls | Total task seconds |
|---|---:|---:|---:|---:|
| Alone | 6 / 9 | 6 / 9 | 9 | 30.14 |
| Tools | 6 / 9 | 6 / 9 | 14 | 48.11 |
| Recursive | 7 / 9 | 7 / 9 | 11 | 32.34 |
| Collective | 6 / 10 | 6 / 9 | 40 | 293.72 |
| Memory | 6 / 10 | 6 / 9 | 38 | 272.09 |
| Experts | 2 / 9 | 2 / 9 | 13 | 523.56 |

There were 125 charged model calls and 73 charged tool invocations. These are
runtime reservations/dispatches, not proof of completed GPU work. Expert failures
were six inference timeouts and one HTTP transport failure. Missing reported token
usage stays null for affected arms. Backend labels do not attest weights; external
GPU activity and partial offload prevent comparing latency against earlier
full-GPU runs. The report's old general limitation strings predate explicit graph
wording; shortest-path scoring also shares the compiled runtime checker. The
separate corpus regression independently enumerates simple paths and rechecks
every frozen outcome's pass bit, identities, totals and reference hashes.

**No expert or collective benefit is demonstrated.** Recursive corrected one
additional case in this single nine-case matched subset. This is incomplete,
public, non-human-reviewed data with no replicate/seed-controlled study; it is not
causal evidence, held-out robustness, general reasoning ability or neural safety.
Direction-trap answers using reversed undeclared edges were independently rejected.
Corpus labels/scoring and budgets were not changed to improve results. The corpus
manifest now records partial live evaluation while retaining incomplete/independent
study flags. Framework residency, broader scheduling/expert isolation and unseen
independent comparisons remain release work.

To reproduce the procedure, configure a fresh state with the report's declared
settings, register/activate the committed workflow, then run:

```bash
cogfabric --home /path/to/fresh-state --json compare evaluation/shortest_path_cases.jsonl \
  --mode deep --source 'Public synthetic shortest-path diagnostics with advisory BFS workflow' \
  --license CC0-1.0 --code-revision "$(git rev-parse HEAD)" \
  --dataset-sha256 0fedaaf6bef27f45ae0a13debcd5c6e391a32637c84891b0f38a19d9cba93edc \
  --workflow-check --max-run-seconds 1200
```

Record the actual tested source and load configuration for each new run. This
reproduces the controlled procedure, not identical stochastic model responses.

### Expert advice diagnostics in benchmark reports

Each benchmark/comparison detail now includes `expert_advice` audit counts:
`phases_started`, `fallback_events`, `inference_timeout_fallbacks`, and
`unclassified_fallbacks`. Current optional-advice timeouts persist the typed
`failure_kind: inference_timeout` classification. Historical events without that
classification and future unknown kinds remain unclassified; error text is never
used to infer a timeout. Counts are task-scoped and read through bounded SQLite
projections (128 events, 16 KiB per diagnostic event), without copying advice,
prompts or error messages into the report. Oversized audits fail reporting.

These are attempted-phase/fallback counts, not expert success rates. A fatal
provider/audit failure or cancellation may leave a started phase without a fallback
event; absence of fallback proves neither completion nor usefulness. Existing
reports remain frozen. Independent quality evaluation, broader expert lifecycle
and physical provider termination remain incomplete.

### Live four-case expert diagnostic follow-up

Source `b6b704323cac67b54935fd06397eeb66113be673`, Qwen3 8B through LM Studio,
completed 24/24 tasks in 282.33 seconds. The first four public graph cases were
run across all six profiles in fresh state with the committed advisory workflow,
empty document memory, 30-second task ceilings and 20-second inference ceilings.
Other declared settings and per-task resources are retained in
`evaluation/results/qwen3-8b-advice-diagnostics-four-cases.json`; hashed context and
summary are in its `-summary.json` companion. The CLI loaded the existing Qwen
weights with 8192 context, 0.35 requested GPU offload, one parallel prediction and
600-second idle TTL. No other model was unloaded; framework residency ownership
and immutable weights remain unproven.

Scores: alone 3/4, tools 3/4, recursive 3/4, collective 3/4, memory 2/4,
experts 2/4. All four expert advice phases emitted typed timeout fallback events;
main reasoning then completed two answers and exhausted two original task budgets.
The six arms charged respectively 4, 5, 5, 17, 16 and 13 model calls. Independent
simple-path enumeration agrees with every recorded pass bit; a frozen integration
regression checks scores, hashes, charged calls and diagnostic totals.

This demonstrates live bounded advice failure containment and report diagnostics,
not expert usefulness. There is no expert gain in this run. The corpus is public,
small and authored in this project; no held-out or independent human review,
training-contamination control, larger baseline or causal quality study is claimed.
Task ceilings differ from the older 90-second partial run, so these results must
not be treated as a controlled before/after comparison. Historical reports remain
unchanged. General expert lifecycle/isolation and independent quality remain open.

### Concise optional advice output contract

Legacy and structured advice prompts now request one sentence of at most 24
words naming the most important domain constraint/pitfall, without restating the
problem, enumerating steps or producing the final answer. This is a model output
request, not enforced linguistic correctness. Runtime token reservations, phase
ceilings, permissions, audit failure handling and independent verification remain
unchanged. Advice remains unverified; no new trusted evidence is created.

A direct Qwen probe on the first public graph used the same system/model settings:
192 output tokens exceeded a 30-second client deadline; 64 tokens returned in
15.78 seconds with length truncation and an incorrect undirected description;
the original 192-token allowance plus concise contract returned a complete
17-token directed-edge/cycle warning in 3.93 seconds. Full requests, responses,
order and limitations are retained in
`evaluation/results/qwen3-8b-advice-output-contract-probe.json`. This is one
sequential sample per setting, not a controlled latency study or independent
quality evaluation. The cancelled provider request's physical termination is
unproven. Full six-profile results from the preceding revision remain unchanged;
after-change collective effectiveness has not yet been measured.

### Concise advice measured through the collective runtime

Source `21970615d1b22439bec35e6082277f99f86b1a1d` completed the same four public
cases across all six profiles in fresh state: 24 tasks in 227.29 seconds. Operator
settings, input hash, workflow and empty document corpus match the preceding
four-case run; the existing loaded Qwen was reused. Results and a hashed summary
are retained in `evaluation/results/qwen3-8b-concise-advice-four-cases*.json`.
Independent exhaustive simple-path enumeration agrees with every recorded score.

Alone/tools/recursive scored 3/4 each; collective 2/4; memory 3/4; experts 4/4.
All four expert tasks completed with three charged calls each and no advice
fallback events. Expert totals: 12 calls, 8 checks, 43.67 seconds task latency,
3940 reported input tokens and 210 output tokens. Their elapsed times were
13.92, 12.24, 12.12 and 5.39 seconds, within unchanged 30-second task budgets.
The previous run's four advice timeouts are preserved in its historical report;
this revised run recorded zero. The frozen integration checks both reports with
one independent oracle and verifies hashes, scores, charges and diagnostic totals.

These are encouraging observations on one small public workflow. Two sequential
runs do not establish causality, stable latency, held-out robustness or a general
expert advantage. Sampling, provider state and whole-device activity were not
controlled; changing collective/memory scores illustrate that uncertainty.
Independent human review, unseen technical evaluation, broader expert provisioning
and physical model lifecycle remain incomplete. No neural training or permissions
were added.
