# Reviewed residency plans

Architecture sections 4, 6, 9 and 12 require Jev+ to reuse loaded models and activate
only needed resources. `residency::planning` adds a deterministic, bounded native
LM Studio reconciliation planner. The operator selects desired installed model
keys/context lengths and any exact instance IDs permitted to be released. Models
receive no new residency capability. This implements reviewed lifecycle planning,
not autonomous ownership, GPU enforcement or demonstrated reasoning improvement.

## Operator workflow

Inspect the native inventory, then write a request using its installed keys:

```json
{
  "ensure_loaded": [
    {"model": "installed-model-key", "context_length": 4096}
  ],
  "release_instances": []
}
```

```bash
cogfabric residency-status --api-base http://127.0.0.1:1234/api/v1
cogfabric --json residency-plan request.json \
  --api-base http://127.0.0.1:1234/api/v1 > residency-plan.json
# Review the exact actions and reused instance IDs in residency-plan.json.
# Apply uses an existing workspace; initialize one explicitly if needed.
cogfabric init
cogfabric --json residency-apply residency-plan.json \
  --api-base http://127.0.0.1:1234/api/v1 --human-approve
```

Preview only reads provider inventory and does not load models or initialize local
state. Apply requires an existing current-schema database and explicit approval
before reading the plan/configuration. It does not bootstrap or migrate a missing
workspace. The native endpoint is supplied explicitly again and must match the
plan. Optional `--authenticate` uses only `COGFABRIC_RESIDENCY_API_KEY`, through
the existing bounded native credential adapter; inference/server credentials are
not inherited. No new native HTTP path or JSON response contract is introduced.

Stop active workspace hosts before application. Both a runtime lease and the
configured-resource ownership lease are held throughout the batch. Even an idle
configured pool prevents model mutation. This guard also applies to the existing
single-model `residency-load` / `residency-unload` commands. Read-only inspection
remains available.

## Selection and ordering

The planner supports 1–8 distinct desired installed model keys, at most eight
explicit release IDs, and at most eight external operations. Desired contexts use
the existing 1–32,768 ceiling. Only exact loaded context matches are reused; when
several match, the lexicographically smallest non-released instance ID is chosen.
Input model/instance order does not alter the proposed operation order.

A missing installed key or unknown release ID is rejected. A different loaded
context is not silently replaced: all incompatible instances must be explicitly
selected for release first. The planner never invents an unload of another model
or assumes that an unused instance belongs to this framework. Explicit unloads
are sorted and performed before sorted missing loads. Desired models already
loaded compatibly cause no load. A fully reusable request can therefore have zero
external operations.

The model-set choice is operator policy, not a learned utility prediction. These
rules minimize avoidable reloads under that request; they do not claim a globally
optimal model portfolio or calibrated gains from additional models.

## Freshness, receipts and bounded execution

A plan contains the canonical request/actions, reused instances, native endpoint,
creation time and inventory digest, plus a digest binding its content. Plans expire
after 300 wall-clock seconds; future timestamps and changed content are rejected.
The inventory projection includes installed model key/type and loaded instance
ID/context length, sorted deterministically. Unknown provider metadata and actual
weight bytes are not fingerprinted. Hashes detect local data drift, not forgery by
an owner who can replace both data and hash.

Apply fetches inventory again and rebuilds the proposed plan before claiming it.
An immediate transaction persists a one-attempt receipt before any mutation. The
same digest cannot be applied again, even if the provider externally returns to
its original state. Malformed/ambiguous receipts fail closed. Claims use a
five-second SQL progress deadline and inspect at most 1,024 receipt records of at
most 32,768 bytes each. The normal database quotas remain in force.

Every step independently checks the compiled operator permission. It fetches
inventory before POST and requires the digest expected from the previous confirmed
step. After POST, fresh inventory must show the desired exact instance and only
the projected change implied by that operation. An unrelated model/context/instance
change during a step, or drift between steps, interrupts the remaining plan.
Checks cannot eliminate races with other applications that ignore these workspace
leases or mutate provider state outside the observed projection.

A batch has a 120-second mutation-phase deadline. Existing native request limits
remain: bounded responses, three-second inventory requests and up to 60 seconds
per mutation POST. Any timeout, failed operation or required audit interrupts later
steps. No automatic retry, rollback or unload of a possibly shared model follows.
Earlier effects cannot be undone by a failed audit.

## Audits and interruption

The parent task records `residency_plan` request/outcome receipts. Before each
possible POST, `residency_plan_step` binds its child operation task; that child
retains the existing `model_residency` requested/confirmation/unknown-outcome audit.
A batch report lists acknowledged confirmed steps, reused IDs and completed or
interrupted state. CLI application returns a nonzero exit for interruption while
printing the structured report. Provider error bodies and credentials are excluded.

If a future/host disappears, a receipt or operation can remain requested/running;
that is not success or proof that the provider stopped. Inspect the child task and
use `residency-inspect` for a sampled observation. Such inspection does not resolve
history or authorize automatic replay. A new plan needs a fresh operator review
and approval. Normal retention preserves plan receipts and their linked operation
tasks so deletion cannot reset the one-attempt gate. Receipt retirement/compaction
is not yet implemented; the 1,024-record receipt quota is an explicit current
operational limit (normally two records per finished plan).

## Evidence and remaining work

`src/residency/planning/tests.rs` covers reuse, ordering, explicit context
replacement, invalid/missing/duplicate targets, expiry, changed plans and corrupt
or replayed receipts. `tests/residency_plan.rs` uses real loopback HTTP and CLI
processes to cover load/unload/reuse, no-state preview, approval precedence,
no-operation application, stale and between-step inventory changes, unknown
outcomes, claim-audit rollback, replay after external reset, idle resource
ownership and retention protection. Provider responses are fixtures.

Reports preserve `ownership_established:false`, `weights_attested:false`,
`generation_tested:false` and `physical_limits_enforced:false`. Model inference,
real load performance, VRAM, energy and neural safety are not evaluated here.
General provider ownership, model-to-task lifecycle automation, receipt retirement,
independent utility evidence and physical resource control remain incomplete.
Native Rust callers are trusted operator embeddings; this is not remote user
identity authentication or an executable sandbox.
