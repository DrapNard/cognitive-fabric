# Optional benchmark GPU observations

Add `--observe-gpu` to the existing `compare` command to sample whole-device
metrics during each benchmark execution window. The default comparison behavior
and output shape remain unchanged when this option is absent.

The existing fixed `/usr/bin/nvidia-smi` query is used without a shell, inherited
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
