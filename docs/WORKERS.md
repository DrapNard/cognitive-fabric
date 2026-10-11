# Managed local inference workers

Managed inference is opt-in. Enable it with
`cogfabric configure-worker --enabled true`, then submit tasks normally. Disable
it with `cogfabric configure-worker --enabled false`. `config-show` reports the
persisted `inference_process` setting. Existing configurations default to false.
The setting applies when starting a new CLI/Engine/service; restart an existing
service to change it. The demo backend remains explicitly a demo in this mode.

The Rust runtime lazily launches its own executable at the hidden inference-worker
entry point on a task's first model call. Arithmetic and other tasks requiring no
inference launch no process. One worker serves that task's roles sequentially and
shares the configured external HTTP inference server; it does not load separate
model weights per role. Only the default inference registry supports this managed
mode. Custom registries fail explicitly rather than being silently replaced.
Injected test backends use their supplied implementation instead.

The host creates a unique private 0700 temporary directory and 0600 Unix socket.
Both peers check approved OS UID/PID credentials. The authenticated COGB bootstrap
pins the task UUID, generation settings, role profiles and cumulative ceilings.
Database limits are appended compatibly with the original 256 MB default. The
worker receives the workspace path through trusted CLI setup, reads no mutable
configuration file and inherits no environment variables. Its SQLite audit anchor
must already exist. Prompts/results then use bounded correlated COGM FlatBuffers.
See `PROTOCOL.md` for frame, task-grant and session limits.

Each Engine task keeps ownership of the child and closes its connection when
execution ends or is cancelled. It waits up to two seconds for exit, then requests
termination and waits up to two more seconds for reaping. Cleanup failures become
task failures. Normal idle disconnect records session_stopped; partial frames and
active disconnects are failures/cancellations rather than successful inference.
Tokio kill-on-drop is a fallback for dropped ownership. It is not a guarantee after
runtime/process crashes. Socket cleanup checks directory and socket identities.
No automatic restart, task replay or permission expansion occurs on failure.

The execution wall-clock ceiling ends reasoning; child cleanup may add up to four
seconds plus existing database/I/O delays. Worker sessions currently have their
independent 300-second lifetime, 1024-exchange bound and ten-second idle deadline.
A longer configured task may encounter that tighter worker ceiling. This is a
documented limitation of the initial lifecycle integration.

The worker_process audit records PID and launch_handoff_ms, measured through
spawn, connection and bootstrap send. This does not measure model loading or prove
the worker/provider is ready. worker_process_stopped records exit code, whether
termination was forced and confirmed reaping. Metadata-only worker.audit events
record authorization and outcomes. Parent RSS counters exclude worker/backend
processes and GPU memory. Process coordination adds latency; no speedup is claimed.

This is same-user process separation, not an executable sandbox or weight
attestation. Workers can open the owner's database and call the approved local
HTTP endpoint. No model-supplied executable path, shell command or process-launch
tool is exposed. Arbitrary executable experts still require separate isolation
and review. External servers may continue GPU work after disconnect. Abrupt worker
or host death can leave incomplete audit sessions and temporary directories;
operator-reviewed crash reconciliation remains unfinished.

Integration tests launch the actual CLI binary with the demo backend, verify one
worker reused across a task, ensure arithmetic launches none, and cancel a pending
loopback HTTP request with Ctrl-C while confirming child reaping. These fixtures
prove process plumbing and cancellation, not real model quality or GPU operation.
