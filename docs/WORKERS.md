# Managed local inference workers

On Linux, managed inference, arithmetic and SQL workers execute `/proc/self/exe`
to retain the parent's running image when an upgrade replaces the original
pathname. Procfs must be available; launch errors remain visible. Other Unix
platforms use `current_exe()` and do not have this replacement guarantee. This
provides version consistency, not executable attestation or a hostile-host sandbox.

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

### Managed built-in SQL workers

`configure-sql-worker --enabled true` persists `sql_process` (default false).
Disable with `--enabled false`; restart an existing service to apply changes.
Direct and socket SQL asks lazily launch one same-binary `sql-worker` process;
other task types launch no SQL child. Arithmetic/inference switches are independent.
Task budget and permission admission precede launch, and the original task deadline
still applies. The default `sql.local` 1.0.0 registry is required; custom replacement
modules are refused before task creation rather than silently substituted.

The shared launcher retains child ownership through failed/cancelled startup,
uses private sockets, clears the environment, authenticates UID/PID, and sends
one-call SQL-only COTB v2 setup. Arithmetic-only COTB v1 remains unchanged; mismatched
versions/capabilities and worker entry points are rejected. Both use bounded setup
frames/deadlines, database quotas and at most 30-second worker sessions. SQL commands
use structured COGQ v1, with the existing inline-only SQLite restrictions. The host
reruns the compiled SQLite checker before accepting results; no model call is added.

Normal, failed and cancelled tasks close the stream, wait up to two seconds, then
kill/reap with a further two-second ceiling. Inode/device checks protect socket
cleanup. Actual CLI/service tests verify SQL success, unsafe-query cleanup and
budget refusal without a child. These workers are trusted Rust processes with
workspace access, not an executable sandbox. Runtime crashes can still leave stale
resources; general executable loading and automatic crash cleanup remain unfinished.

Inference startup now retains child/socket/execution-lease ownership before any
connection or bootstrap await. `worker_process_launch` records intent before spawn;
`worker_process_owned` records the owned PID before handoff. Failed or cancelled
initialization leaves that child available for the same bounded shutdown/reaping
path even when the worker's initialization cell is empty. An unfinished startup
cannot implicitly retry and create another child. Actual child regressions cover
cancellation, failed ownership audit, refusal to retry and identity-checked socket
cleanup. The existing `worker_process` event still measures completed handoff;
none of these records proves loaded weights or external GPU cancellation. Runtime
crashes and stale-resource cleanup remain open.

`recover-workers` now includes abandoned arithmetic/SQL audit sessions alongside
inference. Live tool leases block it; mixed recovery is transactional, bounded and
idempotent, preserving task results and existing call charges. It records unknown
completion rather than replaying tools or killing PIDs. See `RECOVERY.md` for
metadata validation and legacy audit compatibility. Temporary-file cleanup and
broader host-crash recovery remain separate work.

Journaled managed-worker socket cleanup is now available through reviewed
`worker-files-preview` / `worker-files-apply` commands. Launchers journal private
socket identities before spawning; cleanup requires an exclusive lease and exact
identity checks. See `RECOVERY.md` for pagination, partial failures and Linux
requirements. This supersedes the blanket stale-directory limitation above;
older unjournaled resources and automatic crash cleanup remain unsupported.

Managed inference and tool wrappers propagate their initialized Unix adapter's
availability rather than always reporting unknown. Uninitialized inference stays
unknown; an uninitialized tool whose original host deadline elapsed reports
expired. Inventory observation does not trigger worker launch or renew authority.
Confirmed shutdown leaves initialized adapters closed. These observations do not
prove external provider completion, model residency or executable health.

Managed wrapper shutdown is terminal, including before first use. Shutdown marks
the wrapper stopped before awaiting the initialization gate; later calls cannot
launch it again. The gate serializes handoff publication with cleanup inspection.
Existing bounded startup waits still apply, so shutdown may wait for an already
running initialization before reaping its owned child. Repeated shutdown can retry
cleanup. Stopped availability means dispatch is prohibited, not that process
reaping or external provider completion is already proven; use the cleanup audit.

Retained module inventory snapshots membership and active flags under the
registry lock, then observes adapter availability outside that lock. Availability
can change after the snapshot; this is an observation rather than a dispatch
guarantee. Trusted observation callbacks can inspect registry metadata without
holding its inventory lock. No backend probe, inference or permission renewal
is performed. Arbitrary backend callbacks remain trusted host code.

## Native conversation workers

`configure-worker --enabled true` now applies to `cogfabric-agent` native turns
as well as existing collective inference. The core `agent::process` wrapper
lazily launches the same running image through `agent-worker`, exchanges trusted
FlatBuffers bootstrap settings and one native conversation packet, then reaps
its child. `agent::unix` checks UID/PID peers, exact task/correlation/expiry, a
non-deserializable one-call grant and proposal/output/input ceilings. Its only
capability is `llm.generate`; returned client tool calls are data, never executed.

Ownership includes the socket journal/guard, shared execution lease, process
handle and capacity permit until actual waiting confirms exit. At most eight
native children per host process can remain owned, including background cleanup
after an entire runtime future is dropped. Explicit completion/cancellation
awaits cleanup; forced cleanup is recorded when required. Runtime/host death or
unavailable audit storage may still leave cleanup unknown, without a false reap
claim. Normal disconnect/shutdown tests verify actual provider-future drops and
child reaping; external GPU work remains unverifiable.

`max_tool_calls` is appended to trusted bootstrap with default four for old
frames, preserving existing role workers. The new field bounds proposals in
native mode and does not grant tool execution. Both supplied executables support
the hidden native worker entry point. Other library embeddings enabling managed
mode must delegate it to `agent::process::run_worker`. Native role overrides
and arbitrary executable provisioning remain unsupported.

HTTP shutdown also drains native ownership before returning to the embedding
runtime, including reapers scheduled after disconnected request futures drop.
The drain is bounded to ten seconds. Cleanup failures remain visible for the host
lifetime, disable further native children and make shutdown fail; a successful
shutdown is never inferred merely from closing the HTTP listener.
