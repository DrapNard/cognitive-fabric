# Public Qwen3 8B workflow study — 2026-10-13

This is diagnostic evidence, not held-out validation or proof of comparative reasoning gains.
The 100-case authored provisional corpus completed all 600 profile runs in 1138.773 seconds
using source `da2a8b9910afc9e782aa270531991d0c94430050`, Qwen3 8B and managed inference.
The raw report is retained to support independent inspection of all public answers and scores.

| Profile | Strict criterion matches / 100 | Model calls | Total task latency (seconds) |
|---|---:|---:|---:|
| alone | 2 | 100 | 256.396 |
| tools | 22 | 80 | 115.868 |
| recursive | 22 | 150 | 224.294 |
| collective | 50 | 156 | 184.072 |
| memory | 50 | 156 | 179.886 |
| experts | 49 | 156 | 178.069 |

The raw status counts are 118 completed, 438 unverified, 32 rejected and 12 needing
clarification, with no failed tasks. All 500 inference-worker launch records have
matching stopped records; every stop reports exit zero, reaped true and forced false. A strict criterion match is separate from
runtime evidence verification. For example, model-alone answered the fragile-item
question correctly with “Yes” followed by an explanation; the yes/no exact-match
criterion rejected the extra text. Do not label every strict mismatch wrong reasoning.

Known limitations:

- Before fixes `ba5dac1` / `9fec58b`, the baseline lacked explicit sorting/ordering,
  arithmetic and SQL output-format instructions supplied or embodied by other profiles.
- The public corpus was authored with the framework and lacks independent label review
  or held-out contamination review. Scores include format adherence.
- Memory and experts used an empty reference corpus and no specialized model bindings.
  Their labels do not establish retrieval or specialist benefits.
- There was no controlled warmup; host compiler and smoke activity overlapped portions
  of this diagnostic run. Before/after latency cannot establish causal gains.
- This used Qwen3 8B, not a 1–3B baseline; there was no larger-model arm, weight
  attestation, task GPU/energy attribution or broad neural security evaluation.

All profiles used ceilings of three calls, one cycle, 768 reserved output tokens,
8192 prompt bytes, four tools and 45 seconds per task, with a 30-second HTTP timeout.
Temperature was zero and requested reasoning effort was none. The run limit was
1800 seconds. Expected outputs stayed outside inference requests.

Raw report: [qwen3-8b-v1-pre-contract-parity.json](qwen3-8b-v1-pre-contract-parity.json).
Category/resource summary: [qwen3-8b-v1-pre-contract-parity-summary.json](qwen3-8b-v1-pre-contract-parity-summary.json).
Raw SHA-256: `b4fa58005f2e0498047826404642a275335ef11fac2a5efd761006f6a9596299`.
Dataset SHA-256: `6b2c7dd82bce6473f527daff8be56c5bcf3e54e7a93db088cb46def5fc06cd05`.

## Completed output-contract parity rerun

The corrected run at source `9fec58bc93fd97fb3096147360dc67681a7974b7`
completed all 600 tasks in 988.471 seconds with fresh state and frozen executable
SHA-256 `a2012ac53ec77dee168d4e2aadb1dca7067864b419a3ddd6e0db8f47e5c792d0`.
The same model and operator ceilings apply. Sorting, ordering, arithmetic and SQL
output contracts are now supplied to the model-only baseline as well.

| Profile | Strict criterion matches / 100 | Model calls | Total task latency (seconds) |
|---|---:|---:|---:|
| alone | 7 | 100 | 120.301 |
| tools | 21 | 80 | 116.878 |
| recursive | 21 | 150 | 223.715 |
| collective | 51 | 154 | 180.970 |
| memory | 48 | 154 | 173.298 |
| experts | 50 | 154 | 173.119 |

Statuses were 118 completed, 432 unverified, 32 rejected and 18 needing
clarification, with no failed status. The report's `failed_tasks` resource counter
also includes rejected, cancelled and exhausted statuses; it is not a count of
failed-status tasks alone. All 500 worker launches have matching stopped records
with reaped true, forced false and exit zero, checked against the run's SQLite audit.

Raw report: [qwen3-8b-v1-contract-parity.json](qwen3-8b-v1-contract-parity.json).
Category/resource summary: [qwen3-8b-v1-contract-parity-summary.json](qwen3-8b-v1-contract-parity-summary.json).
Raw SHA-256: `866b371d4bc44d5596c478f3e30245d65b6c4d983dd3c7a7e07e25c9011b7774`.

The limitations above still apply, except the identified structured output-contract
asymmetry. Host compilation and smoke validation overlapped this rerun too. The
before/after numbers establish neither causal latency improvements nor robust
reasoning gains. Model labels do not attest weights; expected outputs remain
outside inference requests. Independent review and the broader release gate remain
incomplete. Reports contain public authored inputs and model answers intentionally
retained as evaluation evidence, not private data or model weights.

## Public Rust compiler diagnostics

`qwen3-8b-rust-ownership-before-format.json` contains a 20-case FAST diagnostic
against compiler-checked public Rust questions. Its summary pins source, dataset,
configuration and raw-report hashes. Exact scoring matched 6/20; every runtime
answer remained unverified. Explanations can fail the strict yes/no contract even
when their leading word matches, and some answers were wrong. No score is relaxed
retrospectively. `qwen3-8b-rust-ownership-admission-preflight.json` records the
initial too-small reservation (20 exhausted tasks, zero model calls); it is not a
model-quality result. These are public authored diagnostics, not held-out studies
or evidence of general Rust expertise or collective superiority.

The frozen rerun after format clarification is retained as
`qwen3-8b-rust-ownership-after-format.json` plus its summary. It matched 8/20 strict
labels in 51.657 seconds (20 calls, zero tools; 4,327/780 reported input/output
tokens). Three exact cases improved and one regressed. Original and rerun labels,
configuration and dataset bytes match; extra explanations and factual errors
remain. This is descriptive public evidence only, not reliable Rust expertise or
a causal reasoning gain. Summaries include source, dataset, report and executable
hashes. Model weight identity and GPU attribution remain unverified.

## Native embedding lifecycle smoke — 2026-10-14

`lmstudio-native-embedding-lifecycle-smoke.json` freezes one explicitly approved
operator load/unload cycle, its before/loaded/after inventories, both Core reports
and metadata audit records. Runtime source: `73df440`; script/binary hashes are in
the report. It is infrastructure evidence rather than a reasoning benchmark.
Nomic embedding load/unload completed and the existing Qwen instance/context were
preserved. Generation, generative switching, ownership and weight attestation are
explicitly untested. Report SHA-256:
`c69857287d9e95207ca6f2eeb7e6d86d75504aa8458815580a65a96742175f26`.

## Strategy portfolio public diagnostics — October 20–21, 2026

Two complete 24-task runs retain all outcomes on the same four existing graph
cases, copied without label changes into `../portfolio_cases.jsonl` (parent lines
1, 6, 12 and 16). Each run uses six profiles, Qwen3 8B, RESEARCH mode, common
10-call/4-cycle/3,000-output/8-tool ceilings, 30-second task and 20-second request
limits. Interpretation is disabled, temperature zero and reasoning effort none.
Experts use the existing public BFS advisory workflow; semantic document memory
is empty. These are selected public integration cases, not unseen/held-out data.

Raw reports: [portfolio source](qwen3-8b-portfolio-pre-diagnostics.json) at
`5041478`, and [diagnostic reporting source](qwen3-8b-portfolio-diagnostics.json)
at `b20d736`. The [hashed summary](qwen3-8b-portfolio-public-summary.json) records
source/binary/dataset/report identities, provider settings, arm totals and
content-minimized task-scoped portfolio/selection audits.

| Profile | October 20 solved | Calls | Task latency (s) | October 21 solved | Calls | Task latency (s) |
|---|---:|---:|---:|---:|---:|---:|
| Alone | 4/4 | 4 | 1.39 | 3/4 | 4 | 14.14 |
| Tools | 4/4 | 5 | 1.59 | 4/4 | 5 | 17.77 |
| Recursive | 4/4 | 4 | 1.32 | 4/4 | 5 | 19.50 |
| Collective | 4/4 | 20 | 15.84 | 4/4 | 13 | 88.25 |
| Memory | 4/4 | 21 | 14.91 | 4/4 | 13 | 85.35 |
| Experts | 4/4 | 21 | 15.82 | 4/4 | 13 | 86.99 |

The first run completed in 50.92 seconds and observed 12 accepted portfolios and
14 selection events, including second-hypothesis use. The second completed in
312.05 seconds with 12 typed planning timeouts and 12 typed exploration timeouts.
It recorded zero accepted/selected portfolio events; original-budget fallback
solving completed all collective tasks. A phase attempt is not successful
portfolio execution. Unknown provider token usage after timeout stays unavailable.

The first run used an already loaded model with 4,096 context and four parallel
predictions; its offload configuration is unknown. Before the second run, the
server was off with no loaded model. The operator CLI temporarily started a
localhost-only server and loaded existing weights with context 4,096, one parallel
prediction, requested offload 0.35 and a 600-second idle TTL (7.99 seconds reported
load, outside comparison task latency). The model was then unloaded and the server
stopped, confirmed by status/inventory. This is manual validation setup, not proof
of framework-owned residency or physical GPU accounting. No weights were downloaded.

**Do not interpret the two runs as a software speedup/regression experiment.**
Provider loading/offload/parallelism/cache and host conditions differ. Neither
run proves a general reasoning advantage; the cheaper tools/recursive profiles
matched collective accuracy on the second run. There is no larger baseline,
independent human review, weight attestation or attributed energy/VRAM measurement.
The next useful engineering target is cost-aware optional-stage admission that
avoids repeatedly buying planning/exploration phases that time out.

`portfolio_public_runs_match_independent_scores_hashes_and_diagnostic_audits`
in `tests/shortest_path_corpus.rs` checks all 48 scores by exhaustive simple-path
enumeration separately from runtime BFS. It pins the subset/report hashes,
reconciles charged calls, task latency and per-arm diagnostic sums, and compares
new counters with the exported public audit observations. This protects evidence
consistency, not independent dataset curation or neural safety.

## Public finite-domain progress ablation — 2026-10-21

Source `08f621797cca1b6eafd682038a5c4ff6833e10f1` completed all 17 paired
RESEARCH cases with the installed Qwen3 8B Q4_K_M model. Immutable
[raw report](qwen3-8b-progress-ablation.json) and
[provenance/audit projection](qwen3-8b-progress-ablation-summary.json) retain
configurations, source/prompt/dataset hashes, answers, task identities and costs.
The separate exhaustive Rust oracle checks all 34 final scores and reconciles
reported calls and latency; it does not use the production verifier.

| Observation | Stopping disabled | Stopping enabled (threshold 2) |
|---|---:|---:|
| Valid witnesses across all 17 tasks | 14 | 13 |
| Valid witnesses on 14 satisfiable tasks | 14/14 | 13/14 |
| Model calls | 85 | 78 |
| Summed task latency | 55.87 s | 48.44 s |

The other three cases are independently enumerated contradictions; no answer or
model claim of impossibility receives witness credit. Total run duration was
104.37 seconds. The policy's plateau audit fired only on unsatisfiable case 16.
The satisfiable paired regression at case 4 did **not** fire that rule. Planner,
advisory and solver paths differed, so neither that regression nor the aggregate
call/time changes can be attributed solely to the policy. The evidence does not
justify enabling it by default or claiming a quality/efficiency improvement.

This is one public authored diagnostic, not a held-out, repeated or independently
curated study. There is no larger-model baseline, weight attestation, measured
energy or attributed peak GPU usage. Runtime counters are local audit projections,
not independent execution attestations. The installed model was temporarily loaded
with requested full GPU offload, 4,096 context and one parallel prediction; the
reported 7.53-second load is outside task latency. Initial off/unloaded server
state was restored and checked. No weights were downloaded, no policy was
persistently installed and no neural candidate was promoted.
