# Task-pinned alternate typed expert providers

The trusted host may register several active module identities that implement the
same typed capability. Registration still enforces immutable ID/version pairs,
32 active identities, 128 retained versions, validated capability declarations and
bounded worker limits. Only one version of an identity is active for future tasks.

## Task admission and dispatch

New engine tasks record a version-5 `typed_module_snapshot`: all admitted module
IDs, versions, declared capabilities, implementation/source identities and queue
limits in deterministic identity order. Each capability receives its original
ordered candidate set, bounded by the active registry limit. Later activation or
registration cannot introduce a new implementation into an existing task.

Before dispatch, the broker reserves a free bounded queue slot on the first
available task-pinned provider. Closed or full candidates may be skipped at this
stage. A reservation executes no module code and records no execution intent.
The existing actor/task policy and narrow grant still authorize the request;
the exact selected implementation is then recorded in the dispatch intent.
A failed authorization drops the reservation without executing the module.

Once an intent is published or execution begins, errors, timeouts, cancellation,
invalid replies and unknown effects do not trigger an alternate-provider retry.
The original task deadline and tool/model budgets remain in force. If no provider
has capacity, alternate-route admission refuses rather than creating an unbounded
waiting queue. Direct exact-pin dispatch preserves its existing queue wait under
the original deadline.
Manifest workflows that explicitly name a module still require that exact module.

## Continuation and limits

Recovery compares the original candidate identities, declared limits and chosen
module dispatch history. Versions 1–4 retain their original single-provider
semantics; overlapping providers are not retroactively accepted in old snapshots.
Version 5 does not authorize new modules, reset charges or resolve uncertain work.
The existing original-broker and explicit reconstruction restrictions remain.

This is deterministic availability-based selection among trusted implementations.
It is not measured quality ranking, independent executable isolation, provider
attestation, automatic model loading or proof of improved reasoning performance.


## Verification

Fifty-one focused checks passed with all features enabled:

```sh
cargo +1.99.0 test --lib --all-features fabric::modules --locked
cargo +1.99.0 test --lib --all-features recovery::continuation::module --locked
cargo +1.99.0 test --lib --all-features registry::provider_tests --locked
cargo +1.99.0 test --all-features --test typed_module_reasoning --test typed_modules --test wasm_experts --locked
```

The integration targets were run together with `inference_audit_cli` (two additional
checks for that separate feature). Default/all-feature all-target checks, strict
all-feature Clippy, formatting and diff checks passed. No full reflection suite,
real-model quality campaign or performance benchmark was run. Candidate selection
adds up to 32 bounded queue probes and no additional model inference.
