# Versioned executive escalation

`Engine::with_escalation_policy(Arc<dyn escalation::EscalationPolicy>)` installs
an operator-trusted policy on that Engine value. Existing clones retain their
handle and descriptor. IDs and canonical semantic versions are bounded to 64
bytes. The descriptor identifies an operator declaration, not attested native
implementation or trained Jev weights.

The deterministic runtime first checks remaining calls, output reservations,
prompt bytes, cycle capacity and the original deadline. An independently rejected
answer also requires a future verification-tool allowance. Only then does the
policy receive a typed `Request`: reason, mode, cycle, candidate hash, remaining
resources and required allocations. It receives no candidate text, references,
credentials, permissions or mutable budgets. `Decision` contains only `allow` and
a bounded identifier-like `reason_code`. The default `BudgetPolicy` allows work
that fits these gates; it does not claim measured expected utility.

Policies may decline a revised solver after an independent rejection, or decline
the critic/reflection/revised-solver sequence for an unverified answer. Independent
success and repeated-answer stopping happen before policy invocation. Declining
leaves existing verification evidence unchanged and stops with
`escalation_declined`. No extra call or cycle is reserved. Calls that cannot fit
stop with `insufficient_revision_budget`, without invoking a custom policy.

Example trusted host policy:

```rust
use anyhow::Result;
use cogfabric::escalation::{Decision, Descriptor, EscalationPolicy, Request};
use std::sync::Arc;

struct StopRevision;
impl EscalationPolicy for StopRevision {
    fn descriptor(&self) -> Descriptor {
        Descriptor { id: "operator.stop_revision".into(), version: "1.0.0".into() }
    }
    fn decide(&self, _: &Request) -> Result<Decision> {
        Ok(Decision { allow: false, reason_code: "operator_review".into() })
    }
}
// let engine = engine.with_escalation_policy(Arc::new(StopRevision))?;
```

No CLI metadata, model output or retrieved document can install a native policy.
Callbacks must be short and nonblocking. They run outside Store locks, but native
code can block, panic or use host privileges: this interface is not a sandbox or
preemptible execution boundary. Resource and permission checks remain outside
policy intelligence and are checked again at actual dispatch.

The reasoning task snapshots the descriptor before its whole-goal reasoning
calls. `escalation_intent` persists before the callback, and
`escalation_decision` persists before any additional model dispatch. Both are
metadata-only; no prompts or answers are added. Intent/result write failure,
callback error or invalid reason code fails the task without a silent fallback.
The intent does not prove that a callback completed. Decision events do not grant
permissions or attest quality. At most one decision occurs per unresolved cycle.

Custom policies apply to Full, Collective, Memory and Experts execution profiles.
Alone, Tools and Recursive comparison profiles retain the built-in policy, so
installing a collective policy does not alter those baseline arms. This does not
establish fair model/quality comparisons by itself; record configuration and source.

Continuation accepts matching built-in snapshots and refuses unavailable custom,
changed-version or spoofed snapshots before new dispatch. It never silently swaps
an operator policy for the built-in one. Legacy tasks without snapshots predate
this interface and use the built-in compatibility path under current admission
checks. There is no custom-policy reload/attestation or automatic code loader.
Starting a new reviewed task is required when its policy cannot be restored.

Tests in `src/escalation.rs` cover audited allow/deny, budget-before-callback,
remaining checker budget, independent success, baseline isolation, descriptor and
reason bounds, Store reentry, immutable Engine handles and audit failures.
`src/recovery/continuation.rs` covers unavailable/changed/spoofed policy snapshots.
This implements replaceable escalation decisions, not calibrated gain estimates,
global resource scheduling or independent reasoning-quality evaluation.

## Optional empirical retry evaluation

See [empirical escalation](EMPIRICAL_ESCALATION.md) for reviewed-study aggregation,
source/configuration/checker scope, immutable cost thresholds and operator-only
inspection. It uses this interface without enlarging runtime authority.
