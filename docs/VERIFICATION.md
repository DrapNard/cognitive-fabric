# Independent result verification

## Trusted host verifier extension

Native Rust hosts can select an additional verifier with
`Engine::with_verifier_provider`. It applies to the four existing structured
criteria: dependency ordering, integer sorting, shortest paths and finite-domain
assignments. The original compiled criterion must also accept the exact candidate.
A provider can veto a valid candidate; its acceptance alone cannot establish a
verified result. Ordinary prose and the generation-only comparison profile gain
no checker authority.

The host supplies a `VerifierDescriptor` containing an identifier, canonical
semantic version and lowercase implementation SHA-256. The Engine captures that
descriptor once and retains the provider instance; clones retain their selection.
The digest is an operator declaration, not a binary signature or attestation.
Model output, retrieved documents and checker manifests cannot install providers.
No native callback plugin loader or new model permission is introduced. The
optional restricted Wasm loader is described below.

Each callback receives bounded original objective and candidate bytes, their
hashes, the original criterion identity and the original task deadline. One
existing authorized checker invocation covers the combined provider and compiled
check; no inference allowance is created. Required identity and dispatch audits
precede callback execution. Result audits record both acceptance decisions and
elapsed wall time. Only bounded reason codes enter provider diagnostics; raw
provider errors are omitted. Failure, an invalid claim or an unsupported check
cannot bypass the selected gate. Published-memory reuse must pass it again.

Routing, escalation and recent-advice history use a domain-separated effective
configuration fingerprint when a provider is installed. It binds the captured
descriptor as well as the ordinary configuration, so built-in-only evidence and
different declared provider versions do not match. Default fingerprints remain
unchanged. Existing comparison runners construct their own built-in-only Engines;
this extension does not turn their reports into evaluations of host callbacks.

Cancellation or the original task deadline drops the owned callback future and
retains prior charges. An interrupted callback may have only its durable intent;
missing outcome data never counts as acceptance. Async
timeouts rely on cooperative native code: they cannot preempt a blocking callback
or prove an external process stopped. Providers are trusted host code, receive no
tool or permission handle through this interface, and are not sandboxed. Arbitrary
native executable loading remains unsupported. The optional restricted Wasm path
below is separate from trusted native callbacks; binary attestation and
independently measured quality remain incomplete.

When finite-domain progress stopping is enabled, a compiled witness vetoed by
the provider consumes its original cycle as an explicitly unscored observation.
It resets plateau accumulation without marking the answer verified. Ordinary
failed-answer progress still rejects zero-violation witnesses; the combined
acceptance rule and original recursion limits remain authoritative.

Default recovery cannot reconstruct an arbitrary host callback and refuses such
tasks before new writes or dispatch. An operator embedding may supply the exact
original verifier through the explicit restoration API; descriptor, original
criterion and durable callback evidence must match. Unfinished callbacks are not
replayed. Combined restoration can also supply the original escalation policy.
No path silently resumes with only the built-in checker. Current integration
verification is recorded in `CURRENT_IMPLEMENTATION.md` and
`IMPLEMENTATION_MATRIX.md` after checks run.

The runtime can verify arithmetic and explicit structured objectives without
trusting model confidence. General prose remains unverified.

Explicit `sql:` tasks additionally execute bounded SELECT queries over inline
tables, producing hashed tool results without a model call. See `SQL.md` for the
strict limits and the distinction between query execution and factual truth.

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

Failed checks finish as `rejected`, preserving the last checked candidate even
when explanation is requested. They cannot become ordinary unverified prose via
synthesis. Both accepted and rejected candidates therefore remain bound to their
verification hashes. A denied check still fails the task; it cannot grant itself
a tool budget. Terminal rejection is represented in typed progress, may be retained
as an explicitly rejected episode, and is eligible for reviewed history retention
when no protected references exist. Rejection never qualifies as verified memory.


### Independent directed shortest-path verification

Explicit `path:` objectives declare `nodes`, directed unweighted `edges`, `start`
and `goal`. The compiled `shortest_path/v1` checker computes minimum distance
with bounded breadth-first search, then validates the candidate path. Any optimal
path is accepted; unreachable goals require `[]`, and identical endpoints require
`[start]`. Bounds: 64 unique ASCII node IDs (1–32 bytes), 256 unique non-self edges,
10,000 criterion bytes and 4,096 answer bytes. Unknown fields, missing endpoints,
invalid edges and duplicate IDs/edges fail closed before model dispatch. Cycles
are allowed; distance discovery visits each node once.

Example:

```text
path:{"nodes":["a","b","c"],"edges":[["a","b"],["b","c"],["a","c"]],"start":"a","goal":"c"}
```

The solver returns `["a","c"]`. Each check consumes a tool invocation and requires
`path.verify` for the original reasoning task's Jev+ actor. Failed checks feed the
existing bounded revision loop; checked answers retain hashes and cannot be
rewritten by optional synthesis. Criteria originate from the original objective;
model output, retrieved text and checker manifests cannot alter them or grant
permissions. Checker manifests may bind the built-in implementation as
`shortest_path/v1`; they do not install executable code. Recovery rechecks use the
same criterion and original durable tool accounting. Agent HTTP turns have no
checker grant. This verifies only the supplied finite graph, not external map
accuracy, weighted routing, real-world feasibility or general reasoning quality.

## Finite-domain assignments

Explicit `csp:` objectives ask for an integer assignment satisfying all declared
finite domains and constraints. `finite_domain/v1` checks a proposed witness;
the checker itself performs no search and executes no generated code. An optional
[bounded search](FINITE_SEARCH.md) can propose assignments before this same checker.

```text
csp:{"variables":[{"id":"a","domain":[1,2,3]},{"id":"b","domain":[1,2,3]}],"constraints":[{"kind":"all_different","variables":["a","b"]},{"kind":"linear","terms":[{"variable":"a","coefficient":1},{"variable":"b","coefficient":1}],"relation":"eq","rhs":4}]}
```

Both `{"a":1,"b":3}` and `{"a":3,"b":1}` pass. `{"a":2,"b":2}` fails the
all-different constraint. Answers must be one JSON object containing every
variable exactly once, with signed 64-bit integer values from its domain.
Extra/missing keys, duplicate JSON keys, floats, booleans and claimed verification
objects fail. Object and variable declaration order do not determine acceptance.

### Grammar and bounds

- `variables`: 1–32 records with `id` and `domain`. IDs contain 1–32 ASCII letters,
  digits or underscores; domains contain 1–64 distinct signed 64-bit integers.
- `constraints`: 1–128 records. Every constraint must pass.
- `all_different`: `variables` names 2–32 distinct declared variables whose assigned
  values must all differ.
- `linear`: `terms` contains 1–16 distinct declared variables, each with a nonzero
  integer `coefficient` between -1,000,000 and 1,000,000. `relation` is `eq`, `ne`,
  `le` or `ge`; `rhs` is a signed 64-bit integer. The weighted sum is evaluated
  exactly with 128-bit intermediates, including negative values and `i64::MIN`.
- Criteria are capped at 10,000 bytes; assignments at 8,192 bytes. Unknown fields,
  duplicate keys at any depth, unknown variables, duplicate terms and invalid
  numeric values fail closed. Work is bounded by the declared structures rather
  than the number of possible assignments.

Each check requires the original task's Jev+ `constraints.verify` grant and costs
one tool invocation. Interface, Jev-R and generation-only agent actors have no
such task grant. Rejections feed the existing capped revision loop; stagnation,
original resource ceilings, audit failure and cancellation rules are unchanged.
Accepted answers retain their exact bytes and hashes; optional synthesis cannot
rewrite them. Operator-published matching episodes are independently rechecked
before reuse. Checker manifests can bind `finite_domain/v1`, but cannot install
code, change the criterion or grant permissions.

The criterion proves satisfaction of the supplied constraints only. It does not
prove optimality, the truth/completeness of a real-world specification, general
model quality, or unsatisfiability. Contradictory criteria may be valid inputs;
no assignment or model declaration of impossibility is accepted for them. Optional
exhaustive search may establish a contradiction, but this does not satisfy the
witness contract or change benchmark acceptance.

Comparison JSONL can use `"verification":"finite_domain_v1"` with the explicit
`csp:` input and no `expected` field. Scoring independently checks each arm's
assignment, so alternative solutions are valid; failed/rejected/cancelled arms
cannot count as successes. Paired workflow/retry evidence can name
`finite_domain/v1` under the existing exact scope and operator review rules.
Synthetic tests are not a held-out effectiveness study.

### Optional constraint-violation diagnostics

The same compiled finite-domain parsing and relation checks can count violated
constraints for a complete, in-domain assignment. Malformed/incomplete/out-of-domain
answers are incomparable, not assigned a fabricated worst score. Zero violations
agrees with full satisfaction, but only the full authorized checker establishes
verified evidence. Optional [progress stopping](PROGRESS_STOPPING.md) uses nonzero
counts to decline further revisions after a bounded plateau. Counts express no
distance-to-solution or unsatisfiability proof; all original grants and tool/call
charges remain authoritative.

## Optional Wasm secondary verifier implementation

The default-off `wasm-verifier` Cargo feature adds `WasmVerifierProvider` using
exact Wasmi 2.0.0. Its source is integrated but awaits the final verification pass.
The operator supplies a bounded regular module file and exact SHA-256 pin:

```sh
cargo build --locked --features wasm-verifier
cogfabric --verifier-wasm /absolute/path/checker.wasm \
  --verifier-wasm-sha256 EXACT_LOWERCASE_SHA256 \
  ask 'sort:{"values":[3,1,2]}'
```

The module file must implement the ABI below.
The loader exposes artifact and settings hashes separately; the captured provider
descriptor binds both. The same file/settings identity must be restored for
operator continuation. Exact verifier and escalation callbacks can also be
restored together through the host continuation API.

ABI v1 exports only `memory` and
`verify(i32,i32,i32,i32,i32,i32)->i32`. Parameters are UTF-8 criterion, objective and
answer pointer/length pairs. There are no imports, WASI, host functions, filesystem,
network, secrets or tools. Return 0 vetoes; 1 passes only this secondary check;
other values are invalid. The original compiled criterion must still accept.

Bytecode, memory, table, stack, recursion, fuel and concurrent blocking jobs have
compiled ceilings. Fuel includes start-function work. The caller retains its
original deadline; timing out stops waiting, while a blocking execution keeps its
permit until it finishes. This is not instant thread preemption or proof against
an interpreter vulnerability. Models cannot install providers, change these limits
or turn a verifier claim into independent evidence.

The continuation code can also reconstruct missing publication events for a
completed parallel advisory wave when every original dispatch, response and charge
is uniquely recorded. Preview stays read-only; approved resume writes only the
missing unverified transitions atomically with admission. Unknown responses remain
denied and no model call, plan, schedule or charge is replayed or refunded.
