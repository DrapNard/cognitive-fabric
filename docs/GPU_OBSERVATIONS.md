# Optional benchmark GPU observations

Add `--observe-gpu` to the existing `compare` command to sample whole-device
metrics during each benchmark execution window. The default comparison behavior
and output shape remain unchanged when this option is absent.

By default the fixed `/usr/bin/nvidia-smi` query is used without a shell, inherited
environment, proxy, model-selected arguments or GPU setters. Missing drivers,
unsupported metrics and query failures remain unavailable observations; they do
not turn into zero energy or successful task verification.

## Limits and accounting

- One irreversible query budget is shared across all cases and arms: at most
  **600 queries** per comparison run, with a default one-second cadence.
- Library callers can set a cadence of 250–5000 ms and a query cap of 1–600 using
  `gpu::window::Settings` and `benchmarking::compare_observed_until`.
- Queries retain the existing 16 KB/16-device output limits and bounded timeout/
  child-reaping behavior. There are no detached sampler tasks.
- Task completion drains an in-flight query's bounded cleanup. Its late result is
  excluded from window samples, and `query_cleanup_tail_ms` exposes the delay.
- Exhausting the observation budget does not refund it or stop task execution.
  Later windows report the exhausted budget and unknown measurements.

Each detail contains `gpu_observation` with charged queries, accepted/unavailable
samples, elapsed window, cleanup tail and per-device sampled memory/utilization
maxima. The run report records settings and remaining query budget. Sampling
overhead can affect scheduling and measured runtime; it is not compensated away.
The run deadline and original model/tool/token/resource ceilings remain unchanged.
Fast tasks may finish before any sample is obtained.

## Energy estimates are partial device observations

Power is integrated by the trapezoidal rule between consecutive observed query
midpoints, with no interpolation across unavailable samples, missing power or gaps
over ten seconds. Reports expose the integrated interval count and covered duration.
Device identity changes suppress window energy estimates. No extrapolation fills
the beginning/end of a window or other uncovered intervals.

These are **whole-device estimates**, affected by idle power, other applications,
driver averaging, query latency and unobserved transients. They are not a calibrated
energy meter or proof of causal task attribution. Reports always retain:

```json
{
  "task_attributed": false,
  "task_energy_joules": null,
  "true_peak_memory_known": false,
  "full_window_energy_known": false
}
```

The existing unavailable task energy/GPU peak metrics remain unavailable. This
feature does not establish Joules per solved task, GPU isolation, model identity,
efficiency gains or equal-budget superiority. Task-attributed physical measurement
and independent live-model evaluation remain open requirements.

## Optional NVML sensor

Build production binaries with `--features nvml-gpu` to enable the optional
`nvml-wrapper` library. Select `diagnostics --probe-gpu --gpu-sensor nvml` or
`compare --observe-gpu --gpu-sensor nvml`. Without this explicit selection the
existing sensor and report shape are retained. Library callers can use
`gpu::sample_with` and `benchmarking::compare_observed_with_sensor_until`.

On Linux, NVML runs in a fixed same-image child with a cleared environment and
read-only queries. Each query has a two-second deadline, 16 KiB frame bound and
16-device bound. The versioned CGPU FlatBuffers result is verified before use.
Cancellation retains cleanup ownership through bounded kill/reap. It does not
start a persistent sampling service. Missing feature, driver, library or individual
metrics remains unavailable; no automatic fallback hides an NVML failure.
`sensor_compiled_support` describes the build feature, not runtime library health.

This is telemetry, not a CUDA inference backend. GPU inference still belongs to
the configured external model provider. The bounded frame, missing-metric and child cleanup paths have focused tests.
Live GPU measurements still require a working NVIDIA driver and visible device;
fixture tests do not establish CUDA inference or measured GPU performance.

Resource admission can separately opt into a one-shot GPU memory observation
before dispatch. It requires an operator-selected UUID and sensor and compares
observed free bytes with the call's declared device allocation plus headroom. It
is a best-effort fail-closed preflight, not GPU memory enforcement, model-load
fit prediction, or task attribution. See
[optional GPU observation admission](RESOURCE_SCHEDULING.md#optional-gpu-observation-admission)
for exact limits and recovery behavior.

`scripts/setup.sh --nvml-gpu` propagates the feature to both the core CLI and
the HTTP server. For a direct server build use
`cargo build --locked -p cogfabric-server --features nvml-gpu`. This flag enables
read-only observation; configure model offloading in the inference provider.
