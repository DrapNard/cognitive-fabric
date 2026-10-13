# Expert network

The initial categories are rust.s1, linux.s1, security.s1, reverse.s1,
data.sql.s1, git.s1, unity.s1 and trainer.s1. These are routing metadata and
reference skills, not eight trained neural experts. Empty categories do not
supply domain documentation or independent verification automatically.

The SQL domain now also has a built-in bounded inline-data SELECT tool. Explicit
`sql:` objectives use deterministic authorization and fresh in-memory databases;
reference registration does not grant execution authority. See `SQL.md`. Dynamic
tool manifests and external database connections remain unsupported.

## Versioned reference manifests

`skill-register FILE.json` imports a bounded, reviewed reference manifest. It
records immutable semantic versions and a canonical content hash. New skill IDs
remain disabled until explicit `skill-activate ID VERSION`; registering a newer
version leaves the current version active. The same version with different
content is rejected. Models cannot register, activate or disable skills.

See `examples/reference_manifest.json` for the format. Required fields are id,
version, kind (reference, workflow or checker), description, tags, dependencies, source
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
The default shared loopback inference adapter serves all logical roles; reference skill
replacement is implemented independently of neural hot-swapping.

## Bounded prompt/workflow experts

Manifests also accept `kind: workflow` with a required nonempty `workflow` string
of at most 2,000 bytes. Reference manifests omit that field; omitted fields preserve
existing canonical hashes. Unknown fields and permission/executable declarations
remain rejected. `examples/workflow_manifest.json` demonstrates registration and
explicit activation through the same immutable/dependency-aware lifecycle.

In full/experts profiles, a legacy text workflow provides one separate solver advice
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
expert; optional registered model bindings are described below. It is not an
arbitrary workflow interpreter or executable plugin. Structured multi-step advice is described below; measured usefulness
still requires work.

## Dependency-ordered workflow steps

Instead of `workflow`, a manifest may provide `workflow_steps`, containing one to
four `{id, advice, depends_on}` objects. The two representations are mutually
exclusive. Advice is at most 1,000 bytes per step and 2,000 bytes total; IDs are
3–32 ASCII identifier bytes and dependencies name distinct steps in the same
manifest. Missing/self/duplicate dependencies, duplicate IDs and cycles fail.
`examples/workflow_steps_manifest.json` lists a dependent before its prerequisite
to demonstrate deterministic topological execution. Absent new fields preserve
legacy canonical hashes.

Each step makes one shared-backend solver call with 192 reserved output tokens.
Declared dependency results are copied as unverified data, bounded to 600
characters per step. Step state and result hashes are audited with pinned manifest
identity. The final solver receives the bounded workflow results; they are never
verification evidence, tool commands or permissions. An interrupted workflow does
not automatically resume or publish partial advice as a successful result.

Admission requires a nonzero cycle limit, all step calls plus two remaining calls,
`192 * steps + 640` reserved output tokens and conservative prompt-byte headroom
for dependency data and final reasoning. Each actual dispatch independently checks
the shared limits. Refusal skips the entire workflow before calls. FAST and other
ablation profiles continue to exclude workflow advice. This is a bounded advice
graph, not an interpreter for executable/conditional workflows, and does not automatically load model weights. Lifecycle replacement/disable affects new task snapshots;
in-flight manifests remain pinned.


## Registered local model bindings

A workflow may include `"model": {"id": "domain.local", "version": "1.0.0"}`.
Reference manifests cannot bind models. Register the reviewed model with
`model-register FILE.json` before activating the workflow. Activation requires an
existing enabled immutable model registration with at least 192 output tokens.
Omitting the binding preserves legacy hashes and shared-backend behavior.

The task snapshots the workflow and enabled model registration together. Bound
advice uses that registration's loopback endpoint and requested model, with parent
role overrides cleared. Each advice step reserves 192 output tokens through the
same task policy, broker, call, prompt-byte and wall budgets as ordinary reasoning.
No additional tool permissions are granted. The existing whole-workflow admission
rules still apply, and at most one workflow is selected.

Disabling a model affects new tasks, including tasks using frozen knowledge
corpora; an already admitted task retains its copied registration. Missing or
disabled bindings skip that candidate without inference and allow the next
already routed workflow to be considered. Selection preserves router relevance
order (tag match count, then ID), rather than registry iteration order. At most
one available workflow is attempted; budget refusal or provider failure does
not initiate another expert attempt. Provider failure leaves consumed call charges in
place and continues main reasoning within remaining budgets; incomplete token and
queue measurements remain unavailable. Advice never becomes verification evidence.
Health reports registration availability, not endpoint readiness or model quality.

With managed inference enabled, a bound expert owns a separate authenticated
worker using the same task identity and limits. The main and expert workers are
closed and reaped at task termination; cleanup allows up to four seconds per
child. Tests cover actual loopback endpoint selection, in-flight disable, frozen
corpus revocation, budget refusal, provider failure and two-child cleanup.

Registration hashes identify configuration, not remotely attested model weights.
Automatic backend loading/unloading, GPU residency control, neural adapter switching
and measured expert usefulness remain unverified or unsupported. Comparisons using
a separately bound model must report that model and cannot claim same-model
ablation fairness solely from matching task budgets.

## Compiled checker experts

`kind: checker` manifests bind a reviewed logical expert to either
`integer_sort/v1` or `dependency_order/v1` using the `checker` field. Register and
activate `examples/checker_manifest.json` through the ordinary skill lifecycle:

```sh
cogfabric skill-register examples/checker_manifest.json
cogfabric skill-activate data.sort.s1 1.0.0
cogfabric skill-health
```

Full/Experts execution selects a matching routed checker from the task's pinned
manifest snapshot for an explicit sort/order goal. Verification records its
manifest identity/version/checksum alongside the actual compiled criterion.
Full-profile memory checks also record that binding when present. Manifest
replacement affects later task snapshots; prior evidence retains its version.
Health reports the compiled checker identity and `model_health: not_required`.

The manifest version describes reviewed expert metadata; the checker ID separately
versions the compiled implementation. Registration cannot supply or modify code,
add checker names, bind inference models or grant permissions. Every actual check
still consumes the task-scoped fixed kernel capability and tool budget. Disabling
a checker expert removes its binding from later ordinary task discovery; it does
not disable the independent core checker or allow an invalid answer to pass.
Existing manifests omit `checker` and preserve their canonical serialization/hash.
This is reuse of compiled Rust tools, not arbitrary executable plugin loading.

Tool backend lifecycle now retains up to 128 immutable registered versions in
`fabric::tools::Broker`. Trusted host `activate(id, version)` reuses the original
backend and queue, including after unregistration; only future task pins change.
`versions()` includes inactive metadata and is not a health check. IDs cannot
change capability even after unregistration, and activation rejects conflicting
active modules. Shutdown revokes all retained pins and clears backend history.
Inactive backends/idle queues remain retained until shutdown or explicit retirement; the count ceiling
is not a byte or external-process residency quota. Process-granted backends may
already be expired or disconnected; activation does not renew grants or prove
health. No CLI/model permission or arbitrary executable loader is introduced.

The tool broker's trusted `statuses()` API reports local adapter availability for
retained arithmetic and SQL versions. Permanently discarded Unix streams remain
visible in registered metadata but cannot be newly pinned, reactivated or
queued through existing handles. An in-use connection is not classified as closed.
Connected/unknown status does not renew task grants, expiry or call allowances,
and does not attest remote service health or executable integrity. These remain
subject to the original deterministic authorization at dispatch.

Unix capability availability also reports `expired` and `exhausted` from the
original fixed session deadline and cumulative call grant. These observations
exclude adapters from new pinning, activation and pinned dispatch; they never
reset grants or deadlines. If a discarded stream is visible, `closed` takes
precedence. Observing an in-use lock cannot prove remaining calls, so dispatch
remains authoritative. This is bounded local lifecycle state, not remote health.

### Operator tool inventory

Use `cogfabric --json tool-modules` for local host tool versions, selected state,
local availability and immutable routing ranks. `tool-modules --socket` requests
the running service's typed inventory. This reports the host broker, not temporary
per-task worker sessions or remote health attestations. It starts no workers and
expands no permissions. Local CLI, typed protocol, real-socket and expanded CLI/service smoke assertions
pass. The current smoke used the debug executable; optimized installation for this
revision remains unverified.

### Operator inference inventory

`cogfabric --json inference-modules` and `inference-modules --socket` expose
retained inference adapter versions, roles, active selection, local availability
and immutable routing ranks. This complements active-only `modules` and the tool
inventory. Retained inactive versions may still serve previously pinned tasks;
observations do not renew task grants or move in-flight routes. No physical model
load/unload, executable discovery or remote health guarantee is implied.


### Retiring inactive tool backend versions

Trusted Rust hosts can call `fabric::tools::Broker::retire(id, version)` after
unregistering or replacing an arithmetic/SQL module. Active, unknown and stopped
registry selections fail. Retirement removes the version from inventory and
future rollback, but existing pins continue using their original backend and
permissions. Once the last pin and pending work release it, its queue closes and
backend handle drops. Explicit owned-process cleanup remains separate.

Immutable version identities and module-to-capability bindings remain reserved:
neither registering the same version again nor changing an arithmetic ID into SQL
(or vice versa) is permitted. Retirement does not replenish the 128 lifetime
identity slots. The bounded capability tombstone map stores metadata only, without
backend references. A new version with the same capability may be registered.
These are process-local trusted-host APIs, not model/socket commands, durable
registrations, provider unloading or general plugin isolation.


Retirement and shutdown commit registry changes before releasing backend handles
outside the registry lock. A trusted backend destructor can inspect the committed
registry; arbitrary destructor blocking/panic behavior is not isolated or bounded.


### Local queue occupancy in tool route selection

New arithmetic and SQL task pins compare approved available candidates by immutable
operator rank, occupied local queue slots, then module ID. Equal ranks can route
around a buffered queue; rank priority remains authoritative. Uninitialized queues
count as empty. This samples buffered/reserved channel slots, not running work or
remote capacity, and it reserves no slot. Concurrent changes can invalidate the
observation. Existing pins never migrate or retry on another adapter. Original
permission, call, deadline and capability checks remain unchanged. No new tool or
provider lifecycle operation is exposed.

### Replaceable executive selection

Trusted Rust hosts may install a versioned `routing::ExpertRouter` through
`Engine::with_expert_router`. Selection is restricted to at most two enabled IDs
in the task's pinned snapshot; invalid results fail before expert/model dispatch.
Default tag behavior and all existing workflow/checker/tool admission rules remain
unchanged. Callbacks run outside the database lock, and policy intent/version is
audited before invocation. ROUTING.md specifies the API, ablation exclusions and
trusted-native-code limits. This is not a model-controlled plugin loader or
proof of learned routing quality.

### Bounded retained skill inventory

Legacy `skill-add` and immutable manifest registration now share a transactional
4,096 retained skill-ID ceiling, including disabled IDs. This is separate from
the 256-active-manifest and 4,096-manifest-version limits. Existing-ID idempotent
legacy registration and new versions of an existing ID remain possible at the
ID ceiling, subject to version limits; neither path can add a 4,097th identity.
Concurrent connections serialize admission in immediate SQLite transactions.

Skill reads use a capped query and SQL byte guards before owned Rust copies:
101-byte IDs, 500-byte descriptions, 2,048-byte raw tag JSON and 64-byte optional
manifest versions. Tags are parsed strictly and validated as at most 32 unique
lowercase identifier tags of 2–40 bytes. Invalid JSON no longer becomes an empty
tag list. Enable state, nonnegative RAG version and canonical manifest version
are also checked. New legacy writes use the same metadata rules and require
nonempty tags. Valid pre-existing empty tag arrays remain readable for migration
compatibility. Invalid/oversized records fail the inventory rather than being
truncated, silently repaired or deleted. Existing invalid legacy metadata requires
explicit operator repair or restoration from reviewed valid data.

`skill-health` validates the bounded retained inventory and active dependency
graph in one read transaction, reporting `retained_skill_ids`. Invalid legacy
metadata therefore fails health even without an active versioned manifest. This
count is not a retrieval-quality, model-health or authenticity measurement.

### Manifest body and active-registry reads

Snapshot, activation and idempotent-registration reads now apply SQL byte guards
before owned copies: 64,000-byte bodies, exact 64-byte checksums, 101-byte IDs and
64-byte versions. The active graph verifies each bounded row while streaming,
rejects a 257th active record and rejects duplicate active identities instead of
silently keeping one version. Hash and embedded-versus-row identity checks still
apply. Re-registering an existing immutable version verifies its stored body as
well as comparing its checksum with the submitted content. Corruption does not
become an idempotent success or trigger implicit repair/promotion. Activation
validates its explicit logical ID and canonical semantic version before querying.
These remain local consistency checks rather than owner-resistant signatures.

Active routing description/tags are now checked against the verified manifest
body. Valid-sized projection drift fails snapshots/health without automatic
repair. Reviewed version activation updates the projection and preserves prior
task snapshots. Two regressions plus the 447-test workspace gate pass;
QUALITY_REVIEW.md records scope and actual CLI evidence. Owner-resistant
authenticity and broader independent quality evaluation remain unproven.

Explicit `skill-disable` can now exclude a damaged target while validating every
remaining active expert and reverse dependency. Only its enable flag and audit
change; corrupted body/hash data is retained. Other active corruption or required
dependents still deny the operation. A verified older version may be activated
after successful containment. This is operator-directed, not automatic repair
or a bypass for activation/promotion. Three regressions and all 450 workspace
tests pass; QUALITY_REVIEW.md records CLI evidence and remaining data-repair limits.
