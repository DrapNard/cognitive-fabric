# Expert network

The initial categories are rust.s1, linux.s1, security.s1, reverse.s1,
data.sql.s1, git.s1, unity.s1 and trainer.s1. These are routing metadata and
reference skills, not eight trained neural experts. Empty categories do not
supply domain documentation or independent verification automatically.

## Versioned reference manifests

`skill-register FILE.json` imports a bounded, reviewed reference manifest. It
records immutable semantic versions and a canonical content hash. New skill IDs
remain disabled until explicit `skill-activate ID VERSION`; registering a newer
version leaves the current version active. The same version with different
content is rejected. Models cannot register, activate or disable skills.

See `examples/reference_manifest.json` for the format. Required fields are id,
version, kind (reference or workflow), description, tags, dependencies, source
and license. A dependency has id and a semantic version requirement such as
`^1.0`. Unknown fields are rejected, including permissions and executable paths.
Manifests do not grant tool permissions, execute code or load neural weights.

Activation validates all enabled dependencies, reverse version constraints and
cycles in the complete active graph inside an immediate transaction. Missing or
incompatible dependencies leave the prior state intact. Activate an older
immutable version to roll back metadata, subject to the same dependency checks.
`skill-disable ID` is refused when an enabled dependent needs that skill.
Disabled skills are excluded from routing and new retrieval. Existing task
snapshots remain available to those tasks.

Use `skill-health` to validate active graph integrity and inspect stored document
counts. Counts are informational; they do not prove retrieval quality or model
health. Capacity limits are 256 active manifests and 4,096 stored versions.
Bootstrap legacy categories remain compatible with the earlier skill-add command;
version-constrained dependencies require explicitly registered active manifests.

## Task isolation and pinning

Task context reads active manifests, routing metadata and bounded retrieved
references in one SQLite read transaction. The engine copies this snapshot before
model calls and logs version metadata and document checksums. Replacement,
disabling and RAG rollback affect later tasks. Retrieved body hashes and active
manifest hashes are validated. This protects against accidental tampering;
a compromised same-user process can rewrite both data and hashes.

Neural adapters, inference-model loading/unloading, external executable experts,
sandboxed code execution and backend weight attestations remain unsupported.
The shared loopback inference adapter serves all logical roles; reference skill
replacement is implemented independently of neural hot-swapping.

## Bounded prompt/workflow experts

Manifests also accept `kind: workflow` with a required nonempty `workflow` string
of at most 2,000 bytes. Reference manifests omit that field; omitted fields preserve
existing canonical hashes. Unknown fields and permission/executable declarations
remain rejected. `examples/workflow_manifest.json` demonstrates registration and
explicit activation through the same immutable/dependency-aware lifecycle.

In full/experts profiles, a routed workflow can provide one separate solver advice
call before whole-goal reasoning. At most one workflow is selected deterministically
from the task's pinned active manifest snapshot. FAST and other comparison profiles
do not invoke it. The original objective remains authoritative; workflow content
and output are advisory data and confer no permissions or verification. The call
uses the shared broker/backbone, role settings and task policy, with a 192-output-token
allocation charged to the existing budgets. No model is loaded automatically.

Admission requires a nonzero cycle budget, at least three remaining model calls,
832 remaining reserved output tokens, and conservative prompt-byte headroom. This
reserves allocations for the planner and whole-goal solver. The route, version,
canonical manifest checksum and admission decision are audited. These deterministic
cost checks are not a measured expected-utility estimator. Expert advice remains
unverified and cannot complete a task on its own.

Replacing/disabling manifests affects subsequent snapshots, while copied workflow
text stays pinned for in-flight work. This is a single model-assisted checklist
expert, not an arbitrary workflow interpreter, executable plugin or independent
model backend. Multi-step workflows and measured usefulness still require work.
