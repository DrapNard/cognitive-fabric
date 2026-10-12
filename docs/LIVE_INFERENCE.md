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

To reproduce on a host with that installed model, create an isolated workspace
with `configure-local --help` options, select the above endpoint/model, apply the
bounded settings above, then use `ask` for the direct prompt. Enable managed
inference with `configure-worker --enabled true` and submit the sorting goal.
Inspect `task TASK_ID` for independent proof and child cleanup. Actual answers,
latency and token reports may differ across runs and server versions.

Ollama later reported 9,758,929,057 bytes of VRAM residency and context length
4096; no independent task-attributed GPU measurement or energy measurement was
obtained. Direct timing may include cold loading; it is not isolated load cost.
The available 27B model is not the intended small-model baseline. LM Studio was
offline during these observations; its earlier 401 result does not invalidate
this compatible-endpoint success. Real LM Studio authentication/switching, small
baseline availability, held-out review and equal-budget studies remain open.
