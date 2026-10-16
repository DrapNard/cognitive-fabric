# Shared inference resource admission

Architecture sections 4, 9 and 12 require Jev+ to coordinate compute, memory and
time across modules. `resource_scheduler::ResourcePool` implements operator-set
**logical admission within one Rust process**. Different backends and brokers can
share the same pool. The operator chooses capacities and working allocations from hardware/workload
knowledge. The optional `Config.resource_admission` now enables a pool for normal
CLI, Unix-service and HTTP-server inference, including native turns and bound
experts. Physical GPU enforcement, resident/backbone accounting and general
cross-workspace arbitration remain Partial.

## Host integration

```rust,no_run
use cogfabric::{backend::LocalBackend, config::Config, engine::Engine,
    resource_scheduler::{Allocation, ResourcePool, Settings}, store::Store};
use std::sync::Arc;

# fn configure() -> anyhow::Result<()> {
let cfg = Config::default();
let store = Arc::new(Store::open(&cfg.db())?);
let pool = ResourcePool::new("human-cli", store.clone(), Settings {
    label: "gpu.main".into(),
    capacity: Allocation {
        compute_slots: 1,
        host_memory_bytes: 2 * 1024 * 1024 * 1024,
        device_memory_bytes: 1024 * 1024 * 1024,
    },
    max_pending: 8,
    max_wait_ms: 5_000,
    max_execution_ms: 60_000,
})?;
let backend = pool.wrap_backend("human-cli", Arc::new(LocalBackend(cfg.clone())),
    Allocation {
        compute_slots: 1,
        host_memory_bytes: 512 * 1024 * 1024,
        device_memory_bytes: 512 * 1024 * 1024,
    })?;
let engine = Engine::with_backend(cfg, store, backend);
# let _ = (engine, pool);
# Ok(())
# }
```

These example sizes are declared working allocations, not measured model memory
requirements. Each wrapped backend reserves its fixed allocation per operation.
Register wrappers with `fabric::Broker::{register,register_candidate}` to cover
additional inference modules, or pass them to `Engine::with_backend` as above.
Use the **same Arc** for every competing backend. Clones and backend pins retain
that pool and allocation; fixed transport descriptors pass through unchanged.
Independent pools, unwrapped backends and other OS processes do not share limits.
Native hosts using explicit wrappers remain responsible for their additional
backends. With configured admission, Session wraps every primary/expert call and
the native agent wraps each turn, including managed worker dispatch. Do not also
wrap those paths manually with the same pool: nested reservations can deadlock
until their deadlines. The existing managed lifecycle still owns process reaping.

## Admission and time

A request must identify an existing running task and a supported role, pass the
compiled task/actor inference authorization, and satisfy ordinary prompt/output
bounds before admission. This adds no task budget. Session dispatch charges,
original task deadline, output/prompt/tool ceilings and independent verification
continue to apply outside the adapter. The pool never refunds those charges.

Requests acquire compute slots, host bytes and device bytes atomically. No request
holds one dimension while waiting for another. Strict FIFO admission prevents
smaller newcomers from bypassing an older large request; this deliberately allows
head-of-line blocking. Several compatible requests may dispatch together.
Allocations with zero compute slots or any demand above capacity are rejected at
installation. Settings are immutable and bounded to 256 compute slots, 2^60 bytes
per memory dimension, 1–1,024 waiting requests, 1–60,000 ms queue waiting and
1–600,000 ms execution. Active reservations also consume at least one slot, so
in-memory inventory is bounded by the slot and pending ceilings.

Queue overflow rejects immediately. Queue timeout or cancellation removes the
waiting entry and admits the next feasible prefix. Cancellation after grant but
before dispatch releases the reservation too. The configured execution deadline
starts at provider dispatch; the original task deadline may end it sooner. These
are cooperative Rust future deadlines: trusted synchronous callbacks cannot be
preempted. Shutdown blocks new dispatch, wakes pending callers and leaves already
executing provider requests under their existing deadlines.

## Audit and uncertain outcomes

`resource_intent` records the runtime pool UUID, immutable settings, allocation,
role and unique reservation UUID before queue admission. `resource_admitted`
records measured pool waiting, followed by `resource_dispatch` before provider
entry. These inserts must succeed. `resource_refused` records admission failure.
No prompt, answer, credential or provider error body is included in these events.

A successful provider future plus committed `resource_completed` releases its
reservation. Provider error, execution timeout, cancellation/panic after dispatch,
or failure to commit completion **retains the allocation as quarantined**. A
client disconnect does not establish provider/GPU shutdown. Drop cleanup performs
only in-memory accounting: cancellation may leave a durable dispatch with no
outcome record. `snapshot()` exposes bounded current reservations, phase and
allocated totals; it does not call the provider. `resource_outcome_unknown`
records observed errors/timeouts when the adapter still has control.

An operator must independently stop/check the provider, then call
`acknowledge_stopped("human-cli", reservation_id, true)` to free one quarantined
reservation. The compiled `resources.resolve` permission rejects all model actors.
An audit insert must succeed before removal; rejected audit or confirmation
leaves the reservation held. Simultaneous resolution attempts cannot free it
twice. This acknowledgement is operator testimony, not measured stop proof.
`resources.configure` similarly restricts construction, wrapping and shutdown to
the native operator context; actor strings are not remote authentication.

Normal task continuation currently rejects any recorded `resource_intent` before
backend construction. It cannot replace a missing original pool with unrestricted
inference. Explicit in-flight pool restoration remains incomplete. Configured hosts now
hold an exclusive private workspace resource lease; a second configured owner
is rejected. One pool is shared per Store across Engine clones and native turns,
and HTTP Server construction checks/reuses that pool. Different workspaces and
explicit native pools are not a global hardware allocator. Startup validates
bounded dispatch/completion/release history and refuses unresolved outcomes rather
than silently resetting their capacity. See the operator workflow below.

## Measurements and limits

The adapter measures pool waiting using Rust's monotonic clock. Direct adapter
reports expose that time as `queue_wait_ms`; a surrounding Fabric broker retains
its existing outer-queue metric. `resource_admitted.wait_ms` remains the explicit
pool measurement. Backend-supplied waiting claims cannot overwrite it.

The pool enforces declared arithmetic admission; it does not enforce cgroups,
CUDA allocation, resident weight/KV-cache identity, total RSS/VRAM, GPU seconds,
energy or isolation. Resident/shared-backbone accounting, hardware measurement,
provider ownership, process-wide persistent recovery and policy tuning remain
required for general GPU scheduling. It is not a model-quality benchmark.

## Regression evidence

`src/resource_scheduler/tests.rs` covers all resource dimensions, FIFO blocking,
bounded queues, timeouts, shutdown, backend pin/transport identity, task/role
rejection, audit failure, operator resolution, cancellation before/after grant,
provider failures, independent Engine brokers sharing capacity, original task
charges, and Engine cancellation through the actual broker worker. Recovery tests
prove a valid checkpoint becomes ineligible when original pool usage is recorded,
with task/event state unchanged on preview and resume denial. Backends are
scripted; these tests establish orchestration behavior rather than physical GPU
limits, real-model quality or speed improvement.

## Configure ordinary CLI and services

Save an operator-reviewed JSON file such as:

```json
{
  "settings": {
    "label": "gpu.main",
    "capacity": {"compute_slots": 1, "host_memory_bytes": 1073741824, "device_memory_bytes": 536870912},
    "max_pending": 8, "max_wait_ms": 5000, "max_execution_ms": 60000
  },
  "allocation": {"compute_slots": 1, "host_memory_bytes": 268435456, "device_memory_bytes": 268435456}
}
```

```bash
cogfabric configure-resources ./resources.json
cogfabric config-show
cogfabric ask "Explain Rust ownership" --mode fast
cogfabric resource-status
# After independently stopping/checking the provider, stop the old service first:
cogfabric resource-release RESERVATION_UUID --human-approve
```

The sizes are examples, not measured model requirements. A uniform allocation is
used for every configured call, so it must cover the largest admitted role/expert
working set. Resident weights are not included automatically. Configuration uses
a bounded regular file, strict JSON and validated settings/demand; malformed input
fails before state creation. Absent `resource_admission` retains old config bytes
and behavior. `configure-resources --disable` explicitly removes it. Existing
services retain their immutable settings until restarted; configuration output
reports that restart is required. Active Engine clones cannot reset/remove a live
pool by mutating their public Config. Different allocations must still fit the
unchanged pool ceiling.

`resource-status` opens only an existing database read-only and exposes sampled
unresolved reservation/task/pool identities. It does not inspect provider health
or modify task state. `resource-release` requires approval before config access,
opens an existing current-schema database without bootstrap, takes exclusive
resource and runtime leases, revalidates history, and audits an exact operator stop
acknowledgement. It does not kill a provider or fabricate stop proof. Audit failure
retains the unresolved reservation. Unknown, duplicate, mismatched or ambiguous
lifecycle records deny startup/reconciliation.

Startup/inspection reads at most 4,096 lifecycle records, at most 4,096 bytes each,
under a SQLite instruction budget (30,000 callbacks every 1,000 instructions).
An oversized history requires reviewed retention before restart. Retention now
validates per-task lifecycle history and preserves tasks with unresolved dispatches;
it cannot erase their conservative startup gate. Applying retention excludes a live
configured host through the resource lease. Fully completed or explicitly released
terminal tasks remain eligible under the existing reviewed retention rules.

Coverage now includes actual configure/ask/status/release CLI processes, configured
managed inference and reaping, primary/bound-backend Session calls, Engine/native
sharing, workspace-owner exclusion, tampered audit denial, retention protection,
and real loopback HTTP native/collective contention. These remain fixture results.
