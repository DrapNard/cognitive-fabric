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

Five tests cover execution, imports, nontermination including start functions,
initial/growing memory and tables, malformed/oversized bytes and wrong signatures.
Run cargo test --manifest-path experiments/wasm-experts/Cargo.toml --locked.
The separate workspace prevents adding this interpreter to production builds.

## Limits of the evidence

Fuel is not a wall-clock or parsing/compilation CPU deadline. Wasmi's strict limits
are not our complete declaration/local-count admission policy: preflight must bound
function/type/global/local/data counts before translation. Host heap consumption
and concurrency need independent bounds and measurements. Interpreter/validator
bugs remain part of the trusted computing base; this is not OS process isolation
or a proof of general sandbox safety. No independent security audit is claimed.
Execution of a scalar function is not verification of its domain correctness.

Before runtime integration, implement bounded binary preflight, immutable version
and artifact binding, deterministic per-task/actor permission and invocation
budgets, audits before dispatch, cancellation ownership, held-out domain/security
evaluation, lifecycle/rollback, and typed protocol integration where IPC is used.
Do not enable arbitrary native executables or model-controlled host imports to
complete this experiment. Public autonomy changes require their own release review.

Sources: https://docs.rs/wasmi/2.0.0/wasmi/ and the downloaded 2.0.0 crate source.
The local toolchain is Rust/Cargo 1.97.1; Wasmi declares minimum Rust 1.86.
