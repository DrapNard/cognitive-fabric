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
