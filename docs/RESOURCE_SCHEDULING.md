# Shared inference resource admission

Architecture sections 4, 9 and 12 require Jev+ to coordinate compute, memory and
time across modules. `resource_scheduler::ResourcePool` implements operator-set
**logical admission within one Rust process**. Different backends and brokers can
share the same pool. The default CLI does not create a pool automatically: the
operator must choose capacities and per-backend allocations from their hardware
and workload knowledge. Native installation is implemented; physical GPU memory
admission, residency accounting and cross-process arbitration remain Partial.

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
A host must wrap additional expert/managed backends explicitly. Automatic managed
worker provisioning and default CLI/HTTP configuration are not yet integrated.

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
inference. Explicit pool restoration and durable reconstruction after host death
remain incomplete. Pool UUIDs distinguish runtimes; counters are process-local,
not an OS-wide lock or a durable global allocator. Restarting/reconfiguring a
trusted host is not automatically safe while external provider work is uncertain.

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
