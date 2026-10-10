# Bounded reasoning and verification

Jev+ owns task lifecycle, routing, capability selection and operator ceilings in
`src/engine.rs`. Jev-R owns the replaceable inference session and role graph in
`src/reasoning.rs`. Existing models and deterministic algorithms implement these
roles; no new neural architecture is claimed.

| Mode | Maximum model calls | Maximum solver cycles | Stages |
|---|---|---|---|
| FAST | 1 | 1 | Solver, deterministic verification |
| STANDARD | 2 | 1 | Planner, solver, deterministic verification |
| DEEP | 8 | 3 | Planner, solver, critic, reflector, bounded revisions |
| RESEARCH | 16 | 4 | Planner, explorer, solver, critic, reflector, bounded revisions |

These mode limits are clamped to operator configuration. Defaults impose six
calls, four cycles, 1,600 reserved output tokens, 128,000 cumulative prompt bytes,
four tool invocations, 45 seconds per model request and 120 seconds per task.
A solver reserves 384 output tokens; each other model role reserves 128.
Reservations occur before dispatch and remain charged on errors/cancellation.
Reported output exceeding the reserved request limit fails the task.

Critic JSON contains exactly `revise` and `reason`. A specific bounded reason
can justify a reflector and another solver. Invalid critique, no useful revision,
identical candidate hashes, cycle/call/token/byte ceilings, timeouts or
cancellation stop execution. A missing budget never fabricates a successful answer.

Verifier-R is deterministic and independent of model assertions. The current
executable criterion is bounded arithmetic evaluation. General generated answers
are marked `unverified`, even when critics approve them. Retrieved documents are
reference evidence, never permissions or instructions. A benchmark can separately
verify an answer against its supplied held-out expected result using exact match.
That does not change the runtime's verification status.

Each completed model role produces a typed node with parent, cycle, model label
and content hash, persisted with its output and usage in task events. This is a
bounded role dependency graph; general decomposed subtask DAG execution remains
outstanding. Configured model labels are recorded when providers omit a model
identity; this does not attest to immutable backend weights.

`InferenceBackend` is replaceable and shared by logical roles. `LocalBackend`
uses the bounded loopback OpenAI-compatible adapter. Tests inject a deterministic
script backend that checks actual call order and revisions. Provider-reported
input/output tokens are optional; any unreported/failed in-flight call makes the
task totals unavailable. Runtime timers measure time including cancelled calls.
GPU memory, load costs, energy and peak RAM remain unavailable.

Input prompt bytes are enforced independently. Exact input-token preflight needs
a compatible tokenizer/counting endpoint and is not yet implemented; byte limits
must not be described as exact token limits. Different role model configurations,
semantic task plans and general independent executable verifiers are also pending.
