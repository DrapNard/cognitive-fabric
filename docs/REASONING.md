# Bounded reasoning and verification

Jev+ owns task lifecycle, routing, capability selection and operator ceilings in
`src/engine.rs`. Jev-R owns the replaceable inference session and role graph in
`src/reasoning.rs`. Existing models and deterministic algorithms implement these
roles; no new neural architecture is claimed.

| Mode | Maximum model calls | Maximum solver cycles | Stages |
|---|---|---|---|
| FAST | 1 | 1 | Solver, deterministic verification |
| STANDARD | 4 | 1 | Planner, solver, deterministic verification |
| DEEP | 10 | 3 | Planner, solver, critic, reflector, bounded revisions |
| RESEARCH | 18 | 4 | Planner, explorer, solver, critic, reflector, bounded revisions |

These mode limits are clamped to operator configuration. Defaults impose eight
calls, four cycles, 1,600 reserved output tokens, 128,000 cumulative prompt bytes,
four tool invocations, 45 seconds per model request and 120 seconds per task.
The whole-goal solver reserves 384 output tokens, the planner 256, subtask solvers
192 each, and other model roles 128.
Reservations occur before dispatch and remain charged on errors/cancellation.
Reported output exceeding the reserved request limit fails the task.

Critic JSON contains exactly `revise` and `reason`. A specific bounded reason
can justify a reflector and another solver. Invalid critique, no useful revision,
recurring whole-goal candidate hashes, cycle/call/token/byte ceilings, timeouts or
cancellation stop execution. A missing budget never fabricates a successful answer.

Verifier-R is deterministic and independent of model assertions. Current
executable criteria include bounded arithmetic, dependency-order checks and
explicit inline SQL execution. General generated answers
are marked `unverified`, even when critics approve them. Retrieved documents are
reference evidence, never permissions or instructions. A benchmark can separately
verify an answer against its supplied held-out expected result using exact match.
That does not change the runtime's verification status.

Each completed model role produces a typed node with parent, cycle, model label
and content hash, persisted with its output and usage in task events. Planner JSON is validated by `src/planning.rs`: one to four bounded subgoals,
unique IDs, existing dependencies and an acyclic graph. Jev+ executes ready nodes
in stable topological order, sequentially to avoid redundant GPU residency.
Each subtask receives only its predecessor results and the original objective
with pinned references. Outputs remain unverified advice. A final whole-goal
solver combines the results; subgoals cannot replace the original request.
Call/token/prompt-byte admission reserves that solver before decomposition starts; insufficient
budget skips all subtasks. Invalid plans fall back to the original objective.
Pending/running nodes become cancelled, failed or skipped on termination.
Graph and state transitions are persisted in task events, including dependency
IDs and bounded result snapshots. This is not crash-resumable execution. Configured model labels are recorded when providers omit a model
identity; this does not attest to immutable backend weights.

`InferenceBackend` is replaceable and shared by logical roles. `LocalBackend`
uses the bounded loopback OpenAI-compatible adapter. Tests inject a deterministic
script backend that checks actual call order and revisions. Provider-reported
input/output tokens are optional; any unreported/failed in-flight call makes the
task totals unavailable. Runtime timers measure time including cancelled calls.
GPU memory, load costs, energy and peak RAM remain unavailable.

Input prompt bytes are enforced independently. Exact input-token preflight needs
a compatible tokenizer/counting endpoint and is not yet implemented; byte limits
must not be described as exact token limits. Broader independent executable verifiers and crash-resumable graphs remain pending;
operator-only interrupted-task reconciliation is described in RECOVERY.md.

The optional interface and synthesis stages share these budgets. See
LANGUAGE_INTERFACE.md. Exact input-token preflight and general independent
executable verifiers remain pending; per-role model configuration is implemented.

## Preserve the first whole-goal solver

Decomposition admission now bounds every subgoal prompt, including worst-case JSON
escaping of its capped predecessor results, and reserves the first whole-goal
prompt. Strategies are capped at 4,000 characters (at most 16,000 UTF-8 bytes).
Prompts must also fit the 32,000-byte inference transport bound. These conservative
bounds can refuse a graph that would fit with unusually short results; refusal
falls back to whole-goal solving instead of consuming its byte budget.

Research exploration independently requires two remaining calls, 512 reserved
output tokens (128 explorer + 384 solver), and prompt-byte headroom for both.
Admit/skip decisions and reserved prompt bytes are audited. This protects the
first whole-goal attempt; it does not reserve all future critique/revision stages
or guarantee model quality. Wall time and cancellation can still stop any stage.
Tests cover prompt-limited decomposition, call-limited and token-limited exploration,
and combined low-budget fallback without expanding operator ceilings.

## Reviewed empirical retry admission

An operator-trusted host may install an immutable empirical policy that evaluates
verified-retry rates and cost per success under fixed thresholds before buying
another solver after checker rejection. Study scope includes configuration,
compiled prompts, runtime-source bytes, mode, cycle and criterion. Failed samples
remain counted and unverified prose cannot borrow structured-task evidence.
Inspection is available through `retry-study-evaluate` without creating state.
See [empirical escalation](EMPIRICAL_ESCALATION.md) for bounds, host installation,
operator-attested provenance and the remaining Jev+ components. This does not
establish calibrated expected utility or a live improvement in model quality.

Verifier-R also checks explicit `csp:` finite-domain assignments with bounded
linear and all-different constraints. Alternative witnesses are accepted; failed
checks revise through the original bounded loop. See
[finite-domain verification](VERIFICATION.md#finite-domain-assignments) for grammar
and the limits of its satisfaction-only evidence.

Optional language interpretation now has
[explicit first-solver admission and a bounded phase](INTERPRETATION_BUDGETS.md).
Insufficient resources skip interpretation; timeout fallback retains charges and
never weakens provider/policy/audit failures or cancellation.

## Stop nonadjacent candidate recurrence

`src/reasoning/convergence.rs` tracks the first whole-goal solver hash in each
reserved cycle, including nonadjacent repeats such as A → B → A. Cycle-zero
planning/workflow advice and later same-cycle calls are excluded, including
critic/reflector calls labelled `solver` by recursive ablations. History is bounded
to 16 cycles and reconstructed from at most 64 durable role nodes on continuation.
Missing, malformed, gapped or future-cycle history fails closed.

Independent executable verification runs first. A verified answer completes even
if its hash was previously seen. Otherwise a repeated candidate preserves the
latest answer and charges, sets `no_progress`, and buys no further revision calls.
Each unverified/rejected candidate requires a `candidate_progress` audit with the
policy version, objective/answer hashes and previous matching cycles before any
escalation. Audit failure fails the task. No additional inference is required.

This exact-content heuristic is conservative: a later attempt might have improved,
and different wording can evade it. It proves neither unsatisfiability nor model
quality. No latency or accuracy improvement has been measured on real models.
See [recovery](RECOVERY.md#candidate-recurrence-and-rejected-continuations) for
checkpoint admission and rejection status.

## Explore several bounded hypotheses

RESEARCH Explorer-R now requests two or three structured alternatives. Strict
bounded parsing, canonical ID selection and once-per-cycle use retain independent
verification, critique/reflection and original budgets. Selected advice stays
within the existing strategy cap; recovery reconstructs the portfolio from the
original charged explorer result without replay. See
[strategy portfolios](EXPLORATION.md) for schema, audit gates, legacy fallback,
custom escalation budgets and the absence of measured quality/utility claims.

## Optional recent-cost admission

Owner-configured [advice guarding](ADVICE_GUARD.md) can decline fresh planning or
exploration after repeated scoped failures or observed cost above the current
phase allowance. It reads bounded same-boot metadata, preserves first-solver
resources, records decisions/outcomes and keeps comparison profiles separate.
Default behavior is unchanged; this heuristic is not calibrated expected utility.
