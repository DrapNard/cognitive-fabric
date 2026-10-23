# Approved native model lifecycle

CLI/loopback lifecycle, uncertainty, cancellation and receipt reuse pass the
combined automated suite. See `CURRENT_IMPLEMENTATION.md` for exact evidence.
Real model quality and GPU behavior remain unverified.

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
