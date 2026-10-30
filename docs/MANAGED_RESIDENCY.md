# Managed native residency receipts

Architecture sections 4, 6, 9 and 10 require model reuse and controlled lifecycle
changes. Managed residency adds local causal receipts for explicitly approved
native LM Studio load requests. It is a prerequisite for lifecycle integration;
it does not automatically activate models for reasoning tasks.

## Operator workflow

Initialize or explicitly migrate the workspace first. Managed commands require an
existing current-schema database and do not bootstrap missing state. Inspect the
provider's installed model keys with the existing `residency-status` command.

```bash
cogfabric init
cogfabric --json residency-managed-load --model installed-model-key \
  --context-length 4096 --api-base http://127.0.0.1:1234/api/v1 --human-approve
cogfabric --json residency-managed-status \
  --api-base http://127.0.0.1:1234/api/v1
cogfabric --json residency-managed-release --receipt-id RECEIPT_UUID \
  --api-base http://127.0.0.1:1234/api/v1 --human-approve
```

The release argument is the framework receipt UUID returned by managed load,
not a provider instance ID. Both mutation commands require explicit approval
before configuration/database initialization. The native endpoint is explicit on
each invocation and must match the receipt. Optional `--authenticate` uses only
`COGFABRIC_RESIDENCY_API_KEY`, following the existing dedicated credential adapter.
Local managed status is read-only and never contacts the provider or adopts an
observed instance. It reports local receipt state and whether managed work is
blocked; it does not certify that an instance is still loaded.

## Authority and transitions

Only the existing compiled human operator permission permits mutations. Jev+,
Jev-R, model output, retrieved material and plugin metadata receive no residency
permission. Both runtime and configured-resource ownership leases must be free;
even an idle configured pool prevents changes. These leases coordinate this
workspace and cannot prevent another application from changing LM Studio.

A managed load accepts an installed model with no loaded instances. Task and
exact intent commit before POST. Provider response and fresh inventory must show
only the exact requested transition. Receipt insertion, confirmation audit, task
completion and active cohort digest changes commit together. Existing/manual
instances and old operations never acquire receipts during migration or inspection.

A managed release requires an active receipt, matching endpoint/model/type/context/
instance identity and the exact full-inventory digest. Receipt transition to
`release_requested`, the new task and intent commit before POST. Confirmed release
consumes the receipt as `released`, advances the other active endpoint receipts to
the new observed digest and completes its audit/task atomically. Known managed
loads likewise update all previously active receipts after an exact transition,
so several approved models can coexist without making each other's baselines stale.

Unknown outcomes, cancellation, dropped futures, changed inventory or failed
required audits never imply success or authorize replay. Pending load history can
block the endpoint even when no receipt/instance could be established. An uncertain
release retains a requested or quarantined receipt and cannot be retried through
the managed API. A matching later inventory observation does not resolve history.
Resolution requires the explicit schema-8 workflow below. A later inventory
observation alone cannot clear the gate.

Existing manual load/unload and reviewed residency plans preserve explicit operator
administration of exact selected instances. Before their provider mutation, active
managed receipts at that endpoint are durably quarantined. Failure to commit that
invalidation prevents POST. A manual action never silently refreshes managed
release authority. Read-only inspection leaves receipts unchanged.

## Persistence and bounds

Schema 7 adds bounded receipt rows with retained load/release task references,
immutable receipt identity/details and a checksum. Active or uncertain identities
cannot be duplicated. A confirmed released provider ID may be returned by a later
explicitly approved fresh load, but it receives a new receipt UUID and cannot
reactivate the old receipt. Normal retention preserves receipt-linked tasks and
managed operation history, including unknown loads without a receipt.

The current hot-ledger quota is 1,024 receipt rows; terminal rows remain tombstones
until explicitly archived using the schema-8 command below.
Loads refuse at capacity before an external effect; guarded releases still work.
There is no automatic compaction or deletion to reset the gate. Bounded history
reads and database quotas can independently refuse further work. These operational
limits are not a claim of unlimited unattended service.

## What receipts do not prove

Reports retain `ownership_established:false`, `weights_attested:false` and
`generation_tested:false`. Inventory is provider-reported. GET/POST/GET sampling
cannot eliminate races, an external restart or reuse of the same instance ID for
the same model/context. Local checksums detect inconsistency, not an owner who can
rewrite the database and its evidence. No physical GPU/RSS or energy limit is
established. The inference adapter currently selects endpoint/model, not an
attested native instance; a receipt does not prove a reasoning task used it.

Approved Engine task-driven load/reuse/release is available through
[native lifecycle policies](NATIVE_LIFECYCLE.md), with version 3 supporting
[first-use activation](NATIVE_ON_DEMAND.md). General provider support, continuous
physical ownership and independent utility/physical measurements remain incomplete. Explicit unknown-outcome
resolution and terminal receipt archiving are described below.

Each managed inference also writes a `native_prediction_admission` event to the
task history before provider dispatch. It reports bounded activation, provider
slot and lifecycle-mutation wait durations separately, along with the observed
slot count. The task's existing `queue_wait_ms` remains the aggregate scheduler
and provider queue measurement. These timings help identify where CGF waited;
they do not establish provider execution concurrency or GPU utilization.

## Per-model prediction slots

On first-use activation, CGF reads the instance's reported `config.parallel`
value. Native generations for that model share a bounded gate: CGF allows at most
the reported number of predictions, capped at four; if the provider omits the
value, CGF assumes one. Different configured model keys have separate gates and
can overlap within the task's shared inference capacity. Time waiting for this
gate is included in queue-wait metrics.

The LM Studio v1 load endpoint does not expose a max-concurrent-predictions
request field. Configure that per-model setting in the LM Studio model loader;
LM Studio documents continuous batching for llama.cpp models. The provider's
reported slot count is only a scheduling hint, not proof that it will execute
requests concurrently or that hardware has free capacity. See the [LM Studio
load API](https://lmstudio.ai/docs/developer/rest/load) and [parallel
requests](https://lmstudio.ai/docs/app/advanced/parallel-requests).

## Prior schema-7 verification

Verification on final frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**940 passed across 61 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Eleven CLI/loopback integration tests cover multi-model cohorts, provider-ID reuse
with fresh UUIDs, non-adoption, endpoint/approval/workspace gates, manual-plan
invalidation, before/after-POST audit failures, unknown outcomes, actual client
future cancellation during pending POSTs, resource leases, retained unknown loads
and capacity refusal with releases still allowed. Storage/helper regressions cover
schema 1–6 migration without adoption, atomic failure/data preservation, receipt
integrity, retention, malformed markers, indexed queries and SQL deadline cleanup.

The first full runs found historical migration fixtures requiring v7-object
removal and a general event index causing worker-recovery timeouts. The index is
now partial to native residency events; the unchanged 10,000-session worker gate
passes again. Test compile/lint fixtures were corrected without removing tests.
Malformed managed markers cannot downgrade receipt validation or erase retention
protection. Existing socket/process, mock, malformed-frame, policy-denial,
cancellation and candidate-rollback tests remain green. Canonical French
architecture and protocol schemas/bindings are unchanged. Bundled FlatBuffers
pairing/integrity checks pass; `flatc` is unavailable, so fresh generation was not
run. Port 1234 refused local-model discovery: no real provider load/unload,
inference quality, physical memory or efficiency result is claimed.

Jev+ and Jev-R remain **Partial**. Following the explicit resolution and archive
workflow below, the next lifecycle milestone is task-to-native-instance integration. No model gains
mutation permission and local receipts still do not prove exclusive ownership.

## Operator resolution and evidence archiving (schema 8)

The previous unresolved-operation limitation is addressed by two explicit operator
commands. Task-driven native-instance integration remains partial.

```sh
cogfabric residency-managed-resolve --api-base http://127.0.0.1:1234/api/v1 \
  --task-id ORIGINAL_TASK_UUID --human-approve --provider-stop-acknowledged
cogfabric residency-managed-archive --receipt-id RECEIPT_UUID --human-approve
cogfabric residency-managed-inspect --receipt-id RECEIPT_UUID
```

Resolution requires an existing current database, the original unknown managed
operation, its exact endpoint, both runtime/resource leases, operator approval and
an explicit acknowledgement that the provider operation has stopped. The
acknowledgement is testimony, not measured termination. A fresh inventory GET must
show no loaded instance of the original load key, or absence of the original
release instance. Resolution never POSTs, retries, adopts an instance or marks the
original provider outcome successful. Its original events remain unchanged; a
locally running task becomes interrupted to release the local active-task slot.

A stable active receipt retains its existing digest. Stale endpoint receipts block
resolution unless `--retire-stale-cohort` explicitly abandons their release
authority. This does not unload their instances or refresh their inventory digest.
An uncertain release's receipt is retired even without that option. Other unknown
operations still require their own resolution. Immutable resolution payloads,
member snapshots and audit pairs must validate before the pending gate clears.

Read-only inspection accepts hot or archived receipt UUIDs and validates archive
evidence; it never grants release authority.

Archiving accepts only released receipts or retired receipts with valid resolution
evidence. It contacts no provider. An immutable snapshot, retained task references
and matching audit pair replace the hot row in one transaction. Archived evidence
can validate history but cannot authorize a release. Hot receipts are capped at
1,024, archives at 8,192 and resolutions at 1,024; exhaustion is explicit. History
validation retains at most 36,864 events and 64 MB of projected payload under a
five-second deadline. These are logical bounds, not physical resource guarantees.

Migration preserves schema-7 receipt rows without adopting provider inventory.
No model receives a new permission. Owner-local hashes and audit consistency do
not authenticate evidence against an owner who edits the database or source.


## Empty provider after restart: retire stale release authority

A confirmed load receipt can outlive its provider instance after LM Studio exits.
Its old inventory digest must not be silently reused or rebased. With independent
confirmation that provider work stopped, an operator can explicitly discard that
receipt's release authority:

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" residency-managed-abandon \
  --api-base http://127.0.0.1:1234/api/v1 --receipt-id RECEIPT_UUID \
  --human-approve --provider-stop-acknowledged
```

This narrowly requires a fresh native inventory with **no loaded instances at all**,
an active confirmed receipt at the exact endpoint, no pending unknown operation,
and exclusive local runtime/resource leases. It never unloads or adopts a model,
replays an inference, fabricates a provider failure, or changes the original load's
confirmed outcome. Stop acknowledgement is operator testimony, not proof that the
provider has terminated all work. Other applications can race the observation.

The atomic transaction marks the receipt retired and appends two bounded,
hash-bound events to its retention-protected original load task. Original receipt,
load history and inventory digest remain intact. Audit failure rolls back retirement.
History validation, inspection and archival validate that pair; corruption blocks
further managed work. Existing archive facilities retain the evidence. A later
load requires a fresh receipt. No schema migration or new model capability is
introduced; older binaries cannot validate the new proof and fail closed. The
launcher never calls abandonment automatically.
