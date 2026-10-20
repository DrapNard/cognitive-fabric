# Bounded strategy portfolios

Architecture sections 5 and 9 require alternative hypotheses and bounded revision.
In RESEARCH collective execution, Explorer-R can return two or three distinct
approaches in one existing, budgeted explorer call:

```json
{
  "alternatives": [
    {"id": "a", "strategy": "Try merge sort", "limitations": "Needs scratch space"},
    {"id": "b", "strategy": "Try insertion sort", "limitations": "Quadratic work"}
  ]
}
```

`src/reasoning/exploration.rs` strictly validates at most 4,096 response bytes.
There must be two or three entries, with unique 1–16-character ASCII alphanumeric
or underscore IDs. Strategies are nonempty and at most 480 UTF-8 bytes;
limitations are nonempty and at most 160 bytes. Unknown fields, duplicate JSON
keys, duplicate IDs, strategies equal after whitespace/case normalization,
unsupported control characters and surrounding prose on structured JSON are
rejected. A complete `json` fence is accepted by the existing bounded parser.
These mechanical differences do not prove semantic diversity or useful limitations.

## Selection and revision

Entries are sorted by ID, independent of response-array order. Whole-goal cycle 1
uses the first approach, cycle 2 the second, and cycle 3 the third if present.
Each approach is selected at most once, only when an original solver cycle is
admitted. Available alternatives never force a retry after critic/policy refusal,
verification success, recurrence or budget exhaustion. After exhaustion, ordinary bounded revision
continues without an added portfolio hypothesis. Canonical ordering provides
reproducibility, not a prediction of the best strategy or expected utility.

The selected approach is JSON-encoded and explicitly framed as unverified data.
It precedes other planner/checker/reflection advice within the existing total
4,000-character strategy cap, so long planning advice cannot hide it. The selected
object stays complete even with JSON escaping; remaining advice may be truncated.
Reapplying selection to a saved next-solver strategy is idempotent. Independent
checker rejection still informs the next solver; prose criticism/reflection is
retained. The original objective, constraints and independent acceptance criterion
remain authoritative. No proposal grants a permission or counts as evidence.

Fresh acceptance requires an `exploration_portfolio` audit before using the
portfolio. Each cycle requires `hypothesis_selection` before its boundary/cycle
reservation and solver dispatch. Audit failure fails the task before that work.
Records contain policy/version, source hash, IDs and selection/exhaustion metadata;
strategy/limitation text remains in the original bounded role-result audit.
Selection metadata explicitly denies verification, permissions and calibrated
utility. Parser errors are not copied into these metadata events.

Malformed JSON-looking portfolios are discarded and recorded as `invalid_portfolio`.
Nonempty historical plain-text advice without JSON/fence delimiters, quotes or
unsupported controls still uses the bounded legacy single-advice
path, recorded as `legacy_advice`. It receives no portfolio, ranking, automatic
reuse or verification status. Provider/policy/audit failures remain fatal; only the
existing typed timeout receives its previously implemented fallback.

## Budgets and recovery

The explorer still reserves 128 output tokens. The runtime adds no model calls,
tool calls, solver cycles or deadline allowance. Existing first-solver admission,
exploration phase ceiling, per-call/transport bounds, revision admission and
candidate-recurrence stopping remain enforced. A truncated or unparseable model
response does not buy another explorer attempt. The portfolio must fit the
original call's small response budget; models may fail to return usable JSON.

Custom escalation admission sees the actual next solver prompt including the
selected hypothesis. The same bounded strategy is saved in its durable boundary,
so original-policy recovery validates the original prompt-byte allowance.

Continuation reconstructs the portfolio from its single original cycle-zero
explorer result, already checked against charged inference/node history. Multiple
explorer nodes or an explorer after cycle zero are refused. Parsing uses the same
strict bounds as fresh execution. No new schema/migration or permissions are
restored. The new advisory metadata event is not a recovery authority: original
validated role output and original cycle counters determine the next hypothesis.
Planning/exploration and completed solver calls are not replayed. Existing leases,
operator approval, supported crash windows and original time/resources remain
required. Unknown in-flight outcomes are not resolved by this feature.

The changed explorer system prompt changes the compiled role-prompt fingerprint.
Existing recovery rejects checkpoints with a different prompt fingerprint; it does
not silently migrate historical tasks. Empirical policies likewise need evidence
matching their existing prompt/runtime/configuration scope.

## Evidence and limits

Eight unit/engine tests cover strict parsing, canonical selection, escaping,
strategy caps/idempotence, structured and prose revision, malformed-output discard
and acceptance/selection audit failure. Two actual local HTTP continuation tests
restore the unused alternative without another explorer call, preserve reflection,
and verify original custom escalation prompt-byte coverage. They also cover
reconstruction without the new advisory acceptance event.

This implements bounded alternative-strategy use with existing models. It does
not train a Jev-R neural model, guarantee genuinely different approaches, prove
semantic progress, establish calibrated utility, or demonstrate a quality/latency
improvement. Real-model effectiveness and comparisons under equal budgets remain
separate evaluation work. Native backend callbacks remain trusted host code.
