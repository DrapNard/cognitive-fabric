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
orphan module dispatch evidence. It cannot silently resume with a fresh registry
or a different implementation. Explicit restoration of the original host registry
is still incomplete; operator-selected verifier restoration is a separate API.
