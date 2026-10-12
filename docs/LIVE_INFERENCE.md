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
