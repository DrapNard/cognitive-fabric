# Typed module execution

Typed dispatch, bounds, policy denials and worker cleanup are covered by the
combined automated suite; see `CURRENT_IMPLEMENTATION.md` for exact evidence.
These tests do not establish a sandbox for trusted native callbacks.

## Contract and task ownership

`fabric::modules` exposes a finite typed request/result contract for arithmetic,
inline-data SQL, sorting, dependency ordering, shortest-path diagnostics and bounded
finite-domain assignments. A trusted Rust host registers an immutable descriptor
and implementation. There are no executable paths, credentials or permission
fields in descriptors. Requests and responses remain advisory data.

At admission, Engine pins the active module catalog to the task UUID and records
its versions and capabilities. Replacing or deactivating a version affects future
pins. A failed/closed queue is excluded from new discovery, pinning, activation
and dispatch; an old pin cannot silently switch implementations.

Each dispatch intersects the exact pinned descriptor, compiled policy, original
Session task policy and a bounded task/actor grant. Tool charges and required
intent audits precede execution. Non-cloneable authorization contexts bind one
request, module version, correlation ID and original deadline. Direct process
adapter calls without that context are denied. No failure refunds calls or
creates an automatic retry.

## Same-image worker

```sh
cogfabric --typed-module-worker ask 'calc: (7 + 5) * 3'
```

`Engine::with_builtin_module_worker` registers the compiled
`builtin.typed@1.0.0` implementation. The CLI worker is a fixed hidden entry point
in the same application image; it does not select an arbitrary binary or shell.
Internal messages use the versioned `protocol/general_module.fbs` unions and
verified bounded FlatBuffers frames, not JSON.

A private socket directory, restrictive socket permissions and mutual peer
UID/PID checks bind the host and child. The child rechecks the exact compiled
capability and task identity. Queues, concurrency, frame sizes and deadlines are
bounded. Cancellation retains process/socket/runtime ownership until the child is
reaped. A native lifecycle owner can retain its existing exclusive lease through
the same reaper without acquiring a conflicting second lock.

The same-image process is trusted compiled code, not a sandbox for arbitrary
native executables. In-process Rust implementations are also trusted host code.
A separate optional Wasm verifier path has a restricted no-import ABI; see
`VERIFICATION.md`.

## Expert provider bindings

An immutable expert manifest can optionally name a provider:

```json
{
  "provider": {
    "module_id": "builtin.typed",
    "module_version": "1.0.0",
    "capabilities": [
      "math.evaluate", "sql.query", "sorting.verify", "ordering.verify",
      "path.verify", "constraints.solve"
    ]
  }
}
```

This is only the optional binding fragment. A complete manifest still needs its
normal version, kind, workflow/checker, declared capabilities, dependencies and
provenance. The binding's capability set must exactly match the registered module
and be declared by the expert; it grants none of them.

`registry::providers::pin_provider` validates new active expert admission.
`pin_task_snapshot_provider` instead validates an already admitted task's immutable
route and module snapshots, preserving old versions across subsequent hot swaps.
Both require a provider snapshot audit before returning a usable pin. Session
still enforces the original task grant and budgets at dispatch, and compiled
checking remains independent of all module claims.

## Shutdown and continuation

Dropping a Broker signals shutdown so initialized queue workers release their
implementations. CLI shutdown drains cancelled process reapers before Tokio exits;
required audits and runtime leases remain owned until reaping is observed.

Ordinary checkpoint continuation refuses nonempty typed-module snapshots and
orphan module dispatch evidence. An embedding can explicitly supply retained
original runtime objects through `RuntimeRestoration`:

```rust,ignore
use cogfabric::recovery::continuation::{
    RuntimeRestoration, preview_with_runtime, resume_with_runtime,
};

let runtime = RuntimeRestoration {
    modules: Some(engine.modules.clone()),
    ..Default::default()
};
let preview = preview_with_runtime(&store, &task_id, runtime.clone())?;
// The operator reviews preview and explicitly approves this continuation.
let report = resume_with_runtime(
    store.clone(), &task_id, true, runtime, std::future::pending(),
).await?;
```

The supplied Broker must be the exact original instance and use the same Store
object. Each task pins its original retained module ID, version and capability
set, even after a newer active version is installed. Retired or unavailable
implementations are refused. Registry metadata never constructs callbacks.
Optional original escalation and verifier objects can be supplied in the same
context; their existing descriptor checks apply.

Preview is read-only and invokes no module. Admission revalidates under the
original task deadline and an exclusive execution lease. Before dispatch, the
lease becomes shared so same-image workers can obtain their own task leases.
Original model/tool/token charges remain consumed. Earlier module intents require
unique matching returned outcomes and original tool reservations; process audits
must link completed dispatches and completely reaped child ownership. Unknown,
failed, duplicated or inconsistent history is refused without replay.

`restored_typed_modules` in preview/continuation audits identifies the original
snapshot, its hash, the retained Broker and earlier completed dispatch count.
It grants no permission and attests no implementation binary. Arbitrary callback
reconstruction after process restart and native lifecycle-owner restoration remain
unsupported by this API.

## Approved builtin reconstruction after restart

New task snapshots use version 2 and record each pin's optional declared
`source_sha256`. The compiled builtin worker uses the runtime source fingerprint
(Rust sources, bindings, schemas, build support and locked dependency inputs).
Version-1 snapshots still support retained registries; they cannot reconstruct
workers without a recorded source identity.

For an unfinished non-native task using only `builtin.typed@1.0.0`, run:

```sh
cogfabric --home ./state resume-task TASK_UUID --human-approve --restore-builtin-worker
```

The flag permits reconstruction of the fixed compiled worker only. It accepts no
executable path or callback from the database. Exact current source fingerprint,
all six compiled capabilities, the original registry UUID, unique snapshot and
complete prior dispatch/charge/reaped-child evidence must match. A bounded
transaction under exclusive workspace ownership records reconstruction before
returning a registry. Audit failures roll back; no worker runs during this step.
Normal continuation independently checks the original checkpoint, deadline,
configuration, reservations and permissions before new work.

Library hosts can call `reconstruct_builtin_registry(store, task, true)` and pass
the returned Broker through `RuntimeRestoration.modules` to preview or resume.
Reconstruction itself writes an operator audit and does not admit continuation.
Ordinary preview remains read-only and does not reconstruct anything.

The restored UUID is explicit local provenance, not the original process object.
Source equality does not attest compiler flags, executable bytes, dependencies
beyond locked inputs, runtime environment, or provider weights. Same-owner
database/source tampering remains outside this guarantee. Source changes, custom
callbacks, mixed registries, legacy snapshots, unknown dispatches and native
lifecycle ownership remain refused; no replay, refund or capability is granted.

The explicit native continuation API separately restores native ownership and
may reconstruct the policy's original builtin worker under that same guard.
`resume-task --human-approve --native-policy POLICY.json` uses this path; the
standalone `--restore-builtin-worker` flag still cannot substitute native authority.
Source/history checks and original task grants remain required. See
[native continuation](NATIVE_LIFECYCLE.md#approved-native-continuation).
