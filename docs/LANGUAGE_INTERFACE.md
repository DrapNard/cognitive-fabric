# Language interface

Natural requests in STANDARD, DEEP and RESEARCH modes may use the local interface
profile to produce a strict bounded Interpretation: objective, constraints,
required_capabilities and an optional clarification question. The original user
request remains authoritative and is always sent to the solver. Interpretations
are advisory data and cannot change security permissions or trigger tools.

Missing capabilities produce persisted Jev+ gap events. Existing enabled skills
can refine the route within a four-expert bound; unknown names never grant tools.
Ambiguity yields needs_clarification before planning/solving. Malformed JSON is
logged as an intent fallback and the original request is retained. An unavailable
real backend fails visibly; it does not become a demo success.

FAST mode and explicit arithmetic bypass interface calls to avoid unnecessary
inference. The deterministic demo identifies itself and does not pretend to parse
natural language with a real model. Set interpret_goals=false in reviewed settings
to bypass optional interpretation when measuring reasoning-only baselines.

`ask 'Explain the prior result' --conversation TASK_ID` explicitly supplies bounded
historical reference data. Unrelated task histories are never automatically added
to prompts. The CLI and socket pass this reference as a typed optional field.
Historical answers retain their prior verification status and are never instructions.

`ask 'Explain Rust ownership' --explain` optionally uses the synthesizer profile
to explain a structured result. This consumes the same task budget. Output is
prefixed with the deterministic verification status. It does not upgrade the
answer to verified. Plain solver output remains available when synthesis is not
requested or its budget is unavailable.

The default configuration now permits eight model calls. Mode ceilings are FAST
1, STANDARD 4, DEEP 10 and RESEARCH 18, clamped to the operator ceiling. Interface
and synthesis each reserve 128 output tokens. See REASONING.md for other limits.
Logical roles still share one local model unless independently configured.

Scripted integration tests verify goal preservation, permission injection denial,
clarification, malformed-intent fallback, explicit historical-reference isolation
and synthesis status labels. Live-model interpretation quality is unverified until
a compatible local server is running.
