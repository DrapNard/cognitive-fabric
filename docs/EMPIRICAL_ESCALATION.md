# Empirical Jev+ retry evaluation

Architecture sections 4, 5, 9 and 13 require executive decisions to account for
observed benefit and compute cost. The optional `escalation::empirical::EmpiricalPolicy`
implements a bounded empirical gate for **one revised solver after an independent
checker rejection**. It supplements the existing Router, dependency Scheduler,
Capability Manager and versioned Escalator. All seven architectural subsystems
remain separate.

This is experimental decision support. It is not a trained Jev network, a
calibrated probability, a neural safety evaluator or proof of general reasoning
improvement. No policy is automatically activated. The built-in budget-only
policy remains the compatibility default. Existing public Qwen reports are not
held-out retry studies and must not be relabeled to pass these checks.

## Evidence and scope

A `RetryStudy` has format version 1, mode (`deep` or `research`), original solver
cycle (1–15), and one compiled criterion (`integer_sort/v1`,
`dependency_order/v1` or `shortest_path/v1`). It records configuration, compiled
role-prompt and runtime-source SHA-256 values, dataset hash, source, license and
reviewer ID. These hashes are available in `escalation_intent.request` task events.

The study requires explicit operator attestations for held-out review, independent
outcome checks and inclusion of failed attempts. These are assertions by a trusted
operator, not signatures or independently verified provenance. Each observation
contains a distinct canonical task UUID, a checker-success flag, one charged model
call, 384 reserved output tokens, zero or one checker call, prompt bytes (1–32,000)
and elapsed milliseconds (0–3,600,000). A claimed success requires one checker
call. Include failed, cancelled and timed-out retries with their charged costs;
never discard them because no answer or tool result was obtained.

Studies contain 1–1,024 observations and at most 512 KiB of JSON. Parsing rejects
unknown fields, nested duplicate decoded names, invalid hashes, unsupported
shapes, repeated task IDs and incomplete review declarations. Threshold JSON is
limited to 4 KiB. Operator-selected file reads reject final symlinks and special
files. No file path, credential, prompt or answer enters the evaluation report.

The immutable `Thresholds` specify minimum observations, minimum observed success
rate in basis points (1–10,000), and maximum output reservations, prompt bytes and
milliseconds per verified success. Failed attempts contribute to every total.
Rate and cost comparisons use exact integer arithmetic without rounded-rate
admission. The report exposes counts, observed rate, total costs and maximum
observed duration. It makes no statistical confidence or population claim.

## Admission

The runtime applies its original calls, cycles, output, prompt, tool, deadline,
audit and actor-permission checks first. The empirical policy then requires:

- Exact mode, cycle, criterion, effective configuration, role prompts and source
  fingerprints matching the reviewed study.
- A single 384-token solver retry with one future checker call; unverified prose
  and critic/reflection chains cannot borrow this evidence.
- Enough observations, sufficient observed checker success and affordable cost
  per success under the immutable operator thresholds.
- Current retry prompt size no larger than the study's mean charged prompt size,
  and remaining time at least the largest observed retry duration (minimum 1 ms).

An absent/mismatched study, excessive costs or unverified objective declines extra
inference. Current state and rejection evidence are preserved. Actual dispatch
rechecks permissions and durable reservations; an allow decision is not a grant
or a prediction that the next answer will succeed. An observed maximum duration
is a heuristic, not a bound on future provider work. Original runtime timeouts
remain authoritative, and provider cancellation is not proven.

## Operator inspection and host installation

```sh
cogfabric --json retry-study-evaluate reviewed-retries.json \
  --thresholds retry-thresholds.json
```

This command performs no inference and opens/creates no configuration, database,
worker, task or model. It returns a summary, threshold values and the immutable
policy descriptor, with `installed: false` and `permissions_granted: false`.
Only `human-cli` has `evaluation.retry`; task actors receive no such grant.

Trusted Rust embeddings may install the reviewed value through the existing API:

```rust
use cogfabric::{engine::Engine, escalation::empirical::{EmpiricalPolicy, Thresholds}};
use std::sync::Arc;

fn install_reviewed_policy(
    engine: Engine,
    operator_reviewed_bytes: &[u8],
    operator_thresholds: Thresholds,
) -> anyhow::Result<Engine> {
    let policy = EmpiricalPolicy::from_json(operator_reviewed_bytes, operator_thresholds)?;
    engine.with_escalation_policy(Arc::new(policy))
}
```

Study content plus thresholds are canonically hashed. All 256 digest bits appear
across the bounded policy ID and semantic version, so changing either changes
its task snapshot identity. Handles remain immutable for Engine clones and tasks.
No serialized data installs executable policy code. Default recovery still refuses
unavailable custom policies. Explicit restoration can receive the original native
handle or reconstruct this compiled policy from its exact reviewed study and
thresholds using the CLI options documented in [ESCALATION.md](ESCALATION.md).
Revised cycles must retain their original durable allowance and matching source
fingerprint. An unavailable policy cannot silently resume with a default.
Automatic policy catalogs and promotion remain absent.

The build-time source fingerprint includes Rust sources, schemas, generated
bindings, build support, Cargo metadata and lockfile, ordered by relative path.
It excludes workspace location and documentation. It is byte identity, not
compiler/executable/loaded-weight attestation. Conservative source changes
(including unit tests in `src`) require new measurements. Operator review must
still establish appropriate task distribution, model identity and independence;
matching a criterion name does not establish domain generalization.

## Component status

| Jev+ component | Current behavior | Remaining work |
|---|---|---|
| Router | Versioned, audited skill selection with pinned snapshots and bounded routes | Cost-calibrated adaptive skill selection and independently measured routing quality |
| Scheduler | Versioned dependency ordering, whole-goal budget reservation and capacity-aware local module selection | Global fairness, provider/GPU ownership and physical memory/energy admission |
| Evaluator | Independent structured checkers; bounded retry-study aggregation and cost comparisons | Reviewed live retry studies, statistical calibration and general independent answer verification |
| Escalator | Versioned allow/decline interface; optional immutable empirical retry gate | Automatic policy catalogs, general custom reconciliation and independently demonstrated cost/quality improvement |
| Dependency Manager | Validated bounded DAGs, committed execution order and supported approved continuation | General concurrent graph execution and unsupported interruption boundaries |
| Capability Manager | Versioned declarations, pinned provider bindings and explicit gaps | Safe executable provisioning and independently established capability health |

Jev+ and Jev-R remain **Partial**. Novel recurrent neural decision/reasoning
architectures remain research beyond proven v1.0 scope. This implementation closes
a specific evaluator/escalator gap; it does not complete every architecture goal.
