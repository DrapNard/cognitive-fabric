# Bounded client-conversation collective

**Status: Implemented for bounded client tool-loop sessions; broader framework
integration remains Partial.** When enabled, the client-facing agent endpoint runs
a durable Jev+ / Jev-R role graph across ordinary tool-call requests. Opaque CGF
tool-call IDs link client-returned history to one root task and its persisted
budget/checkpoint. CGF does not execute client tools. This is deterministic
orchestration over the configured model, not a newly trained model or a claim of
improved reasoning quality.

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

The initial request pins its effective effort, configured model registration,
mode, temperature, limits and original deadline for the tool-loop session. Every
continuation must match those pins; configuration drift is refused instead of
starting a fresh budget. The same model and effort are forwarded for each role.
Actual roles depend on remaining calls, cycles, cumulative input/output allowances
and time. A smaller budget can omit advisory roles while preserving the solver.
This is deterministic orchestration; expected utility is not empirically calibrated.

## Client-owned tools

All role calls retain the supplied conversation, function descriptions and schemas.
Advisory roles receive `tool_choice: none`; only the solver may propose functions,
subject to the original tool choice, current catalog and proposal ceiling. CGF
returns a valid tool proposal immediately without speculative critique or
execution. The client authorizes and executes the tools, then supplies correlated
results/errors and updated history in its next ordinary request.

Before returning proposals, CGF replaces provider-generated IDs with opaque
`cgf_<32 lowercase hex digits>` IDs. Standard clients echo these IDs in tool
results, so no custom session header is needed for a tool loop. All calls proposed
in one response belong to one checkpoint. Missing, altered, unknown, replayed or
out-of-order continuation history is rejected before inference; it never falls
back to a fresh task budget.

Text candidates can receive a strict bounded critique. Only an actionable critique
and room for critique, reflection and a revised solver admit revision. A repeated
candidate stops for lack of progress; call/cycle/deadline ceilings remain hard.
Advisory results are quoted as untrusted evidence. Tool reports and model claims
cannot become independent verification.

## Budgets, persistence and privacy

The operator's configured `max_tokens` is the cumulative output-reservation ceiling
for the whole client tool loop, including hidden advisory calls and later turns.
Each request's `max_tokens` limits that step's additional reservation and can lower
the remaining allowance; it cannot raise the root session ceiling. Calls, cycles,
input bytes, output reservations, tool proposals and bounded internal tool use are
persisted before dispatch and never reset between tool turns. FAST permits one
model call, so it cannot resume a tool loop after using that call; use STANDARD or
higher effort when a solver may need to continue after client tools. Usage sums
reported role usage; missing usage stays unknown. All serialized role inputs,
including repeated conversation/tool catalogs and runtime instructions, count
toward the cumulative configured input-byte allowance. Role calls can therefore
be skipped on a long history even when calls remain.

Collective requests allow at most 254 client messages, leaving two runtime messages
inside the existing 256-message typed limit. Managed role frames must each fit the
existing 256 KiB ceiling. Optional-role admission preserves a bounded solver input.
Each role reservation is committed before entering the backend. Errors and
cancellation do not refund it or cause retries. One original deadline spans the
whole tool loop. Managed mode uses a fresh authenticated one-use worker for each
role; workers remain owned until reaped. None of this proves an external inference
server stopped GPU work.

Trace events contain role/mode, hashes, reservations, usage and stop reasons, not
the raw conversation, client tool arguments or results. A bounded private SQLite
checkpoint does retain derived planner/explorer advice (up to 2,000 bytes) and
checkpoint references (up to 8,000 bytes); advice may quote conversation content.
These records are sensitive and are not encrypted by this feature. Terminal or
failed transitions clear the current advice checkpoint, but SQLite WAL history is
not secure-erased. Existing terminal-task retention deletes associated session
rows by cascade.

The task policy grants only `llm.generate`. A bounded `knowledge.search`
reservation, when selected, counts as one internal tool operation and uses the
task-pinned local RAG documents, memory and skill-manifest references through the
same configured model. Catalogs, client prompts, tool results and model advice
cannot create permissions. Client tool execution remains the client's
responsibility.

Only a small set of pure, compiled checks over the original root goal—including
supported sort/order/path constraints and related deterministic criteria—can
complete independently as verified. Client-supplied tool results remain untrusted.
All other answers and tool proposals remain `unverified`; a critic or model does
not independently confirm factual correctness.

## Expiry, recovery and new goals

The original elapsed-time allowance is restored only on the same Linux boot using
the saved boot clock. After reboot or clock unavailability, a waiting session is
refused. A checkpoint waiting for client tools is resumable after process restart;
an in-flight call interrupted by cancellation or crash is uncertain, charged, and
never replayed automatically. Waiting-session expiry is cleaned up under a bounded
later admission. `recover-tasks` can abort active/waiting work; it does not refund
budgets or retry inference.

Unknown handles (including after retention), modified/compacted/split histories
and changed collective settings fail closed. Disabling the collective also makes
its prefixed continuation handles refuse; restore the original collective
configuration to continue, or start a new conversation. The client must start a
new conversation to obtain a new budget. After a final answer closes the tool-loop session, echoing
that answer and adding a new user message starts a new root task with a fresh
operator-bounded budget. This standard tool-ID mechanism cannot associate a
user-only follow-up after closure unless the request includes the echoed history.

## Integration boundaries

This endpoint persists and resumes its bounded role graph during client tool-loop
conversations. It does not expose the Engine's full expert registry, custom expert
execution, project-profile/model selectors or general task recovery workflow
through this API. Those remain separate Engine capabilities. The client catalog
may change between turns; every advertised function still requires the client's
own permission and schema checks. Existing collective aliases retain their
goal-only contract.
