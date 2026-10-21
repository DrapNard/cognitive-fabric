# Finite-domain progress stopping

Architecture sections 5 and 9 require bounded revision and a stopping rule when
improvement becomes marginal. The existing exact-answer recurrence rule catches
repeated bytes. Optional `progress_stopping` also tracks independently computed
constraint violations for explicit `csp:` objectives.

## Configuration

Add this owner-controlled setting to `config.json`:

```json
{
  "progress_stopping": { "plateau_cycles": 2 }
}
```

Omission/null disables it and preserves legacy serialized configuration. The
strict setting admits only `plateau_cycles`, from 2 through 4. These thresholds
are bounded operational choices, not tuned recommendations or statistical claims.
Existing mode cycle limits remain authoritative; a larger threshold may never be
reached in a short task. Settings are pinned with the original execution
configuration and cannot be changed by a model or current configuration on resume.

## What is measured

The compiled `finite_domain/v1` verifier counts failed declared constraints only
for one complete assignment with exactly the declared integer keys and values
inside every declared domain. The score ranges from 0 through 128. Parsing and
relation evaluation share the full checker's strict JSON, byte/structure bounds
and i128 arithmetic. Malformed JSON, duplicate/extra/missing keys, non-integers
and out-of-domain values are **incomparable**, represented by a null score.
Other criteria and unrestricted prose do not use this policy. Failed checks of
published memory before whole-goal solving are separate retrieval evidence; they
do not initialize or advance the solver progress history.

The first comparable rejected candidate establishes a best score. A strictly
lower score updates that best and resets the non-improvement streak. Equal or
higher scores increment the streak against the best observed score. An
incomparable candidate resets the streak without erasing the best score. Once
the configured number of consecutive comparable non-improvements is reached,
Jev-R stops with `no_progress` before buying another solver revision. Exact-hash
recurrence, escalation decisions and all original budgets still apply.

The full independent verifier always runs first under the original task capability
and tool accounting. Any valid witness completes immediately, even when the next
failed candidate would have reached the plateau threshold. A score never grants
verification or permissions. Scoring requires no new inference, tool capability
or extra call allowance; it is bounded deterministic work following the charged
independent check.

A constraint count is not a distance to a satisfying assignment. Constraints can
have different difficulty, overlap or conflict. A useful search can traverse a
plateau or temporarily get worse; this heuristic can stop such a search early.
It proves neither impossibility nor optimality, neural intelligence, provider
termination or an improvement in real-model quality/latency. It never searches
the assignment space or executes generated code.

## Durable evidence and continuation

Each rejected candidate in an enabled finite-domain task requires a
`reasoning_progress` event from `verifier-r` before further escalation. Fixed
metadata binds the policy version, exact objective hash, criterion, cycle,
answer hash, score, best score, streak and stopping decision. It explicitly
records `verified:false` and `permissions_granted:false`; no answer content is
added to this audit. Audit failure prevents further inference. The existing
reasoning result remains separately stored under its original protections.

Ready solver checkpoints include a compact `progress_state` consistency anchor.
Continuation rebuilds state from the original recorded solver texts and failed
independent checks, recomputes assessments and compares durable audits and the
checkpoint. It does not trust saved counters as authority or start a fresh
streak. Missing, altered, duplicate, gapped, future or incorrectly ordered
assessment evidence and an already stopped history prevent new dispatch.
Original execution leases, deadline, resources, model/context pins and permissions
still apply. Operator result recovery uses a fresh charged independent check and
the same progress rules; it does not replay inference or refund old attempts.
A recorded runtime check and one matching saved-result recovery proof may coexist.
Repeated operator rechecks retain fresh tool charges and recovery audits while
reusing the identical per-cycle recovery proof, without duplicating assessments.

Default-off legacy tasks retain their existing continuation format. This feature
adds no SQLite migration or internal protocol/schema change. Same-owner database
or source compromise remains outside the local trust guarantee.

## Verification scope

Deterministic tests cover exact violation counts, acceptance equivalence, order
invariance and incomparable input shapes. Scripted runtime tests cover disabled
compatibility, distinct-candidate plateaus, improvement, incomparable resets,
valid-witness precedence and audit failure. Recovery tests exercise evidence
recomputation and tampering; loopback HTTP continuation checks that only the next
solver runs under the original charges. These are functional regressions, not an
independent held-out study or a measured real-model speed/quality gain.
