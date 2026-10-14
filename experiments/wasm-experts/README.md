# Import-free expert executor experiment

This separate Rust workspace tests a possible execution boundary for future
versioned domain experts. It is not part of the default CLI, HTTP server, module
registry, or model tool permissions. No active expert is installed or promoted.

The ABI is one exported evaluate(i64) -> i64 function. Inputs are binary Wasm,
at most 64 KiB. Wasmi 2.0.0 validates modules and rejects imports; no linker host
functions, WASI, filesystem, network, clock or environment interfaces are supplied.
Each call creates fresh instance state. Memory is limited to one 64 KiB memory;
tables are disallowed. Fuel is fixed at 1000 and covers start functions as well as
function calls. Stack height is capped at 64 KiB and recursion depth at 32. Float,
reference-type and tail-call proposals are disabled; SIMD and memory64 features
are not compiled. Wasmi strict compilation limits are selected.

Twenty-three tests cover execution, imports, nontermination including start functions,
initial/growing memory and tables, malformed/oversized bytes and wrong signatures.
Run cargo test --manifest-path experiments/wasm-experts/Cargo.toml --locked.
The separate workspace prevents adding this interpreter to production builds.

## Artifact and invocation binding

The public prototype API imports an Artifact using a canonical identifier,
semantic version, expected SHA256 and binary bytes. Full shape/semantic validation
finishes before import succeeds. Compiled state is immutable and shared by artifact
clones; later source-buffer mutation or replacement cannot change a granted call.
The raw execution helper is private.

TaskAuthority::operator pins an approved artifact set, one version/content per ID,
a canonical task UUID, immutable expiry and a total allocation allowance of 1–8.
Its grant method binds one exact input and task UUID to that artifact identity. The
non-serializable, non-cloneable TaskGrant permits the human-cli actor only; invoke
checks task, actor, input, ID/version/hash, then consumes the grant before guest
execution. Traps and fuel exhaustion consume it; scope denials do not dispatch.
Each successful dispatch creates fresh guest instance state, including globals.
Tests check mismatched tasks/actors/inputs/artifacts, replay, traps, version/content
replacement and state isolation.

Grants expire at the authority's original deadline, at most 30 seconds. The
embedding must use its original task deadline, not reset that allowance. No public method extends a deadline. A cloneable Revoker can only
reduce authority. Its atomic transition orders revocation against invocation
admission: queued revoked work never starts; cancellation after admission discards
the result and keeps the grant spent. Expiry is checked before admission and after
execution; late results are discarded. Final result commitment wins against later
revocation. Revocation does not forcibly interrupt the interpreter: fuel still
bounds execution, and host parsing/compilation is outside these grant checks.

The authority now requires a trusted Audit sink; there is no public unaudited
constructor. Typed events contain only task/artifact identity, allocation slot and
Allocated/DispatchReserved/Returned/NotReturned kind. Scalar inputs, outputs and
backend error bodies are omitted. Allocation is reserved before its acknowledgement;
failed/uncertain acknowledgements revoke the unreturned grant without refunds.
Dispatch acknowledgement must succeed before guest entry. Cancellation/expiry is
rechecked after that callback. Terminal acknowledgement must succeed before a
result is exposed, and failure leaves the invocation consumed. DispatchReserved
means reserved work, not proof the guest entered or returned a correct result.

The sink must acknowledge durable recording. The library does not attest that
claim, bound synchronous callback time or provide a persistent backend itself.
Tests inject failures before/after record writes, preserve spent allocation,
check metadata ordering/privacy and cancellation during acknowledgement. Comprehensive
policy-denial logging and crash reconciliation remain production integration work.

The embedding Rust caller must authenticate the actor and approve grant creation.
String comparison is not user authentication. This library has no remote/CLI
endpoint and does not deserialize grants from model output. Per-artifact issuers
are crate-private outside tests; the public authority shares one allowance across
all its approved artifacts/inputs. Allocation is conservative: unused, revoked or
trapped grants are never refunded. Revoking the authority cancels its issued
grants and prevents new issuance. It is not cloneable or serializable.

The embedding must ensure exactly one approved authority per real task: a trusted
caller can construct independent authorities, so this is not yet global task,
verified durable audit or registry enforcement. Hashes identify
content, not signatures, licensing or domain correctness. No active runtime expert
or candidate is created by this API.

## Limits of the evidence

Fuel is not a wall-clock or parsing/compilation CPU deadline. A bounded preflight
now runs before Wasmi translation: 32 flat integer function types, 64 functions,
16 integer globals, 4 exports, at most 8 parameters/1 result per internal signature,
32 local groups/128 locals per function, 8192 function-body operators and control
nesting 32. Memory must declare at most one fixed-maximum 64 KiB page; tables,
elements, imports, recursive/GC types and component encodings are rejected.
One data segment is allowed. The exported evaluate interface is checked before
start-function execution. Full Wasmi semantic validation remains mandatory.
Tests reject a 48-byte module declaring u32::MAX locals before translation, test
positive/negative shape boundaries and prove invalid typed bodies remain rejected.
These structural limits do not prove a hard host-process heap or compilation-time
quota. The parser/validator/interpreter are still trusted dependencies. Host heap consumption
and concurrency need independent bounds and measurements. Interpreter/validator
bugs remain part of the trusted computing base; this is not OS process isolation
or a proof of general sandbox safety. No independent security audit is claimed.
Execution of a scalar function is not verification of its domain correctness.

Before runtime integration, implement persistent conflict-checked artifact/version
registration, authenticated deterministic per-task/actor permission and cumulative invocation
budgets, audits before dispatch, cancellation ownership, held-out domain/security
evaluation, lifecycle/rollback, and typed protocol integration where IPC is used.
Do not enable arbitrary native executables or model-controlled host imports to
complete this experiment. Public autonomy changes require their own release review.

Sources: https://docs.rs/wasmi/2.0.0/wasmi/ and the downloaded 2.0.0 crate source.
The local toolchain is Rust/Cargo 1.97.1; Wasmi declares minimum Rust 1.86.
