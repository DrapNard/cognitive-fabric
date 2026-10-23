# Structured progress stopping

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
Other criteria and unrestricted prose do not use the finite-domain policy; the
separate shortest-path policy is described below. Failed checks of
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

## Compare quality and cost before deployment

The explicit [controlled ablation](BENCHMARKING.md#controlled-progress-stopping-ablation)
runs the same collective twice per finite-domain case, disabling the policy in
one arm and enabling the selected threshold in the other. It reports paired
quality changes and actual resources instead of inferring utility from lower
call counts. It does not install its winner or change persistent configuration.
Public [diagnostic cases](../evaluation/README.md#public-finite-domain-diagnostics)
have a separate exhaustive oracle; their authored provenance does not establish
held-out independence or general quality.

The first retained [public Qwen diagnostic](../evaluation/results/README.md#public-finite-domain-progress-ablation--2026-10-21)
found 14 versus 13 valid witnesses with the policy disabled/enabled, using 85
versus 78 calls. Only one plateau fired, on an unsatisfiable case; the observed
satisfiable regression did not trip the plateau rule. Different reasoning paths
and a single public run prevent causal/general claims. This evidence supports
keeping the policy optional, not a default deployment recommendation.

## Shortest-path policy

The separate `shortest-path-progress/v1` policy extends the same optional setting
to explicit `path:` objectives. Candidate JSON arrays are
bounded to 4,096 bytes and 64 declared node IDs. Unknown nodes, malformed JSON or
larger arrays are incomparable and reset the non-improvement streak.

For a reachable goal, breadth-first search derives the shortest length. The score
adds 65 per incorrect endpoint or undeclared directed edge, plus the absolute
candidate-length difference. The 64-node cap makes each structural penalty larger
than any length deficit. This is a weighted violation measure, not a distance to
a correct answer. For an unreachable goal, a nonempty comparable candidate scores
one for violating the required empty witness; `[]` is accepted by the checker.
Scoring never proves correctness, impossibility, provider termination or utility.

Separate path history and policy markers preserve finite-domain history bytes.
Fresh policy-enabled tasks record the version in execution settings and reasoning
context. Continuation must reconstruct the exact history and preserve whether the
original task enabled the policy; an unmarked legacy task cannot acquire it midway.
Provider-vetoed compiled witnesses consume their cycle as explicitly unscored,
resetting plateau accumulation without becoming ordinary rejected score-zero data.
Controlled comparisons accept matching `finite_domain_v1`/`csp:` and
`shortest_path_v1`/`path:` cases. Mixed corpora retain per-case criteria and use the
report label `mixed-structured/v1`; lower call counts do not establish better quality.


Verification on final frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**921 passed across 60 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
The 12 new integration tests cover runtime stopping, original-budget HTTP
continuation, repeated charged result rechecks with one proof, legacy behavior,
nine tamper/no-dispatch cases and a two-case HTTP comparison. Helper tests cover
exhaustive small-graph score/checker equivalence, bounds, history transitions,
criterion-label preflight and malformed/duplicate recovery evidence.

Tests exposed and fixed shortest-path result-proof deduplication missing from the
existing finite-only gate. Review also fixed duplicate cycle-zero proofs and
foreign/malformed progress audit acceptance. Initial fixture/compile/lint errors
were corrected; no tests were removed. Existing socket/process, migrations, mock,
malformed-frame, policy-denial, cancellation and rollback coverage remains green.
Protocol schemas/bindings are unchanged; bundled generator/runtime checks pass.
`flatc` remains unavailable, so fresh generation was not run. Localhost port 1234
refused discovery. No real-model quality, GPU, latency or energy gain is asserted.
Jev+ and Jev-R remain **Partial**: automatic ownership-aware provider lifecycle,
broader isolated executable verification and independent live-model effectiveness
measurements remain open. The next lifecycle step requires explicit provider
instance identity and durable ownership, without granting models mutation rights.
