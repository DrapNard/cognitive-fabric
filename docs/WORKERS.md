# Managed local inference workers

Managed inference is opt-in. Enable it with
`cogfabric configure-worker --enabled true`, then submit tasks normally. Disable
it with `cogfabric configure-worker --enabled false`. `config-show` reports the
persisted `inference_process` setting. Existing configurations default to false.
The setting applies when starting a new CLI/Engine/service; restart an existing
service to change it. The demo backend remains explicitly a demo in this mode.

The Rust runtime lazily launches its own executable at the hidden inference-worker
entry point on a task's first model call. Arithmetic and other tasks requiring no
inference launch no inference process. One worker serves that task's roles sequentially and
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
seconds per child plus existing database/I/O delays. A registered-model workflow
may own one additional expert child, allowing up to eight seconds for both. Managed worker lifetime follows the
task's approved max_wall_seconds (1–3600 seconds), with 1024 exchanges maximum.
Trusted standalone worker authorities default to 300 seconds and may explicitly
select a bounded lifetime. Waiting between calls consumes that lifetime; once a
frame begins, its remaining prefix/body must arrive within ten seconds. Individual
inference RPCs retain their independent 300-second ceiling. Legacy bootstrap
frames without the appended lifetime field retain the 300-second worker default.

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
explicit audit reconciliation is available below; stale-resource cleanup remains
separate operator work.

Worker audit reconciliation is now available through `recover-workers`; persistent
workers and host startup hold cooperative execution leases, so recovery refuses
while either is active. It marks open sessions interrupted without changing task
answers, replaying inference or killing recorded PIDs. See `RECOVERY.md` for bounds
and failure behavior. Stale temporary-directory cleanup remains unfinished.

Integration tests launch the actual CLI binary with the demo backend, verify one
worker reused across a task, ensure arithmetic launches none, and cancel a pending
loopback HTTP request with Ctrl-C while confirming child reaping. These fixtures
prove process plumbing and cancellation, not real model quality or GPU operation.


### Managed built-in arithmetic workers

`configure-tool-worker --enabled true` persists `arithmetic_process` (default
false); restart a running service to apply it. Disable with the same command and
`--enabled false`. Direct and socket `ask 'calc: 6*7' --mode fast` now lazily launch
the same Rust binary as an arithmetic worker, using the existing bounded broker
and COGT commands. No inference worker/model call is needed for arithmetic.
The default arithmetic registry is required; custom versions are rejected before
task creation rather than silently replaced. Other capabilities remain in-process.

`protocol/tool_bootstrap.fbs` defines verified COTB v1, bounded to 2 KiB with
two-second read/write deadlines. Both peers authenticate approved UID/PID before
bootstrap. Trusted setup grants exactly one task, built-in module/version and one
arithmetic call; its lifetime is at most 30 seconds and conservatively rounds up
remaining task seconds. The host still enforces its original precise deadline.
The worker opens the matching private workspace database with the pinned quota;
no mutable generation settings, model credentials or inherited environment are used.
Permission/tool-budget admission precedes lazy launch and the host independently
verifies the returned arithmetic answer.

Launch intent, owned PID, bootstrap delivery and confirmed reaping are distinct
audit records. Ownership begins at spawn and survives cancelled/failed handoff;
normal task cleanup closes the connection, waits two seconds, then kills/reaps
with a further two-second limit. Directory/socket cleanup checks inode/device
identity. Cleanup errors fail the task. This also applies after execution errors
and cancellation. Process/runtime crashes can still leave stale files or unknown
cleanup outcomes; no automatic crash cleanup or executable sandbox is claimed.
This supersedes the arithmetic-specific provisioning/CLI lifecycle limitations
above. General executable provisioning and non-arithmetic command routing remain
**Partial / Not implemented**. Process overhead is not a measured speedup.
