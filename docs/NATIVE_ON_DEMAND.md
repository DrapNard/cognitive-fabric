# First-use native model activation

Architecture sections 4, 6, 7 and 9 require available capabilities to stay separate
from resident models. An explicitly approved native lifecycle policy with
`format_version: 3` opts into first-use activation. Versions 1 and 2 retain their
eager-loading behavior.

```json
{
  "format_version": 3,
  "api_base": "http://127.0.0.1:1234/api/v1",
  "authenticate": false,
  "models": [
    {"model": "your-default-model-key", "context_length": 32768},
    {"model": "your-critic-model-key", "context_length": 32768}
  ],
  "release_after": true,
  "typed_module_worker": false,
  "max_lifecycle_seconds": 300
}
```

Use installed model keys and approve the configuration explicitly:

```sh
cogfabric --native-lifecycle-policy ./native-policy.json --approve-native-lifecycle ask 'Your objective' --mode fast
```

All configured default/role targets and any selected expert model must be in the
approved set. The policy declares what may be loaded; it does not request that
every entry be loaded immediately. Initial inspection checks the approved scope
without loading weights. A task that reaches no inference call loads no model;
a role or expert model is activated only when its selected backend is invoked.
Shared model keys reuse one validated activation receipt.

## Identity and ownership

Before activation, a `managed_native` target records the canonical native endpoint,
model key and original policy digest. It contains no instance ID and does not
assert residency. After successful load or exact managed-receipt reuse, a separate
binding records the real instance. Fresh inventory and exact receipt checks still
precede native chat; every reply must match that instance. Unreceipted instances
are never adopted. Main-role and expert backends share one approval coordinator.

Model mutation and inference remain under the original workspace ownership and
budgets. A resource reservation is held across first-use loading and inference.
Working profiles and resident catalogs remain operator declarations; the lazy
path does not turn them into physical GPU measurements or dynamically reclaim
all declared resident allocations.

## Cancellation and continuation

An interrupted load remains an uncertain provider operation. It cannot be retried
or treated as a clean no-op. Failed or dropped generations retain their existing
unknown-outcome gates. Automatic cleanup may release only instances newly loaded
by this lifecycle, after safe confirmed work; reused operator instances are kept.

Restoration uses the exact original policy, activated bindings, model mappings,
resource declarations and original task clock. A persisted same-boot lifecycle
cutoff also prevents restart from granting another lifecycle allowance. Preview
performs no load or generation. Approved continuation restores activated receipts
and leaves never-used models unloaded until a later selected call needs them.
Unknown operations, changed receipts/policies and missing required evidence refuse
continuation. Restoration does not attest weights or prove provider termination.

## Limits

This is optional coordination for a trusted local provider. It supplies neither
physical GPU isolation nor independent model quality/safety evaluation. Concurrent
external provider clients can still interfere; fresh identity checks detect
observed disagreement rather than establish exclusive physical ownership.

The lifecycle journal and restoration readers are bounded; oversized or malformed
history refuses continuation rather than dropping records. Configured-role
activation must match the original role model; expert adapters retain an explicit
approved model binding. Unknown activation without a generation call also blocks
endpoint mutation until independent provider-stop acknowledgement and applicable
operation reconciliation. Local integration tests cover loading, reuse, cleanup,
continuation, cancellation and wrong-instance refusal; see the implementation
matrix for current results. The separate KV-cache placement fixture tests both
boolean settings and rejects mismatched load echoes or inventory without claiming
physical residency.
