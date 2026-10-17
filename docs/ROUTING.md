# Replaceable Jev+ expert routing

`routing::ExpertRouter` is a trusted Rust extension point for executive expert
selection. The default `TagRouter` preserves the existing deterministic tag-score
and ID tie-break behavior. It calls no model, starts no process and selects at
most two skills. Jev-R reasoning and the independent security kernel remain
separate; selection never grants tool permissions or expands a task's budgets.

## Trusted host API

Implement `descriptor()` with a bounded logical ID and canonical semantic
version, and `select(Request)` returning ordered skill IDs. Install it with:

```rust
let next = engine.clone().with_expert_router(policy)?;
```

Here `policy` is an `Arc<dyn routing::ExpertRouter>` supplied by trusted host code.
`Engine::expert_router_descriptor()` exposes its cached declaration. New Engine
values use the new policy; old clones retain their original handle. The descriptor
is captured and validated at installation. Hosts must use a new version for changed
behavior; this is declared metadata, not code attestation or a global immutable
policy registry. There is no policy CLI/socket loader, executable provisioning,
sandbox, automatic promotion or serialized callback restoration.

`Request` contains the original objective, pinned skill metadata and the accepted
route ceiling. The wrapper rejects empty/oversized objectives or more than 4,096
skill records before invoking the callback. Returned selections must contain at
most two distinct IDs enabled in that exact snapshot. Unknown, disabled, duplicate
or excessive selections fail before expert/inference dispatch. An empty selection
is valid. All existing expert admission, model availability, call/token/time,
checker matching and per-tool actor authorization still apply afterward.

## Snapshot and audit boundaries

The task obtains one consistent SQLite context snapshot before routing. Callbacks
run after its transaction and database mutex are released. Returned IDs are
checked against the copied snapshot, not a new live lookup. A later skill disable
or replacement affects new tasks; an admitted route retains its original metadata.
Trusted callbacks can re-enter operator APIs, but those APIs retain their own
rules and do not become permissions available to a model.

A `routing_policy` audit precedes callback invocation; audit failure prevents the
callback. The existing `route` event retains selected IDs and pinned versions,
adding the cached router declaration and `router_invoked`. Full, Collective and
Experts profiles invoke the policy. Alone, Tools, Recursive and Memory comparison
profiles preserve their exclusions. Basic and comparison reports disclose the
configured router with `router_implementation_attested:false`; task audits show
whether it actually ran. No model/cost advantage is inferred from a descriptor.

## Native extension limits

Callbacks must be short, nonblocking metadata decisions. They must not perform
unaccounted tool/model calls; model-assisted routing needs separate integration
through budgeted runtime services. Arbitrary trusted Rust can ignore this contract,
block, panic or allocate memory before returning. Selection validation is not a
sandbox or a hard execution bound on native code. Untrusted policy implementations
require a separately designed isolation/provisioning boundary. Default routing
remains the bounded deterministic heuristic; no new trained Jev model is claimed.

This implements one executive replacement point. General module provisioning,
persisted policy catalogs/rollback, independent scheduler replacement and
calibrated expected-utility routing remain incomplete. Saved continuation still
uses its already-validated context/decisions under the existing recovery gates;
this API does not reconstruct or rerun arbitrary callback state.

The 4,096-record input ceiling now shares `store::MAX_SKILLS` with persistent
legacy/manifest admission. Storage reads cap rows and field bytes before copying
metadata into routing snapshots; malformed tags are rejected rather than treated
as empty. See EXPERTS.md for retained-ID and legacy compatibility rules.

Optional interpreted capability requirements now use
[joint pinned-provider coverage](CAPABILITIES.md) after initial routing. Existing
route order is preserved; greedy additions reuse providers and retain the four-route
refinement ceiling. Required metadata never grants execution or measured usefulness.
