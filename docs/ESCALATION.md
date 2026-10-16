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

Default continuation accepts matching built-in snapshots and refuses unavailable
custom, changed-version or spoofed snapshots before new dispatch. Explicit native
restoration and empirical data restoration are described below. Recovery never
silently swaps an operator policy for the built-in one. Legacy tasks without
snapshots retain only the runtime-owned built-in compatibility path. There is no
automatic policy catalog, executable loader or implementation attestation.

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

## Explicit policy restoration

Trusted hosts can call `recovery::continuation::preview_with_escalation_policy`
and `resume_with_escalation_policy`, passing the installed policy implementation
as an `Arc<dyn EscalationPolicy>`. Descriptors are captured outside Store locks;
every original snapshot must match. Preview never invokes a decision callback or
writes a task. Approved resume validates again under an exclusive execution lease
and uses the restored handle in its Session. No callback, grant or mutable budget
is reconstructed from the descriptor. Native code remains operator-trusted and
must use distinct descriptors for changed behavior.

For a cycle-positive checkpoint, the original `escalation_intent` and allowed
`escalation_decision` must also exist, occur before the saved boundary, and bind
the exact candidate, cycle, mode, independent criterion and required chain. Their
configuration, role-prompt and source fingerprints must match. Missing, duplicate,
denied or changed records refuse continuation before dispatch. A prior allowance
is not evaluated again, preventing a restored stateful policy from changing a
committed choice. Subsequent unresolved candidates still invoke that same policy
normally. This does not authorize reconciliation of missing results or create a
new allowance; existing supported phase, model/module, deadline and budget checks
still apply. Legacy records without snapshots are not eligible for a supplied
native policy, even one declaring the built-in ID/version.

The CLI can restore the compiled empirical policy from its original reviewed data:

```sh
cogfabric --home STATE resume-preview TASK_UUID \
  --retry-study reviewed-retries.json --retry-thresholds retry-thresholds.json
cogfabric --home STATE resume-task TASK_UUID --human-approve \
  --retry-study reviewed-retries.json --retry-thresholds retry-thresholds.json
```

Both options must be supplied together. Exact validated study content and
thresholds recreate the original descriptor; changed data cannot replace it.
File reads retain their caps, duplicate-key and no-follow validation. Resume
approval precedes input/configuration access. Continuation requires an existing
current-schema database and never initializes or migrates a missing/old workspace.
A database may open SQLite sidecar files and enforce its configured page limit;
this writer path is not read-only inspection. General automatic policy loading,
custom-policy crash reconciliation and cross-source migrations remain unsupported.
