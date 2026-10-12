# Observed local inference — 2026-10-12

These are integration observations, not benchmark or general reasoning claims.
An existing Ollama server at `http://127.0.0.1:11434/v1` advertised
`orcarouter/Qwen3.8-27B-Uncensored:latest`. Its tags reported Q4_K_M, 27.3B
parameters and digest
`6fac2f98fdf716f292de04c8554681b1e1f3a0d71445e374afebb3433911f705`.
These are provider claims, not independently attested loaded weights. No weights
were downloaded and no server authentication or configuration was changed.

The existing debug executable used a fresh temporary workspace, temperature zero,
goal interpretation disabled, one call/cycle, 384 reserved output tokens,
8,192 input bytes, a 90-second HTTP timeout and a 120-second task wall limit.
An initial 128-token trial made zero inference calls because the solver's
384-token reservation could not fit. It did not test provider generation.

| Execution | Answer/status | End-to-end ms | Inference ms | Provider input/output tokens |
|---|---|---:|---:|---|
| Direct, `Reply with just READY. /no_think` | `READY.`, unverified | 79062.485473 | 79059.804553 | 65 / 33 |
| Managed worker, `sort:{"values":[3,1,2]}` | `[1,2,3]`, completed/verified | 19484.212881 | 19479.940799 | 95 / 61 |

Both used one model call and one reasoning cycle. Sorting consumed two tool calls
and produced `verified:integer_sort/v1` through the compiled independent checker.
The trace records worker launch intent, owned PID, authenticated handoff,
authorization before inference, inference completion, session stop and exit zero
with `reaped: true`, `forced: false`. The worker remains trusted same-binary code,
not a sandbox. Its handoff took 3.56988 ms; weight-loading time was not measured.

Task IDs: direct `70e0c0bc-9322-4c40-a2ba-b1ec9352771f`, managed
`cdb7e741-b162-429f-92c4-1725fa3f11ad`. Local evidence was retained under
`/tmp/cogfabric-ollama-live.48ZWhh` as `outcome-model.json`,
`outcome-worker.json` and `trace-worker.json`. Temporary files may disappear.

To reproduce on a host with that installed model and `jq`:

```sh
live_home=$(mktemp -d /tmp/cogfabric-live.XXXXXX)
cogfabric --home "$live_home" configure-local \
  --model 'orcarouter/Qwen3.8-27B-Uncensored:latest' \
  --api-base http://127.0.0.1:11434/v1 --temperature 0
jq '.interpret_goals=false | .max_calls=1 | .max_cycles=1 |
    .max_tokens=384 | .timeout_seconds=90 | .max_wall_seconds=120 |
    .max_input_bytes=8192' "$live_home/config.json" > "$live_home/bounded.json"
chmod 600 "$live_home/bounded.json"
mv "$live_home/bounded.json" "$live_home/config.json"
cogfabric --home "$live_home" --json ask 'Reply with just READY. /no_think' --mode fast
cogfabric --home "$live_home" configure-worker --enabled true
cogfabric --home "$live_home" --json ask 'sort:{"values":[3,1,2]}' --mode fast
```

Inspect `cogfabric --home "$live_home" --json task TASK_ID` for independent proof
and child cleanup. Actual answers, latency and token reports may differ across
runs and server versions. Use separate state from an existing running service.

Ollama later reported 9,758,929,057 bytes of VRAM residency and context length
4096; no independent task-attributed GPU measurement or energy measurement was
obtained. Direct timing may include cold loading; it is not isolated load cost.
The available 27B model is not the intended small-model baseline. LM Studio was
offline during these observations; its earlier 401 result does not invalidate
this compatible-endpoint success. Real LM Studio authentication/switching, small
baseline availability, held-out review and equal-budget studies remain open.

## Installed socket service — 2026-10-12

Optimized source `dec46fa` was tested through a fresh owner-local socket service
with the same bounded model settings and managed inference enabled. The actual
query `sort:{"values":[9,-2,9,0]}` returned `[-2,0,9,9]`, completed/verified, with
`verified:integer_sort/v1`. One call/cycle consumed two tool calls, reserving 384
output tokens. Provider usage: 97 input / 63 output tokens. End-to-end time:
40655.618962 ms; inference time: 40652.959121 ms; queue wait: 0.007795 ms.
These timings include the observed request path and do not isolate cold loading.

Task: `d77a142e-b1cd-45fd-b3a5-f5f1a21d2143`. The trace recorded a 3.355072 ms
worker handoff followed by exit zero, `reaped: true`, `forced: false`. Service STOP
returned accepted/inactive; the service exited and removed its socket. Evidence:
`/tmp/cogfabric-live-service.6LePf9/{outcome.json,trace.json,service.log,stop.json}`.
The installed binary SHA-256 is
`6aec07685580e3e519571e5a1047af52e07592b1b3fcf93c5e50f98217766354`.
To repeat, start `cogfabric --home "$live_home" serve` with the bounded settings,
submit the above goal via `ask --socket --mode fast`, inspect `task`, and stop with
`stop`. This proves one installed service integration case, not general model
quality, weight identity, isolation, switching or an equal-budget comparison.

## Structured language interpretation failure — 2026-10-12

Installed source `2ac215b` was also tried with interpretation enabled, STANDARD
mode, a two-call/one-cycle ceiling, 768 total reserved output tokens and a
180-second task limit. The natural request was `Explain Rust borrowing in one
sentence. /no_think`. The interface's fixed 128-token reservation produced empty
provider text. The CLI exited 1 with a visible `Model returned empty text` error;
no structured goal or solver answer was produced. One call remained charged,
zero cycles ran, 250 prompt bytes were reserved, and token usage stayed unavailable.
Elapsed time: 76925.494189 ms; inference time: 76924.511327 ms. The empty response
alone does not prove why the provider failed or that internal reasoning consumed
its allocation. Do not infer a successful language-interface test from earlier
solver-only sorting results.

Task: `d362daa4-b139-440a-a727-8c1989c9038c`. Evidence:
`/tmp/cogfabric-live-language.W9nCKs/{outcome.json,trace.json,stderr.txt}`.
Real structured interpretation remains unverified for this model/configuration;
provider output control and bounded interface allocation need further investigation.

The same natural-language request was repeated against debug source `a918e18`
using the new operator-selected `interface_tokens: 384`, with all earlier
cumulative limits unchanged. It timed out at 90006.747877 ms (inference
90004.322645 ms), retaining one charged call, 384 reserved output tokens and 250
prompt bytes, with no solver cycle or verified answer. Provider token totals were
unavailable. CLI exit: 1. Task: `5db99aa2-3aa9-4625-ab1b-3a9dbccab8cd`; evidence:
`/tmp/cogfabric-live-interface-budget.448IsG/{configured.json,outcome.json,trace.json,stderr.txt}`.
The configurable reservation worked but did not establish successful structured
interpretation. A larger budget is not evidence that the earlier empty response
was caused solely by the allocation. No retry or timeout increase followed.

## Explicit provider effort trial — 2026-10-12

Debug source `418b722` repeated the same STANDARD natural-language request and
384-token interface allocation with `reasoning_effort: none`; all earlier task
ceilings were retained. Unlike the prior empty/timeout cases, both interface and
planner produced provider text. The interface text failed strict JSON parsing,
so the original request was preserved. The planner then consumed the second/last
call; no solver could run and the CLI exited 1 with `exhausted`.

Task `7110a991-6a53-4a8f-a0f9-7d89f5f2e79c`: two calls, one cycle, 640 reserved
output tokens, 383 prompt bytes, provider totals 264 input / 188 output tokens,
103113.508214 ms elapsed and 103107.179455 ms inference. The worker exited zero
and was reaped without force. Evidence:
`/tmp/cogfabric-live-effort.jpDI6A/{configured.json,outcome.json,trace.json,stderr.txt}`.
The request setting is transported correctly but this trial proves neither valid
structured interpretation nor controlled hidden reasoning. It exposed a scheduler
gap: initial planner admission used total rather than remaining call/token limits
after interface work. That gap must be fixed and regression tested.

## Successful bounded structured interface and solver — 2026-10-12

Debug source `3b186cd` repeated the same natural request with unchanged two-call,
768-output-token, 90-second-per-call and 180-second task ceilings; explicit
`reasoning_effort: none` and 384 interface tokens were retained. The interface
returned the observed fenced schema-valid object. The trace records
`structured_goal` with the original request, one-sentence constraints, `rust.s1`
and `trusted_permissions: false`. Planning was skipped to preserve the last solver
call; the whole-goal solver returned an explanation. CLI exit: 0; status:
`unverified`, with no independent answer evidence. No claim of correctness follows
from model prose or successful interpretation.

Task `aa74a0c7-a197-476a-8d86-268e24030864`: two calls, one cycle, 768 reserved
output tokens, 783 prompt bytes, provider totals 315 input / 113 output tokens,
63102.431378 ms elapsed, 63095.939683 ms inference and 0.104088 ms queue wait.
The actual trace dispatches 384 tokens to interface then 384 to solver. The worker
exited zero and was reaped without force. Evidence:
`/tmp/cogfabric-live-fenced-intent.5usCUv/{outcome.json,trace.json,stderr.txt}`.

To reproduce, use the earlier isolated setup, enable interpretation, set
`max_calls: 2`, `max_cycles: 1`, `max_tokens: 768`, `max_wall_seconds: 180`, and run:

```sh
cogfabric --home "$live_home" configure-local \
  --model 'orcarouter/Qwen3.8-27B-Uncensored:latest' \
  --api-base http://127.0.0.1:11434/v1 --temperature 0 \
  --interface-tokens 384 --reasoning-effort none
cogfabric --home "$live_home" configure-worker --enabled true
cogfabric --home "$live_home" --json ask \
  'Explain Rust borrowing in one sentence. /no_think' --mode standard
```

This establishes one working real structured language/solver integration with
bounded budgets, pinned settings and cleanup. Robust interpretation/synthesis,
recursive gains, small-model baseline comparisons and held-out evaluation remain
unmeasured. Earlier failures remain valid observations for their recorded code
and settings; they are not silently reclassified as successes.

## Bounded live planner, solver and critic — 2026-10-12

Source `9b8385a` ran the same borrowing request in DEEP mode against the existing
Ollama model and endpoint above. Interpretation was disabled; reasoning effort
was `none`. Limits were three calls, one cycle, 768 reserved output tokens,
8192 prompt bytes, 240 task seconds and 90 seconds per HTTP request.
The planner returned an exact outer JSON fence containing a valid two-step
acyclic graph. The runtime accepted it as advisory data with
`trusted_permissions: false`, skipped subtask execution for insufficient budget,
and preserved the whole-goal solver and critic calls. The critic returned strict
JSON with `revise: false`; execution stopped with `no_useful_revision`.
The explanation remained `unverified` with no independent evidence. This proves
one real bounded role sequence, not answer correctness or recursive improvement.

Task `0d24802d-f46b-4016-8c9e-c47a47810757`: three calls, one cycle,
768 reserved output tokens, 793 prompt bytes, provider totals 329 input / 189
output tokens, 112598.241102 ms elapsed, 112589.381882 ms inference and
0.132632 ms queue wait. Worker exit was zero, reaped without force.
Evidence: `/tmp/cogfabric-live-roles.EZsIft/{outcome.json,trace.json,trace-after32.json,stderr.txt}`.
The two event pages cover the planner graph, all role calls and worker cleanup.
Reproduce the earlier isolated setup with the limits above and run the same
request using `--mode deep`, with `interpret_goals: false` in its configuration.

## Public six-profile workflow check — 2026-10-12

Source `55cd52d` ran `compare --workflow-check` on the first public case from
`examples/sorting_comparison.jsonl`: sorting `[3,-1,3,0]`. Dataset SHA-256:
`1434ca3af327f9e3b96ccc7dd9dfb04e81314fcb1928e53498321d8372a4049a`.
Source declaration identified the repository public example, licensed Apache-2.0.
The existing 27B Ollama model, temperature zero and explicit effort `none` were
used throughout. Per-task ceilings were three calls, one cycle, 768 output tokens,
8192 prompt bytes, four tool calls and 240 seconds; HTTP timeout was 90 seconds.
Interpretation and synthesis were disabled. The run limit was 600 seconds.

| Profile | Result | Calls | Reserved output tokens | Elapsed ms |
|---|---|---:|---:|---:|
| alone | HTTP inference timeout; failed | 1 | 384 | 90016.171987 |
| tools | Checked `[-1,0,3,3]` | 1 | 384 | 8493.257807 |
| recursive | Checked `[-1,0,3,3]` | 1 | 384 | 5362.535589 |
| collective | Checked `[-1,0,3,3]` | 2 | 640 | 43346.400414 |
| memory | Checked `[-1,0,3,3]` | 2 | 640 | 48956.604196 |
| experts | Checked `[-1,0,3,3]` | 2 | 640 | 41637.854245 |

CLI exit was zero and `complete: true`: every planned profile executed, including
the failed baseline. Total elapsed was 237821.87943 ms. The report explicitly
records `run_purpose: workflow_check`, `held_out_run: false` and
`held_out_operator_attested: false`. The baseline's provider usage is unavailable;
its timeout remains charged and was not retried. All six managed workers were
reaped without force; baseline worker exit was 1, the other five exits were 0.

This public single-case observation establishes live comparison plumbing, failure
reporting and structured scoring. It establishes neither small-model performance,
held-out accuracy nor collective advantage. Order/warmup/provider state are not
controlled by a single case. The recursive profile stopped after its first verified
answer; no recursive improvement occurred. The fresh corpus contained no ingested
references or candidate expertise. Framework RSS excludes model-server memory.
Evidence: `/tmp/cogfabric-live-workflow.X9Phae/{cases.jsonl,config.json,report.json,stderr.txt,exit-status.txt}`;
worker cleanup is recorded in that workspace's SQLite events.
