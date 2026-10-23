# Approved native model lifecycle

CLI/loopback lifecycle, uncertainty, cancellation and receipt reuse pass the
combined automated suite. See `CURRENT_IMPLEMENTATION.md` for exact evidence.
Real model quality and GPU behavior remain unverified.

## Read-only restoration preflight

```sh
cogfabric --home ./state native-resume-preview TASK_UUID --policy ./native-policy.json
```

This command inspects original receipt identity for an unfinished native child
task using a read-only database. The original policy, unique scope/bindings,
configured role models, completed generation admission and unexpired same-boot
clock must match. It holds exclusive local runtime/resource leases, checks the
pending-generation gate and performs a bounded fresh inventory GET. Every original
receipt must still validate against that complete inventory; state is rechecked
after the observation. It does not adopt or rebase foreign/drifted instances.

`receipt_identity_matches: true` establishes only this observed local consistency.
The report explicitly retains `checkpoint_validated: false` and
`resume_authorized: false`. No task audit/status, model load/unload, generation,
stop acknowledgement, budget or permission is changed. Ordinary continuation
without explicit native policy restoration still refuses.
Provider identity reuse, same-owner tampering, real weights and physical provider
termination remain outside this preflight's proof.

Library callers use
`residency::lifecycle::restoration::inspect(store, task, &policy).await`.

## Approved native continuation

```sh
cogfabric --home ./state resume-task TASK_UUID --human-approve --native-policy ./native-policy.json
```

The original policy must match the recorded digest. Setup retains the validated
exclusive runtime/resource owner through normal checkpoint admission and execution.
Exact native role targets and original receipts are restored. Original deadline,
charged calls/tokens/cycles/tools, route maps and resource configuration remain
authoritative. The original builtin worker is reconstructed only with matching
version-2 source/history evidence and borrows this same owner. Custom callbacks
still require retained host objects through `RuntimeRestoration`.

Each new model dispatch creates a separate lifecycle task with recorded original
receipt bindings, role/prompt hash/output ceiling and a durable generation start
before receipt revalidation and native chat. Confirmed successful replies close
that generation; failed/dropped/uncertain calls retain a blocking start. Existing
operator generation resolution accepts these lifecycle task IDs after independent
provider stop and explicit acknowledgement. Resolution grants no replay or refund:
unresolved original inference/resource evidence still blocks retries. No model
load, release, receipt rebasing or automatic cleanup occurs on this path.

Library hosts call `recovery::continuation::resume_native_with_runtime` with the
original policy, explicit approval and retained verifier/escalation/module objects
when required. Cancellation covers setup and execution. Inference process
substitution and arbitrary callback reconstruction remain refused. Logical
resource admission and returned-instance checks do not prove physical GPU limits,
continuous exclusive provider ownership, weight identity or general answer quality.

## Operator policy

Ordinary collective tasks can use an explicitly approved, immutable local policy:

```json
{
  "format_version": 1,
  "api_base": "http://127.0.0.1:1234/api/v1",
  "authenticate": false,
  "models": [{"model": "your-installed-model-key", "context_length": 4096}],
  "release_after": true,
  "typed_module_worker": true,
  "max_lifecycle_seconds": 120
}
```

Each configured role must map to an approved model key. The policy contains no
user goal or approval assertion. Files must be regular, non-symlink files under
64 KiB. The default is disabled. Explicit command-line approval is required:

```sh
cogfabric --native-lifecycle-policy policy.json --approve-native-lifecycle ask 'calc: 7 * 6'
cogfabric --native-lifecycle-policy policy.json --approve-native-lifecycle serve
```

The HTTP service uses the same policy options for collective Engine requests.
Its ordinary language-agent endpoint retains its separately configured backend.
Requests cannot replace the service's reviewed lifecycle policy or grant approval.
The server's loopback and authentication requirements still apply.

## Execution and uncertainty

Rust validates configured models before provider effects, holds workspace
exclusion leases, loads only approved keys and records managed receipts. Existing
valid receipts may be reused. Before generation, fresh inventory must match the
exact recorded instance and cohort. Native chat requests use the model key; the
returned instance identifier must match. This is not request-time instance
selection or proof of provider ownership.

The original task UUID, options, resource budgets and absolute deadline survive
lifecycle delegation. Loading does not reset the task clock. Cancellation and
uncertain dispatch defer cleanup. Only instances newly loaded by this lifecycle
may be released automatically; reused receipts remain resident.

Generation intent is durable before dispatch. An unknown result blocks subsequent
managed mutation; it is never retried automatically. Operator reconciliation
requires explicit provider-stop acknowledgement and fresh inventory. It records
uncertainty without converting the original outcome to success. See
[managed residency](MANAGED_RESIDENCY.md) for resolution and archive operations.

Authentication, when enabled, uses the dedicated
`COGFABRIC_RESIDENCY_API_KEY`. Redirects and proxy discovery are disabled. Native
lifecycle adapters remain replaceable inference integrations; they are not newly
trained Jev+ or Jev-R networks.
