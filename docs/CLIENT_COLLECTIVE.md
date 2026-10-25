# Bounded client-conversation collective

**Status: Partial.** The optional agent collective is a working per-request Jev-R
role graph. It does not yet join client requests into a durable, resumable
collective session or expose all the Engine's registry/expert/recovery facilities.
The compiled goal/checker Engine remains separate. This is not a trained model or
a demonstrated improvement in reasoning quality.

## Enable it

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" configure-agent-collective --enabled true
```

Restart `cogfabric-server` using that home. The setting defaults off, preserving
existing installations' one-call agent behavior. Disable with `--enabled false`.
Keep `cogfabric-agent` selected in any compatible client. No new model alias,
weights, MCP client, network tool or model-specific role override is required.

| Request reasoning effort | Collective mode | Eligible roles |
| --- | --- | --- |
| `none` or no configured effort | FAST | Solver |
| `low` | STANDARD | Planner, Solver |
| `medium` | DEEP | Planner, Solver; budget-gated critique/revision |
| `high` | RESEARCH | Planner, Explorer, Solver; budget-gated critique/revision |

An explicit request effort overrides the configured default. The same effort is
also forwarded to the same configured inference model on each role call. Actual
roles depend on remaining calls, cycles, cumulative input/output allowances and
time. A smaller budget can omit advisory roles while preserving the solver.
This is deterministic orchestration; expected utility is not empirically calibrated.

## Client-owned tools

All role calls retain the supplied conversation, function descriptions and schemas.
Advisory roles receive `tool_choice: none`; only the solver may propose functions,
subject to the original tool choice, current catalog and proposal ceiling. CGF
returns a valid tool proposal immediately without a speculative critique or any
execution. The client authorizes and executes the tools, then supplies correlated
results/errors and the updated history in its next ordinary request.

Text candidates can receive a strict bounded critique. Only an actionable critique
and room for critique, reflection and a revised solver admit revision. A repeated
candidate stops for lack of progress; call/cycle/deadline ceilings remain hard.
Advisory results are quoted as untrusted evidence. Tool reports and model claims
cannot become independent verification. Verifier-R records structural validation
and an explicit absence of an independent criterion; task status stays unverified.

## Budgets, privacy and lifecycle

`max_tokens` is the aggregate output reservation for the entire request, including
hidden advisory calls, capped by the operator's output limit. It is not a fresh
allowance per role. Usage sums reported role usage; missing usage stays unknown.
All serialized role inputs, including repeated conversation/tool catalogs and
runtime instructions, count toward the cumulative configured input-byte allowance.
Role calls can therefore be skipped on a long history even when calls remain.
Collective requests allow at most 254 client messages, leaving two runtime messages
inside the existing 256-message typed limit. Managed role frames must each fit the
existing 256 KiB ceiling. Optional-role admission preserves a bounded solver input.

The runtime commits each call's reservation/hash before entering the backend;
errors and cancellation do not refund it or cause retries. The original request
wall deadline spans the entire graph, with per-call inference timeouts. The resource
pool admits each call against the same configured solver target. Managed mode uses
a fresh authenticated one-use worker for each role; workers are owned until reaped.
Cancellation may outlive the response while owned cleanup runs, and shutdown drains
that cleanup. None of this proves an external inference server stopped GPU work.

Trace events contain role/mode, hashes, reservations, usage and stop reasons, not
raw conversation, advisory text, tool arguments or results. The task policy still
grants only `llm.generate`. Catalogs, client system prompts and model advice cannot
extend it. Client tool execution has its own permission boundary.

## Remaining integration work

**Each HTTP request is a fresh task budget.** Client history is context, not a
trusted restored reasoning checkpoint. The client must enforce session-wide
iteration/spend limits. Persistent session identity, cumulative reservations across
client tool turns, cancellation/restart reconciliation, replay refusal and exact
pinned role-graph continuation remain to build before calling this full Jev-R
session orchestration. Plain tool-result correlation is not that mechanism.

The generic role graph also does not currently import the Engine's pinned expert
registry, project profiles or compiled factual criteria. Such integration must
retain independent checker authority and must not turn a client-reported result
into verified evidence. Existing collective aliases keep their goal-only contract.
