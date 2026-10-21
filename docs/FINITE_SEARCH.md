# Bounded exact finite-domain search

Architecture sections 5 and 9 call for algorithmic search and a low cost per
verified solution. Optional operator configuration enables a deterministic Rust
search before fresh whole-goal Jev-R inference:

```json
{"finite_search":{"max_nodes":4096}}
```

This is a configuration fragment, not a replacement for the complete config.
The setting is absent by default. `max_nodes` must be 1–4096. It applies only to
explicit `csp:` objectives with verification enabled; the LLM-only comparison
profile remains excluded. It does not interpret natural language as constraints.
The existing reasoning entry gate remains in force: a zero cycle ceiling disables this phase too,
although a direct search success consumes no model cycle.

## Search and outcome contract

Depth-first search visits variables and domain values in declaration order.
Each candidate partial assignment, including the empty root, costs one node.
A constraint can prune a branch only once all its variables are assigned. The existing strict grammar,
32-variable/64-value domain ceilings, 128-constraint ceiling and bounded i128
arithmetic remain in force. The deadline is checked during traversal and before
returning its result. The engine yields before traversal so a ready task cancel
can win. A cancel arriving during this synchronous traversal is observed only
after it returns; the node/structure ceilings bound that delay, without a hard
physical latency guarantee.

- **Witness:** serialize an exact assignment, then independently authorize and
  execute the original satisfaction checker before returning verified success.
- **Unsatisfiable:** only a completely exhausted search tree permits this result.
  The task remains `rejected`, with `proven_unsatisfiable` as its stopping reason,
  because its contract requests a satisfying witness. No model impossibility
  declaration is accepted as proof, and witness-only benchmarks award no success.
- **Limit reached:** retain the search charge and fall back to the existing
  bounded model reasoning path. This establishes no impossibility claim.
- **Deadline reached:** stop under the original task budget.

Declaration order affects work and the first returned witness, not satisfaction.
The hard ceiling deliberately makes many larger instances inconclusive. This is
not a general SAT/SMT engine, an optimality proof or a trained neural solver.

## Permissions, resources and durable evidence

Search requires its own Jev+ `constraints.solve` permission and one tool charge.
Admission preserves a second tool slot for the original independent witness
checker or the fallback solver's check. Search never creates calls, tokens,
cycles or additional wall time. Generation-only agents and model roles receive
no search grant. An explicit operator setting cannot authorize external programs,
network access or generated code.

A required `finite_search` audit records bounded fixed metadata including the
objective hash, configured ceiling, explored nodes and outcome. Audit failure
stops execution. The separate original-criterion verification event is the
success authority for witnesses. Search introduces no protocol change, SQLite
migration, model loading or active-weight mutation.

### Continuation after an incomplete search

Ready solver and supported advisory-graph checkpoints may continue after a
recorded `limit_reached` search. Admission strictly validates the original
configuration, objective hash and first reasoning context, one authorized search
reservation and its ordered receipt. The receipt must record exactly the original
node ceiling, no verification/permission authority, and exhaustion of that ceiling.
It must precede the following reasoning work and the checkpoint being restored.
Missing, duplicate, oversized, malformed, altered or out-of-order records refuse
continuation before new inference or budget changes.

Preview and continuation audits expose `finite_search_history`: bounded receipt
identities and original charges, without query or answer bodies. The resumed
session retains original tool/call/token/cycle charges, model/context pins and
deadline. It continues the saved reasoning strategy and never reruns search.
Revalidating or continuing again does not create another search allowance.

A search reservation without a complete receipt, completed witness/contradiction,
deadline interruption, or a budget-skipped search remains unsupported for this
continuation path. Saved model-result reconciliation is supported after a valid
limit receipt as described below. No incomplete traversal is reconstructed, rerun
or refunded. These
owner-local consistency checks do not authenticate against a malicious database
owner or establish provider termination.

The progress-policy ablation refuses enabled finite search so it cannot silently
compare two arms that bypass the policy under study. Broader search-phase recovery
and independent utility evaluation remain subsequent milestones.

### Reconcile a saved model candidate after the search limit

`recover-verified-result TASK_UUID` can independently recheck a ledger-backed
saved solver candidate after a valid recorded search limit. The original search
authorization/charge/receipt chain is validated before any new check reservation.
A fresh check consumes one remaining original tool allowance, and its proof and
recovery audit commit atomically. Successful full-criterion checking completes the
task; a failed check retains its charge without fabricating success. Search and
inference are never replayed. The recovery report exposes `finite_search_history`.

The full original criterion applies even when a candidate came from advisory
subtask work. A satisfying assignment is a valid witness regardless of its source
role. A rejected advisory candidate does not advance whole-goal progress or create
a new reasoning cycle. Its failed proof must remain linked to the recorded solver
text and independent rejection; it cannot poison later graph continuation or be
borrowed as a whole-goal assessment. With progress tracking enabled, repeated checks retain their new charges and
reuse only identical bounded proof metadata. Each fresh failed whole-goal check
records a new ready checkpoint when eligible, because its consumed tool count
has changed; prior immutable checkpoints are not rewritten.

A rejected whole-goal candidate may produce a ready revision checkpoint only under
the existing recurrence, plateau, escalation, graph, resource and original-clock
checks. Explicit reconciliation has the existing five-second SQLite computation
ceiling; it grants no fresh task wall time. An expired original deadline prevents
new model reasoning even when a saved witness can still be checked. Audit failure
rolls back the new charge, proof and prospective checkpoint together.

Terminal direct-search witnesses/contradictions, incomplete or skipped searches,
unknown inference outcomes and broader traversal-state recovery remain outside
this path. These checks establish local consistency, not weight identity, physical
provider termination or general neural safety.

## Evidence and limits

Unit tests exercise independent small-domain enumeration, exact bound edges,
contradictions, cancellation hooks and deterministic witness checking. Runtime
regressions cover witness/contradiction/fallback paths, profile exclusions and
tool accounting, alongside the public diagnostic corpus. These are functional
checks; no live-model efficiency improvement, held-out quality, physical resource
bound or general Jev-R intelligence follows from them. Jev+ and Jev-R remain
Partial.

Final workspace checks pass: formatting, all-target build, 763 tests across 43
targets (none ignored), and Clippy with warnings denied. The public-corpus runtime
regression finds 14 checked witnesses and proves three contradictions with no
model calls. This fixed authored corpus establishes functional behavior only; it
is not a live-model speed comparison or independently curated held-out study.

### Recovery verification

Eight validator unit tests cover exact receipt matching, reserved verifier budget,
configuration mismatch, disabled/unsupported histories, strict JSON and event/byte
ceilings. Three integration tests use the public continuation API: repeated ready
solver recovery with original pins, a partially executed advisory graph over
loopback HTTP, and 18 independent tampering cases. Invalid admission leaves the
task trace unchanged. Successful continuation retains one original search receipt
and reservation and charges only new inference/verification work. These tests use
a scripted provider; they do not establish real-model efficiency or GPU behavior.

Final recovery workspace verification: formatting, all-target build, **774 passing
tests across 44 targets** (none ignored), and Clippy with warnings denied. The
compiled protocol pair/integrity checks pass; no schema or generated binding was
changed, and unavailable `flatc` prevented fresh generation.

### Saved-result recovery verification

Nine integration tests cover a recovered witness with no inference/search replay,
invalid receipts with no new writes, verifier-audit rollback, repeated charged
checks, proof/assessment crash windows, positive and negative advisory candidates,
partial-graph continuation, plateau refusal and expired-clock reconciliation.
The advisory proof unit regression validates call/hash/reason/objective/source/
shape and duplicate-proof tampering without changing whole-goal progress.
Loopback HTTP fixtures confirm the actual resumed solver and graph call counts.
These are deterministic/scripted recovery checks, not live-model quality or
physical performance measurements.

Final saved-result workspace verification passes: formatting, all-target build,
**784 tests across 45 targets** (none ignored), and Clippy with warnings denied.
Protocol schemas and bindings are unchanged; bundled pairing/integrity tests pass,
while unavailable `flatc` prevents fresh generation.
