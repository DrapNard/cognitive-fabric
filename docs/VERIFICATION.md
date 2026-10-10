# Independent result verification

The runtime can verify arithmetic and explicit dependency-order objectives without
trusting model confidence. General prose remains unverified.

Example objective (pass as the quoted argument to `ask`):

```text
order:{"nodes":["build","test","review"],"before":[["build","test"],["test","review"]]}
```

The solver must return only a JSON array of node identifiers. The deterministic
Rust checker requires every node exactly once and all precedence constraints.
Multiple valid orders are accepted. Criteria come exclusively from the original
request; retrieved documents and model output cannot replace them. Unknown fields,
cycles, missing dependencies and duplicate edges fail closed. Bounds are 64 nodes,
256 edges, 32 ASCII identifier bytes, 10,000 criterion bytes and 4,096 answer bytes.

Each check requires the task-scoped `ordering.verify` capability and consumes one
tool invocation. Failures feed checker diagnostics into the next solver cycle;
existing call, token, cycle, wall-time and cancellation limits still apply. Repeated
answers stop as no progress. Passing checks stop immediately and record criterion
version and objective/answer hashes in the durable verification event. The checked
answer is preserved: optional language synthesis cannot rewrite it. The alone
comparison profile receives no executable checker.

This proves only a permutation satisfying the supplied precedence constraints.
It does not verify real-world scheduling feasibility, optimality, external facts,
or general model safety. Invalid criteria currently produce a failed task; they
are not silently repaired by language interpretation. Verified ordering results
are not automatically published as verified memory.

Tests cover alternate valid orders, violations, duplicate/missing/extra nodes,
forged verification objects, invalid criteria, failed-then-successful revision,
stagnation, tool budget denial and preserving the exact checked answer.
