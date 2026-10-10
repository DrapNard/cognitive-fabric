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
