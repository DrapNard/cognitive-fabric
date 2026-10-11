# Operator diagnostics

Run `cogfabric --home STATE --json diagnostics` for storage statistics, schema
version and registered model selection validation. No task, generation, recovery,
retention or model lifecycle operation is performed. As with other CLI commands,
opening a workspace initializes/migrates its database and bootstraps missing defaults;
this is not a forensic read-only database opener.

Add `--probe-inference` to request local model discovery. This performs only a GET
of the configured OpenAI-compatible `/models` endpoint, under the operator-only
`inference.diagnose` capability. No proxy, redirect, hostname resolution other than
localhost-to-loopback normalization, credentials or remote endpoint is permitted.
The request has a three-second timeout and 64 KiB response bound. The list is
limited to 256 distinct bounded model IDs. Both the default and role-profile models
must be advertised for the check to pass. Demo inference skips network discovery.

Reports distinguish not-requested, not-applicable, responded and failed discovery.
They state whether generation was tested (false) and whether weights were attested
(false). A model list cannot establish loaded weights, residency, correct generation,
GPU support or reasoning quality. SQLite page/WAL statistics are not integrity or
backup validation. Use `service-status` separately for a bounded typed socket check.

The CLI prints its report and exits nonzero on invalid registered selection,
failed discovery or missing configured model IDs. Configuration/database errors
that prevent diagnostics from starting retain the normal CLI error behavior.
Use an actual task and inspect its trace to validate generation separately.

Tests cover offline operation without task creation, loopback GET behavior,
redirect rejection, malformed/duplicate/oversized responses, missing configured
models and denial to reasoning actors. The live CLI smoke check found the default
local endpoint unavailable and correctly returned exit status 1.


## Optional NVIDIA observations

`diagnostics --probe-gpu` performs one operator-authorized `telemetry.gpu` query.
It launches only `/usr/bin/nvidia-smi` with fixed read-only query arguments, no
shell/PATH lookup, cleared environment and no stdin. Models receive no executable
path, arguments or GPU capability. The installed NVIDIA executable/driver remains
trusted system software, not a sandboxed plugin. Standard diagnostics performs no
GPU process launch and reports `not_requested`.

The child has a two-second query deadline, 16 KiB stdout limit and discarded stderr.
Failure/overflow/timeout triggers termination and up to two seconds for reaping;
kill-on-drop is a fallback on dropped ownership. General process-crash guarantees
and external driver cancellation are not provided. At most sixteen unique device
rows are accepted, with strict shape/range/unit checks. Unsupported fields remain
null; malformed data makes the complete sample unavailable.

Samples expose whole-device UUID, total/used memory bytes, utilization percent and
reported power watts. Memory query MiB values are converted to bytes. Power is the
reported sample, not an integrated energy measurement. `energy_joules` is null and
`task_attributed` is false: other processes and model servers can contribute to
all observations. These fields do not change task budgets or benchmark totals.
See NVIDIA's query documentation:
https://docs.nvidia.com/deploy/nvidia-smi/index.html.

Missing executable, inaccessible driver or unsupported platform produces
`status: unavailable` and null devices. Optional GPU unavailability alone does not
fail the CLI's storage/model diagnostics. Successful hardware sampling remains
unverified in this environment: the actual CLI found the NVIDIA driver unavailable.
Parser and bounded same-binary child fixtures cover valid/missing/malformed values,
timeout, overflow and cleanup; they do not prove GPU integration.
