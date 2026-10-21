# Recent optional-advice cost guard

Architecture sections 4 and 9 require Jev+ to avoid unnecessary inference cost.
The public portfolio diagnostics showed repeated planning/exploration timeouts.
The optional `advice_guard` uses recent recorded phase failures and elapsed costs
to decline those stages while retaining the original whole-goal solver path.
It is a conservative operational heuristic, not learned intelligence, calibrated
expected utility or evidence of a quality/latency improvement on real models.

## Owner configuration

Add this object to the owner-controlled `config.json`:

```json
{
  "advice_guard": {
    "min_observations": 3,
    "sample_size": 8,
    "window_seconds": 300,
    "trip_failure_basis_points": 7500
  }
}
```

These are example settings, not tuned recommendations. Omission/null disables
the guard and preserves legacy serialized configuration. Configuration requires
1–16 minimum observations, minimum–16 samples, 1–3,600 seconds and a trip threshold
of 1–10,000 basis points. Unknown fields and invalid values are rejected. Enabling
or changing settings enters configuration snapshots/fingerprints and starts a new
scope. No new model, weight change, tool grant or provider mutation is introduced.
CLI/service/HTTP collective tasks and direct Session construction share the path.
Native Rust embeddings remain trusted code, not sandboxed policy plugins.

## Scope and bounded history

A scope hash binds effective configuration, compiled role prompts, runtime-source
bytes, canonical module ID/version/role metadata, reasoning mode, executable
criterion name and 1-KiB initial-prompt-size bucket. Engine execution profile is
also bound, so failures in Full, Collective, Memory or Experts cannot silently
suppress another profile. Direct Session construction has a distinct profile.
The bucket does not establish semantic similarity or exact input-token cost.
Declared model/module versions do not attest actual provider weights or hardware.
Canonical module scope is capped at 64 descriptors and 16 KiB before hashing.

Before a resource-admitted fresh planning/exploration stage, the reader examines
only the newest 512 database events, then filters guard observations. It does not
scan unbounded history looking for enough matches. Payloads are bounded to 4 KiB;
source projection copies a fixed expected label rather than arbitrary text.
Strict parsing/shape validation is required. Malformed/oversized observations or
an invalid source inside the inspected tail fail the task before optional dispatch.
Other valid scopes/phases are ignored. No prompt, answer, strategy, document,
credential or provider-error body is read by this history query.

Only same-boot Linux BOOTTIME observations inside the configured window apply.
Other boots, expired samples and samples later than the captured read time are
ignored; the latter can arise from a concurrent writer. If the clock is unavailable,
the enabled guard skips optional work rather than pretending it has fresh history.
Civil timestamps cannot extend a task budget. Fresh phase observations use the
existing kernel clock helper; supported recovery retains its original clock rules.

The newest matching samples, up to `sample_size`, must have an original phase
budget at least as large in ceiling-rounded one-second buckets. A failure under
a shorter allowance does not prove failure under a newly larger allowance.
A recorded timeout/error counts as failure; completed work also counts as cost
failure if its measured elapsed time exceeds the current phase budget. Otherwise
`completed` means the phase future finished, not that its advice was valid, useful
or independently verified. Invalid but quickly returned plans can be completed.

Once the minimum sample count exists, Jev+ skips the phase when the failure fraction
is **at least** the configured threshold. Integer arithmetic avoids rounded-rate
admission. Insufficient history allows ordinary bounded probing. After expiration,
reboot, configuration/source/profile changes or tail displacement, probing may
resume. This is not a persistent security quota or a guarantee of global fairness.

## Audits and original limits

`advice_guard_decision` records scope, phase, observed counts/cost, allowance and
reason before the original scheduling/phase path. It explicitly denies calibrated
utility, permission grants and provider-stop proof. Audit failure prevents further
inference. Allowed stages get a ticket with their original phase ceiling; time
spent on scope/history/audits is not reset into a new allowance. An already expired
ticket starts no inference. The original task deadline remains authoritative.

After an actually charged phase attempt, `advice_phase_observation` records its
same-boot completion time, original ceiling, elapsed cost and completed/timed-out/
failed outcome. Recording failure fails the task before later solving. Charged
calls/output/prompt reservations are never refunded. No-call phases create no
success observation. Cancellation or process loss can leave no terminal observation;
missing observations are not counted as successes or treated as provider stop.
Historical phase-failure events are not invented into new scoped observations.

A skipped phase buys no extra solver cycles, tool calls, tokens, permissions or
wall time. It merely avoids spending on optional work. Independent verification,
critic/reflection admission, recurrence stopping, capability policy, resource
queues and model pins remain in force. FAST and profiles without these optional
stages do not invoke the guard. Saved graph/revision continuation reuses its recorded
work; it does not replay fresh planning/exploration or reapply this gate to it.

## Evaluation and limitations

Comparison profiles are isolated in the guard scope. Within a profile, recent
cross-case/run timing history remains live; the read-only corpus snapshot does not
freeze it. `compare.adaptive_advice_history_frozen` is false when the guard is
enabled, and the report limitation says so. Old reports are unchanged. Do not
present an adaptive run as an experiment with frozen policy history.

The guard does not know physical GPU time, energy, model load state, queue causes,
semantic difficulty or marginal verified benefit. It can skip useful advice after
transient failures and can probe again after forgetting history. Concurrent tasks
can all observe insufficient history before any finishes; original admission and
resource ceilings still apply independently. Same-user database tampering, model
replacement under unchanged labels and trusted blocking callbacks remain outside
its guarantees. Native callback execution cannot be forcibly preempted.

Unit tests cover history/window/boot/scope/phase/budget/tail bounds, exact thresholds,
completed-but-too-expensive work, timeout suppression, reopened persistence, model
and execution-profile separation, independent exploration suppression, expiration
and decision/observation audit failure. A real CLI/local HTTP regression runs three
separate processes: the first two retain charged planner timeouts and solve; the
third skips planning and reserves only its 384-token solver. This demonstrates the
mechanism with fixtures, not a real-model speedup or general quality preservation.
