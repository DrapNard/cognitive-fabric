# Provider-reported inference metrics

Native role inference can carry optional version-1 timing observations through
`Generation` and the FlatBuffers module response. LM Studio native statistics
supply model load time, time to first token (converted from seconds to
milliseconds), and output tokens per second. llama.cpp native completion timings
supply prompt processing milliseconds, generation milliseconds, and output tokens
per second. Missing fields remain absent; they are never reported as zero.

Each supplied duration must be finite and within 0–86,400,000 milliseconds;
throughput must be finite and within 0–1,000,000,000 tokens per second. Empty,
unknown-version and malformed records are refused. The optional module field is
compatible with older responses without metrics. Bindings use flatc 25.12.19.

Successful role calls record a separate `inference_provider_metrics` task event,
linked to the call, role and answer hash. Existing reasoning recovery payloads
retain their schema. Metrics do not grant permissions, verify answers, approve
candidates, or change admission budgets. Local elapsed time remains separate;
provider durations may overlap and must not be summed as physical compute time.

These are provider reports, not independent measurements of GPU use, CPU time,
energy, model identity or reasoning quality. Text-only chat responses do not
expose these extra metrics. Real-backend timing accuracy remains unverified.

Provider references: [LM Studio native chat](https://lmstudio.ai/docs/developer/rest/chat)
and [llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).

## Verification scope

Focused checks cover numeric bounds, unknown versions/sources, optional typed
round trips, malformed metric frames, native field conversion, and hash-linked
trace records. The chat CLI regression separately exercises every `/effort`
setting without a live server. Workspace all-target/all-feature compilation,
strict Clippy, formatting and diff checks are used for this slice. The full
all-target test suite and live-provider timing measurements are not part of this
verification run; no release-readiness or reasoning-quality claim follows.

## Inspect per-call observations

```sh
cogfabric --json inference-audit TASK_UUID
```

The operator report adds `provider_metrics` to each charged call (null when no
record was published) and `provider_metrics_coverage` counts. Coverage is complete
only when every reserved call has a recorded result and validated metrics, with
no legacy uncorrelated results. Partial records retain their individual missing
fields. No latency or throughput totals are inferred from overlapping values.

Inspection uses an existing current-schema database without runtime bootstrap,
migrations or model requests. One read transaction and a shared five-second
cooperative inspection deadline cover the reservation ledger and metric history.
At most 64 metric records are accepted. Each must follow exactly one recorded
reasoning result and match its call, role and answer hash; duplicate, orphan,
malformed or authority-claiming records are refused. A failed inspection does not
rewrite events or grant continuation/replay authority. Recovery admission uses
its existing ledger validator independently of these optional observations.

The report contains model IDs, counters, timings and content hashes, without the
original goal, prompts or answer bodies. Local hashes and audit records do not
provide authenticity against a hostile same-owner database edit.


Inspection verification passed: 14 `recovery::inference` unit regressions and two
`inference_audit_cli` integration checks (all features). Cases include complete,
missing and legacy coverage, malformed/duplicate/orphan/early/hash-substituted
records, bounded histories/payloads, unchanged recovery authority and CLI refusal
without state creation. The CLI fixture supplies an absent verifier module path,
proving the inspection command does not load unrelated executable options.
