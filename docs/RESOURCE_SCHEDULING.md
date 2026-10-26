# Shared inference resource admission

Architecture sections 4, 9 and 12 require Jev+ to coordinate compute, memory and
time across modules. `resource_scheduler::ResourcePool` implements operator-set
**logical admission within one Rust process**. Different backends and brokers can
share the same pool. The operator chooses capacities and working allocations from hardware/workload
knowledge. The optional `Config.resource_admission` now enables a pool for normal
CLI, Unix-service and HTTP-server inference, including native turns and bound
experts. Physical GPU enforcement, measured/dynamic residency and general
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
requirements. Without working profiles, each wrapped backend reserves its fixed
allocation per operation; configured profiles select the role/target allocation.
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

## Optional GPU observation admission

An operator may add `resource_admission.gpu_observation` to require one fresh
whole-device sample before each configured provider dispatch:

```json
"gpu_observation": {
  "device_uuid": "GPU-0123456789abcdef",
  "sensor": "nvidia-smi",
  "headroom_bytes": 536870912
}
```

Replace the example UUID with one returned by `diagnostics --probe-gpu`. Add
this field to your existing resource configuration and apply it with
`configure-resources FILE`; running services require a restart. The sensor is
explicit (`nvidia-smi` or `nvml`); no fallback is attempted. The
runtime waits for the shared logical pool reservation, samples the selected
device, and requires its measured `total - used` bytes to be at least the
dispatch's declared `allocation.device_memory_bytes` plus operator headroom.
Missing devices or metrics, malformed/duplicate identities, unavailable sensors,
insufficient headroom, and a query completed more than two seconds before
dispatch all fail closed. Query duration is recorded separately; the driver does
not supply an independently verified measurement timestamp. Sampling is bounded by the existing GPU child timeout/cleanup and is
cancellable by the original task deadline. A pre-dispatch denial releases the
Reserved logical slot and retains the task's already charged model call. If the
sample ages out during the synchronous dispatch audit write, the provider is not
invoked, but the persisted dispatch remains quarantined until operator
reconciliation because process interruption could leave an ambiguous boundary.

This is a best-effort preflight, not a CUDA quota, reservation in the driver,
cross-process arbitration, or proof of provider stop. The check covers only the
operator-declared working allocation for this call plus headroom. It does not
estimate or subtract loaded model weights, KV cache, other process memory, or
future load requirements; a successful sample does not prove a first-use model
will fit. It applies before inference and supported lazy v3 activation, not manual
operator load commands or models loaded externally. Eager native policy versions
1 and 2 are refused when the guard is enabled; select version 3 for guarded
first-use activation. The default `None` omits the
setting from saved configuration and status, preserving legacy behavior. Exact
sensor, UUID and headroom are pinned into resource bindings and checked during
continuation; prior observations are never recreated by re-sampling.

## Audit and uncertain outcomes

`resource_intent` records the runtime pool UUID, immutable settings, allocation,
role and unique reservation UUID before queue admission. `resource_admitted`
records measured pool waiting, followed by `resource_dispatch` before provider
entry. These inserts must succeed. `resource_refused` records admission failure.
No prompt, answer, credential or provider error body is included in these events.

On-demand wave preparation uses a separate `resource_preparation_intent` and
reservation UUID before the same admission chain. Preparation acquires the
configured role/model working allocation, validates resident coverage and any
GPU observation, then invokes only the trusted backend lifecycle hook. It does
not dispatch or complete an inference call. Generation subsequently acquires its
own lease. Warm native bindings and adapters with no preparation work skip this
extra lease; this capability is trusted adapter metadata, not residency evidence.

The preparation allocation covers the load operation, not the lifetime of the
loaded weights. Persistent weight footprints must be represented in the resident
catalog to participate in declared pool capacity after the operation lease is
released. An empty catalog does not establish weight-memory accounting or a
physical VRAM limit. GPU observations are temporary admission evidence only.

Preparation errors or cancellation after resource dispatch use the same
quarantine and operator-stop reconciliation as inference. Recovery correlates
preparation markers with complete matching role/pool resource chains separately
from model-call chains, with at most 64 preparations and 1,536 resource-related
events per task. A missing or repeated marker/chain refuses continuation. These
changes have static-check evidence only; no live preparation or GPU behavior was
verified and no speed improvement is claimed.

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

Normal task continuation can now restore compiled configured pools at supported
ready checkpoints, under operator approval and strict original-history validation.
Native custom pools absent from original execution settings remain unavailable.
See the restoration section below. Configured hosts hold an exclusive private
workspace resource lease; a second configured owner is rejected. One pool is shared
per Store across Engine clones and native turns. Different workspaces and explicit
native pools are not a global hardware allocator. Startup still refuses unresolved
workspace dispatches rather than resetting their capacity.

## Measurements and limits

The adapter measures pool waiting using Rust's monotonic clock. Direct adapter
reports expose that time as `queue_wait_ms`; a surrounding Fabric broker retains
its existing outer-queue metric. `resource_admitted.wait_ms` remains the explicit
pool measurement. Backend-supplied waiting claims cannot overwrite it.

The pool enforces declared arithmetic admission; it does not enforce cgroups,
CUDA allocation, resident weight/KV-cache identity, total RSS/VRAM, GPU seconds,
energy or isolation. Measured resident/shared-backbone accounting, hardware measurement,
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

The sizes are examples, not measured model requirements. Without working profiles,
the uniform allocation covers every configured call and must cover the largest
admitted role/expert working set. Resident weights are not included automatically. Configuration uses
a bounded regular file, strict JSON and validated settings/demand; malformed input
fails before state creation. Absent `resource_admission` retains old config bytes
and behavior. `configure-resources --disable` explicitly removes it. Existing
services retain their immutable settings until restarted; configuration output
reports that restart is required. Active Engine clones cannot reset/remove a live
pool by mutating their public Config. Different allocations must still fit the
unchanged pool ceiling.

### Target-specific working profiles

Optional `working_profiles` gives separate operator-declared working allocations
to canonical local model targets and runtime roles. It is part of the resource
configuration, not model output or a capability declaration. For example, a pool
with sufficient capacity can declare:

```json
"working_profiles": [
  {
    "id": "small-planner",
    "target": {"api_base": "http://127.0.0.1:1234/v1", "model": "small"},
    "roles": ["planner"],
    "allocation": {"compute_slots": 1, "host_memory_bytes": 134217728, "device_memory_bytes": 134217728}
  },
  {
    "id": "solver",
    "target": {"api_base": "http://127.0.0.1:1234/v1", "model": "reasoner"},
    "roles": ["solver"],
    "allocation": {"compute_slots": 1, "host_memory_bytes": 536870912, "device_memory_bytes": 536870912}
  }
]
```

Place this field beside `settings`, `allocation` and optional `residents` in the
file passed to `configure-resources`. These are example logical sizes, not measured
requirements. Profiles do not select models: the original pinned model/role
configuration still determines each target. Every invoked role/target needs a
matching profile when the table is nonempty. Unknown, deterministic and unlisted
targets cannot silently fall back to `allocation` under an active profile table.
Absent or empty profiles retain the original uniform behavior and serialization.

The table has at most 32 profiles and 16,000 canonical JSON bytes. Each profile has
a bounded unique identifier, canonical loopback model target, valid unique roles
and a working allocation that fits capacity after the fixed resident footprint.
Overlapping role/target entries are rejected. Working memory remains distinct from
once-per-pool resident weights.

The runtime pins the backend before resolving its target and retains that same
backend while queued and during dispatch. Required profile intent and selection
audits bind the role, exact target, canonical table digest, selected profile and
allocation before provider execution. Shared queue capacity, original task
deadlines, per-call authorization, resource quarantine and operator reconciliation
remain authoritative. A smaller declared profile cannot expand any task allowance.
Native conversation turns require an explicit `ChatBackend` snapshot when profiles
are enabled. The local and managed adapters retain their immutable target settings;
a custom chat adapter without snapshot support is refused before dispatch.
Recovery must reconstruct the original profile decision and allocation rather than
selecting from current configuration or granting new resources.

This supports heterogeneous logical admission. It does not load/unload models,
choose an alternate broker route, attest adapter metadata or enforce physical
GPU/RSS usage. Automatic ownership-aware residency remains a separate milestone.

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

## Resident backbone declarations

An optional `residents` array in the resource configuration reserves host/device
memory for the pool's entire lifetime. This is useful when a backbone is kept
available across many requests/roles. Declare shared consumers in one entry:

```json
{
  "id": "backbone.small",
  "version": "1.0.0",
  "weights_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "host_memory_bytes": 1073741824,
  "device_memory_bytes": 4294967296,
  "consumers": [
    {"api_base": "http://127.0.0.1:1234/v1", "model": "shared-solver"},
    {"api_base": "http://127.0.0.1:1234/v1", "model": "shared-critic"}
  ]
}
```

The digest and sizes above are placeholders for the example. Operators must supply
their reviewed declarations and a capacity large enough for both resident and
working allocations. Each backbone entry is charged **once**, independently of its
consumer count. Distinct IDs/versions are charged separately; duplicate identities
are rejected rather than silently deduplicated. Consumer names are declared coverage metadata, never model registrations,
route selections or permission grants. With a nonempty catalog, dispatch now checks the selected adapter's configured
endpoint/model against at least one declared consumer. This still does not attest
actual loaded weights or the completeness of declared memory footprints.

Validation allows at most 128 backbone entries, 32 consumers per entry, 512
consumers total and 16,000 canonical JSON bytes. Field lengths are bounded before
cloning the catalog. IDs and semantic versions are canonical, weight digests use
64 lowercase hexadecimal characters, memory uses checked addition and must fit
capacity. Every entry needs a nonzero footprint and at least one consumer.
Consumer endpoints use existing literal-loopback restrictions without sending any
HTTP request. Canonical endpoints/order reject duplicate aliases and make catalog
identity independent of list ordering. Unknown fields and duplicate decoded keys
remain rejected by configuration parsing.

A live pool pins its canonical catalog. Reordering entries/consumers does not
change it; changing footprints, identity, digest or consumer contents cannot reset
a live pool. The operator must restart under the existing startup ownership and
uncertain-outcome checks to apply a changed catalog. Native hosts can explicitly
construct `ResourcePool::with_residents` under `resources.configure`; their
existing native lifecycle responsibilities still apply. Ordinary configured CLI,
services, native agent and bound expert calls inherit the same resident charges.

Snapshots expose `allocated` (resident + working), `resident_allocated`,
`working_allocated`, the canonical catalog and its digest. Compute slots belong to
working requests; static residents reserve only memory. Idle, completed, cancelled
and quarantined request transitions leave the resident base intact. Releasing an
uncertain request frees only that request's working allocation. Resource intent
audits include the canonical digest and resident totals; configured execution
settings preserve the original full catalog. Empty catalogs are omitted from
serialized configuration, preserving old configuration fingerprints.

All reports keep `residency_attested:false` and `physical_limits_enforced:false`.
No weight file is opened, model loaded/unloaded, provider ownership established,
GPU memory measured or shared-backbone implementation proved. Incorrect/incomplete
operator declarations can undercount real memory. Actual resident-instance
discovery/ownership, measured footprints, dynamic lifecycle scheduling and safe
pool restoration remain engineering work. These tests do not demonstrate faster
or better neural reasoning.

## Selected-target coverage

A nonempty catalog enables a required adapter-target check before queue admission.
The canonical endpoint and selected role model must appear among its consumers.
`LocalBackend` and managed inference derive this identity from the same immutable
configuration used for generation; role-profile overrides are included. Native
`LocalChat` and managed native turns report the native model they actually request.
They do not apply role-profile selection. The demonstration adapter explicitly
identifies deterministic computation and needs no resident model match. Its
resident base, if configured, is still held.

Trusted hosts implement `InferenceBackend::inference_target(role)` or
`ChatBackend::inference_target()`. Defaults return unknown. Metadata callbacks must
be short, nonblocking configuration queries; they must not perform inference/I/O
or treat model output as identity. Unknown, malformed or unlisted targets cannot
use a nonempty catalog. Existing empty catalogs skip these queries entirely. Raw
Unix adapters currently report unknown because their transport identity alone
does not establish a model target. Host-supplied code remains trusted rather than
sandboxed or executable-attested. A native implementation can lie about its target.

The scheduled adapter pins dynamic brokers before looking up target metadata.
Pinned routes query their retained module, so replacement of registry entries
between observation and dispatch does not redirect the checked request. Local and
managed targets are fixed by construction; custom native adapters must preserve
that property themselves. Bare metadata inspection of an unpinned Broker is a
sample, not a promise of future route choice.

`resource_target_intent` commits before the target callback; `resource_target`
commits its coverage/denial before the resource intent and any backend dispatch.
Allowed entries include canonical target kind/identity and matched backbone
identities, bound to the canonical catalog hash. Deterministic entries have no
matched resident consumer. Rejected entries omit raw invalid target/error bodies.
Callbacks execute outside Store, pool and registry locks; an audit failure prevents
the next action. Session's prior call/output/prompt reservation remains spent on
denial. No fallback/retry, provider call, model load or new permission is inferred.

Coverage establishes which declared consumer applies to the configured request.
It cannot establish that an HTTP provider honored the requested model, loaded the
declared digest, actually shares backbones, or stays inside the declared footprint.
Provider-returned model labels cannot approve coverage. `weights_attested:false`,
`residency_attested:false` and `physical_limits_enforced:false` remain explicit.
Real ownership/measurement and dynamic load/unload remain separate requirements.

## Restore an interrupted configured task

Use the existing operator workflow:

```bash
cogfabric resume-preview TASK_UUID
cogfabric resume-task TASK_UUID --human-approve
```

The saved execution settings govern restoration. Changing current CLI config cannot
enlarge the original task's pool, working allocation or resident catalog. The pool
may have a new runtime UUID, but it retains the original settings/binding. A matching
live pool on the same Store can be reused without resetting its counters. A closed
pool, a live reservation for the task, an incompatible catalog or another workspace
owner denies restoration. Preview samples eligibility without constructing a backend
or changing database state; resume always revalidates.

The strict original-history validator reads at most 768 bounded records (16,384
bytes each) and requires each charged call to have the correct resource chain.
Pool/reservation UUIDs, source/version, role, settings, working allocation, resident
footprint/digest, target intent/coverage, admission and successful completion must
match in order. Duplicate, missing, ambiguous, denied, released-as-uncertain or
changed records cannot buy new inference. Current nonempty catalogs require target
audits. Old empty catalogs can omit all three resident metadata fields together;
partial omission is rejected. Any original native pool with no configured settings
remains unsupported rather than being replaced.

Calls after the immutable reasoning context must have the pinned local/role target.
Earlier completed expert advice can have its own declared covered target; that
advice remains untrusted and is never replayed. Global bounded lifecycle validation
checks unresolved workspace dispatches under the caller's original admission SQL
deadline, without replacing its progress handler. Resource release/reconciliation
does not reconstruct missing model answers or certify uncertain inference.

Approved restoration holds runtime ownership, validates a read-only preflight,
constructs/reuses the compiled configured pool outside Store locks, then revalidates
the complete original configuration/history in an immediate admission transaction.
The phases share a five-second deadline. Audit failure rolls admission back before
backend construction or worker launch. Managed continuation keeps its existing
lease conversion, reduced worker grants and reaping lifecycle. Task clocks and
resource charges are reconstructed from original records; they are not restarted.

Reports disclose prior pool IDs, completed resource-call count, canonical resident
binding and the restored pool identity with physical limits still false. Direct
HTTP and actual CLI tests resume only the missing solver; graph tests resume only
a pending advisory node and final solver. Initial-solver, legacy-empty, cancelled,
corrupt, audit-failing and contended-ownership cases are covered. Test prefixes are
scripted Engine runs truncated at real recorded checkpoints, not crash/GPU or
neural-quality evidence. Full within-call recovery, arbitrary native pool code,
provider ownership/stop proof and physical memory enforcement remain incomplete.


## Reviewed provider reconciliation

[RESIDENCY_PLANS.md](RESIDENCY_PLANS.md) describes the separate operator planner
for exact loaded-context reuse, explicit release and missing installed model loads.
Both batch application and the existing individual residency mutations now refuse
while a configured pool owns this workspace, even if no inference is active. This
prevents those commands from replacing a live pool's declared model set. Planning
and inspection remain read-only. This workflow does not prove provider ownership,
synchronize the resident catalog automatically, measure footprints or grant models
authority to load/unload themselves.

## Optional role priority with bounded bypass

FIFO remains the default. An operator may add the following `scheduling` object
to the existing resource configuration's `settings`:

```json
{
  "scheduling": {
    "role_priorities": {"solver": 15, "interface": 8, "critic": 2},
    "max_bypasses": 4
  }
}
```

Apply the complete reviewed configuration using the existing
`configure-resources FILE` command. The fragment above is not a complete resource
configuration. Native Rust `Settings` constructors now specify `scheduling: None`
for FIFO or `Some(PriorityScheduling { ... })`. Omitted JSON fields preserve old
serialized settings and FIFO audit/status shapes. No model actor gains configuration
permission; a live policy change requires a new host under existing workspace
ownership rules. Immutable pool identity includes the policy.

Priorities are integers 0–15 for the seven existing inference roles: interface,
planner, solver, explorer, critic, reflector and synthesizer. Unlisted roles get
priority zero. The map must contain 1–7 known roles, and `max_bypasses` is 1–64.
Priority comes from trusted runtime role classification and fixed operator settings,
not prompt text, model confidence or client-supplied urgency. All solver-role calls
share the same configured priority, including advisory subtask/expert solver calls.

Among waiting requests, higher priority wins and equal priority is FIFO. When a
later request reserves capacity, each earlier waiting request gains one bypass.
After reaching its ceiling, an older protected request wins ahead of later arrivals;
the oldest protected request wins ties. Counts apply to capacity reservations,
even if a later audit fails or a reserved request is cancelled. Cancelling a request
while it is still waiting does not increment other requests' counts.

Running work is never preempted. If the selected/protected request cannot fit its
complete allocation, smaller newcomers wait even if they could fit. This prevents
small requests from bypassing protected large ones indefinitely. The policy bounds
bypasses, not wall latency: waiting deadlines, quarantine, shutdown and real capacity
still determine whether any request can run. It is not a globally optimal or
work-conserving scheduler, and no throughput/quality gain is established.

Optional policies add `queue_priority` to intent/admission audits and
`queue_bypasses` to admission audits; current reservation snapshots expose both.
Audits must commit before provider work. Recovery requires the exact original
settings, role-derived priority and a recorded bypass count within the original
ceiling. FIFO history rejects forged priority fields. This validates the saved
local record, not a retrospective proof of every cross-task ordering decision.
Supported continuation restores the original compiled policy under original budgets;
metadata cannot load executable policy code or release uncertain provider work.

Declared compute/host/device allocations, resident memory accounting, bounded queues,
timeouts, task authorization, cancellation and unknown-outcome quarantine remain
independent of priority. The feature provides no physical GPU/RSS enforcement,
provider-stop proof, cross-workspace arbitration or learned expected utility.

## Initial resource-aware Broker selection

Working profiles or resident declarations activate eligibility checks before a
new task pins its inference routes. Candidates retain rank/load/identity ordering;
only candidates whose canonical targets satisfy the declared role allocation and
resident coverage may be selected. Missing roles remain unavailable. No inference
is required for an otherwise deterministic task. With neither catalog configured,
the legacy routing path and audit shape remain unchanged.

`routing_resource_intent` precedes metadata callbacks and
`routing_resource_selection` records the selected role map and bounded fixed-code
exclusions before generation. Registry changes during observation invalidate the
selection. Once published, routes never migrate or retry after failure. Final
shared admission, queue bounds, deadlines and quarantine remain authoritative.
The stable route-scope digest binds policy evidence to catalog and selected
module/version/target/profile allocations, excluding transient pool IDs and load.
These declarations neither load weights nor prove physical capacity or utility.

Verification on frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**899 passed across 57 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Initial new-test compilation/fixture errors and lint findings were corrected;
the queue test now uses actual same-task parallel work because the Store execution
lease prevents simultaneous task runs. Existing socket/process, migrations, mock
inference, malformed-frame, policy-denial, cancellation and rollback tests remain
green. Schemas/bindings are unchanged; bundled FlatBuffers pairing/integrity checks
pass. `flatc` is unavailable, so fresh generation was not performed. Localhost
port 1234 refused discovery; no real-model/GPU quality or speed claim is made.
