# Advisory interpretation admission

Language interpretation is optional executive advice. It must not consume the last
call or output reservation needed to answer the original objective.

## Before inference

`Session::interpretation_advice` requires two remaining model calls: one interface
call and one whole-goal solver. Remaining output reservations must cover configured
`interface_tokens` plus 384 solver tokens. Both individual prompts must fit the
32,000-byte bound; cumulative input capacity must cover the interface prompt plus
the original solver base and 64 bytes of initial solver framing. Defaults therefore
need at least 512 output tokens for interpretation plus solving.

These are admission checks, not new charges or expanded ceilings. Actual dispatch
still commits the existing per-call reservation. A refusal records
`interpretation_admission` and the compatible `skip_interpretation` schedule event,
then leaves the original reasoning path available. Optional planning/workflows
remain governed by their own solver-preserving admission checks. Cheap call/output
refusal also avoids creating an unnecessary capability-reference snapshot.

## Time, errors and cancellation

An admitted interface call has an audited phase ceiling of one-third of remaining
original task time, capped by the inference timeout. The task deadline and worker
grants are not changed. Only the runtime's typed inference timeout permits
`interpretation_failure` fallback to the original objective. Charged calls, output
reservations and prompt bytes remain charged. Interrupted usage and queue metrics
remain unknown; elapsed inference accounting is finalized.

Policy, provider and audit errors remain fatal. A provider error merely containing
the words "Inference timeout" cannot trigger fallback. Failure auditing must persist
before further inference. User cancellation terminates the task and does not start
a fallback solver. Dropping a request does not prove the external provider stopped;
configured resource quarantine and all ordinary admission rules still apply, so
fallback is an attempt within remaining resources, not guaranteed completion.

## Advisory context

Parsed interpretation can still supply bounded capability metadata or request
clarification without replacing the original goal. Its raw advisory text is added
to context only if the actual encoded solver prompt and remaining cumulative input
budget still fit. `interpretation_context` records acceptance/refusal before input
is changed. Refusal does not refund the interface call or disable authoritative
original constraints. Capability-directed retrieval checks the resulting context
under its own tool/prompt/verifier reserves.

Tests cover one-call and exact output/input boundaries, context refusal, typed
vs textual timeouts, preserved unknown metrics, admission/phase/failure/context
audit denial and cancellation. These functional checks do not establish live
interpretation accuracy, calibrated expected utility or a reasoning-quality gain.
