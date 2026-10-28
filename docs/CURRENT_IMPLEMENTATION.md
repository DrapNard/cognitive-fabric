# Current Rust implementation

## Project individual expert step records — 2026-10-28

**Implemented; runtime unverified.** Overlap strategies now select whole
validated step records individually, retaining expert and step identities and
recording omissions against the source digest. Unrecognized or legacy advice
remains an opaque entry. Limits, trust and permissions are unchanged. See
[cognitive network](COGNITIVE_NETWORK.md). No tests, benchmarks, live inference
or recovery execution ran.

## Preserve whole overlap advice entries — 2026-10-28

**Implemented; runtime unverified.** Planner/expert overlap and continuation
now project complete expert entries into the existing strategy limit, recording
omissions and a full-source digest rather than cutting an entry. Earlier long
strategy can be omitted with a digest. Exploration preserves the projection
when its addition would exceed the limit. See
[cognitive network](COGNITIVE_NETWORK.md). No tests, benchmarks, live inference
or recovery execution ran.

## Structure multi-expert workflow handoffs — 2026-10-28

**Implemented; runtime unverified.** Structured engine multi-expert steps now
exchange bounded advisory objects. Version-2 planner/expert overlap records
support reconstruction of those objects; the recovery reader retains the
version-1 prose rules. Legacy workflows and raw audit text remain unchanged.
Existing prose-based fixtures require updates. See
[cognitive network](COGNITIVE_NETWORK.md). No tests, benchmarks, live inference
or recovery execution ran.

## Structure single-engine-expert workflow handoffs — 2026-10-28

**Implemented; runtime unverified.** Sequential and parallel steps in the
single-expert engine workflow now exchange strictly parsed advisory records of
up to 600 bytes. Malformed or prose-only model output fails that workflow.
Multi-expert batch and recovery formats and raw inference audit events retain
their existing representations. Existing prose-based fixtures need updating.
See [cognitive network](COGNITIVE_NETWORK.md). No tests, benchmarks or live
inference ran.

## Prepare admitted engine routes concurrently — 2026-10-28

**Implemented; runtime unverified.** Engine reasoning and expert waves prepare
their admitted routes concurrently and settle started preparations before
propagating an error. Per-route resource admission and native activation locks
remain in force; contention can still reject a wave and provider effects can
remain unknown after cancellation. See [cognitive network](COGNITIVE_NETWORK.md).
No tests, benchmarks, model loads or live inference ran.

## Describe live reusable computations — 2026-10-28

**Implemented for client expert workflows; runtime unverified.** Later expert
steps can receive bounded, untrusted descriptions of whole eligible in-flight
batches and request an exact match. If descriptions do not fit input admission,
they are omitted before reservation. Existing matching, policy and task limits
remain authoritative. See [cognitive network](COGNITIVE_NETWORK.md). No tests,
benchmarks or live inference ran; throughput remains unmeasured.

## Bind opening interpretation to task memory — 2026-10-28

**Implemented for new client collective tasks; runtime unverified.** The opening
language interpretation now enters the root- and reference-bound structured
reasoning-memory envelope before expert or reasoning handoffs. Existing limits
and advisory trust apply; legacy continuations and other memory paths are not
migrated. See [cognitive network](COGNITIVE_NETWORK.md). No tests, benchmarks or
live inference ran.

## Reuse live work with exhausted tool slots — 2026-10-28

**Implemented; runtime unverified.** Later expert steps can receive the
computation catalog while this workflow owns a live coalescible batch, even
without free tool slots. Exact matching and existing policy and budget checks
still apply; stale persisted reservations do not enable this path. See
[cognitive network](COGNITIVE_NETWORK.md). No tests, benchmarks or live
inference ran; throughput and general heterogeneous orchestration remain unproven.


## Share exact expert core computations — 2026-10-28

**Implemented task-local exact batch reuse and coalescing; runtime unverified.**
`core_reuse.rs`, `collective/experts.rs` and `sessions.rs` share checked results
or a single in-flight source for eligible independent batches while preserving
paid caller receipts, per-operation policy, root binding and existing budgets.
Waiting expert steps cannot release their dependents before checked publication.
`expert_advice.rs` writes v4 references and preserves v1–v3 read rules. Typed
operation graphs and partial overlaps are not coalesced; compiled checks do not
establish the intended meaning of model-selected arguments. See
[cognitive network](COGNITIVE_NETWORK.md) for details and limitations.
No tests, benchmarks or live inference ran. General heterogeneous workflows,
broader structured memory and evaluated project LoRA remain incomplete or
unverified.

## Reserve concurrent core-memory bytes — 2026-10-28

**Implemented known-byte reservation; runtime unverified.** `sessions.rs` reserves
projected serialized metadata across pending core batches, using validated sizes
from `core_tools.rs`. Actual results must leave other batches' holds intact.
Fingerprint/byte ledgers share canonical call keys and are released atomically;
orphaned or unresolved holds refuse checkpoint publication. Existing core/session
byte ceilings remain unchanged.

Unknown result growth can still cause a charged refusal; there is no automatic
retry, refund, truncation or weight promotion. Legacy empty ledgers remain valid;
older unresolved holds without byte costs fail closed. See
[cognitive network](COGNITIVE_NETWORK.md) for accounting and remaining gaps.
Static compilation, formatting, strict Clippy and whitespace checks plus source
review are the validation scope. No tests, benchmarks or live inference ran.
General orchestration, request coalescing and evaluated project LoRA remain Partial.

## Bridge expert-model steps into the typed core — 2026-10-28

**Implemented bounded expert → typed → dependent-step dataflow; general workflows
remain Partial and runtime unverified.** `expert_compute.rs` parses optional
explicit computation envelopes, while `collective/experts.rs` schedules owned
core work beside models and releases dependencies only after checked admission.
The exact prompt catalog, paid model receipts, original policies, shared slots,
held verification budget and pending fingerprints govern execution.

`sessions.rs` pins call namespaces, reserves pending work durably and refuses
checkpoint publication while any batch is unresolved. `expert_advice.rs` writes
v3 checked-core references and validates them against task memory; v1/v2 reads
remain supported. Advice, summary, tool and memory caps are unchanged. See
[cognitive network](COGNITIVE_NETWORK.md) for protocol limits and remaining gaps.
Static formatting, locked workspace/all-target compilation, strict Clippy and
whitespace checks plus source review are the validation scope. No tests,
benchmarks or live inference ran. Arbitrary mixed graphs, unknown-outcome recovery
and evaluated project LoRA remain incomplete or unverified.

## Keep structured reasoning-role memory — 2026-10-28

**Implemented for client collective handoffs; broader memory remains Partial.**
`reasoning_memory.rs` binds planner/explorer/reflector records to the original
request and references within the existing 2 KiB advice cap. `collective.rs`
replaces prose merges, checks joint capacity before optional role calls, requires
complete structured responses and preserves typed checker feedback. `sessions.rs`
validates marked memory on continuation/checkpoint publication. No truncation,
role-record eviction, quota increase or weight promotion occurs.

Legacy prose is explicitly hashed/omitted when merged into new memory. See
[cognitive network](COGNITIVE_NETWORK.md) for limits and compatibility boundaries.
Validation is static formatting, locked workspace/all-target compilation, strict
Clippy and whitespace checks plus source review. No tests, benchmarks or live
inference ran. General mixed workflows, other legacy memory paths and evaluated
project LoRA remain incomplete or unverified.

## Store structured expert conclusions — 2026-10-28

**Implemented for new expert step summaries; network-wide memory remains Partial.**
`advisory_record.rs` validates bounded claims, suggested actions and uncertainties.
`collective/experts.rs` requires complete structured model responses and performs
no prose truncation. `expert_advice.rs` writes v2 objects while reading v1 legacy
strings as untrusted data. Original root/reference hashes, budgets and caps
remain; no claims become verified facts, permissions or neural weights.

Planner/explorer/reflector and broader session advice are still separate
free-form paths. See [cognitive network](COGNITIVE_NETWORK.md) for schema limits
and failure handling. Static formatting, locked workspace/all-target compilation,
strict Clippy and whitespace checks are the validation scope. No tests,
benchmarks or live inference ran. Runtime compatibility, full mixed workflows
and evaluated project LoRA remain incomplete or unverified.

## Share execution capacity across typed and model experts — 2026-10-28

**Implemented for mixed phases; runtime unverified.** `src/agent/network_slots.rs`
replaces the fixed typed/model capacity split with shared, deadline-bound owned
execution slots. `sessions.rs`, `collective.rs` and `collective/experts.rs`
retain lower configured group limits and acquire permits inside owned futures.
Planner/explorer pairs reserve two slots atomically; preparation, inference and
typed checking remain within the shared local ceiling. Budgets, permissions,
evidence gates and failure accounting are preserved. Unknown provider activity
after a timeout is outside this local semaphore guarantee.

See [cognitive network](COGNITIVE_NETWORK.md) for boundaries and remaining gaps.
Formatting, locked workspace/all-target compilation, strict Clippy and whitespace
checks passed; source review found no concrete budget/permission bypass. No tests,
benchmarks or live calls ran. Throughput and reasoning quality remain unverified;
general heterogeneous dataflow and evaluated project LoRA remain Partial.

## Schedule dependent core computations in one solver response — 2026-10-28

**Implemented bounded core DAG proposals; runtime/provider behavior unverified.**
Solver-only host functions now accept `_cgf_after` (up to three sibling call IDs)
and `_cgf_inputs` whose source is exactly one prior checked `operation_id` or a
sibling `call_id`. A sibling result binding automatically creates a dependency.
Cycles, self references, unknown calls, duplicate ordering edges, invalid source
fields and overlapping decoded target paths refuse the batch before tool charges.
The existing four-operation and eight-bindings-per-operation limits remain.

`src/agent/core_batch.rs` maps proposal IDs to host-generated operation IDs,
resolves prior-memory inputs, and constructs a strict typed graph. It validates
the combined target set before splitting memory and sibling sources. Required
argument placeholders must be valid for their typed operation; SQL query-text
binding and arithmetic expression interpolation remain forbidden. Numeric mode
replaces only a whole expression with one checked number token.

`typed_experts/plans.rs::from_reasoning_graph` reuses the existing bounded graph
parser, rejects model-selected experts/provider selectors and binds every node to
the original root digest. Independent batches retain their simpler existing path.
Graph templates distinguish reasoning-selected candidates from language
translation; neither proves fidelity to the original goal.

`sessions.rs` jointly binds the entire frozen provider graph before individual
policy/operation/verifier reservations. The solver catalog can now advertise
dependency-backed provider capabilities; advertisement alone never makes a
standalone invocation valid. Missing prerequisites refuse whole-plan admission.
The existing rolling checked-evidence scheduler runs independent ready nodes
within the original width and releases dependents as prerequisites complete.
No intermediate model turn is needed for sibling value transfer. Failures stop
new admissions and drain already-admitted owned work under existing cleanup rules.

Provenance sizing uses disposable fixed-length hash markers only. Admission
records distinguish actual prior checked evidence from declared sibling bindings;
they never publish sizing markers as checked results. After the entire batch
succeeds, completion re-resolves each operation from original memory plus checked
sibling outputs, compares the final criterion, and atomically stores actual
source/selection hashes. Existing memory/checkpoint/provenance caps may still
refuse oversized results after charged work; no truncation, retry or refund is
introduced. Current task deadlines and cumulative model/token/tool budgets remain.

This closes same-response typed-core dependencies, not arbitrary neural/typed
workflow graphs, interrupted-provider recovery or evaluated project LoRA.
Formatting, locked workspace/all-target compilation, strict Clippy and whitespace
checks are the validation scope. No tests, benchmarks, live inference, model
activation, installation or recovery execution were run. Speed and reasoning
quality remain unmeasured.

## Reuse checked core results through structured references — 2026-10-28

**Implemented bounded checked-memory inputs; runtime/provider behavior unverified.**
Every solver-only `cgf_core_*` function now accepts an optional `_cgf_inputs`
array of one to eight bindings. Each binding names an earlier finalized core
operation, an optional source JSON pointer, an existing argument target pointer,
and `value` (default) or `numeric_expression` mode. Required argument placeholders
remain present; references cannot add fields, replace the root, overlap targets
or select capabilities/providers. Same-batch, future and missing operation IDs
are unavailable. Qwen remains outside this computation/data-transfer path.

`src/agent/core_memory_bindings.rs` resolves these references from a validated,
frozen copy of the task's checked-result memory before provider selection or
operation charging. Pointer bytes/depth, unique source bytes, copied values,
resolved arguments and provenance are bounded. SQL query-text targets are
forbidden. Numeric-expression mode accepts only a whole plain JSON-number token
and replaces the entire math expression; it does not evaluate model-selected
templates or concatenate executable text. Ordinary value bindings remain data
and must pass the destination's existing typed Work parser.

`core_tools.rs::Memory::outputs` exposes exact finalized JSON result strings after
checkpoint validation. `sessions.rs` retains original canonical proposal arguments
and fingerprints, reserves each destination's ordinary operation/verifier slots,
and records source-result/selected-value hashes and pointer provenance alongside
the provider identity. Known provenance/checkpoint sizes are preflighted before
work. Completion re-resolves against the same pre-batch memory and requires the
checked criterion to match before atomic publication. Existing 2 KiB provider
provenance, 12 KiB memory and 32 KiB checkpoint caps can refuse an otherwise valid
large binding set; these caps are not increased or silently truncated.

This works with restored pinned routes after validated client handoffs. It does
not replay earlier work or raise quotas. It reduces model-mediated value copying;
there is no measured speed or quality claim. References cover finalized core
working memory, not arbitrary retrieved documents, unfinished graphs or a new
permission source. General mixed neural/typed DAGs, independently proven reasoning
quality and evaluated project LoRA remain incomplete or unverified.

Validation is limited to formatting, locked workspace/all-target compilation,
strict Clippy and whitespace checks. No tests, benchmarks, live provider calls,
model activation, installation or recovery execution were run.

## Preserve typed core routes across client handoffs — 2026-10-28

**Implemented bounded exact-route restoration; runtime behavior unverified.**
The client collective can now restore its original typed core routes after a
validated client-tool handoff, allowing the reasoning solver to request further
host computation without a new language-interpretation call. Original model,
token, tool, cycle and same-boot wall budgets remain cumulative. Legacy sessions
without frozen route metadata retain their data-only continuation behavior.

`src/agent/typed_snapshot_storage.rs` writes immutable, task-bound, digest-linked
event chunks in the opening transaction. The checkpoint stores only their SHA-256.
The canonical envelope is capped at 2,500,000 bytes (including escaped inner
metadata); chunks are capped at 8 KiB and restoration has bounded row counts and
a one-second SQLite progress deadline. Module metadata remains capped at 64 KiB,
provider declarations at 1 MiB/256 entries. No workflow bodies, model weights or
client history enter this route snapshot. Hex encoding adds storage overhead;
the existing storage admission margin and database limits still apply.

`typed_experts.rs` restores the original module descriptors, registration limits,
source fingerprints, implementation identities and compact provider declarations.
`typed_experts/providers.rs` rebuilds semantic-version caches and exact bindings
from these frozen routes without reading current manifests. This does not grant
permissions or attest trusted executable behavior. Unknown versions, changed
metadata, incomplete chunks and mismatched digests refuse restoration.

`fabric/modules.rs::fork_retained_owned` validates the complete frozen module set
against trusted retained implementations before binding. Non-native restoration
reuses the original broker's retained pins and shared queues. Native restoration
uses the current exclusive owner and fresh task-bound adapters, never the prior
request's ownership guard. The opening broker UUID remains historical provenance;
the live execution broker may have a different UUID. Exact implementation metadata,
not UUID equality, governs reconstruction. The runtime retains access to the
unowned source registry across its per-request native fork.

`sessions.rs` restores only after checking original configuration, history,
pending opaque handles and the call ledger. It clears reconciled pending handles
only in the local working copy and preserves the stored checksum for the next
compare-and-swap. No prior operation, uncertain result or model call is replayed;
new core requests still pass existing individual policy and budget reservations.
Checked-result memory remains structured, bounded and untrusted as instructions.

Validation: formatting, locked workspace/all-target compilation, strict Clippy
and whitespace checks only. No tests, benchmarks, live inference, model activation,
installation or recovery execution. General-purpose deterministic reasoning,
measured speed/quality and evaluated project LoRA remain incomplete or unverified.

## Reasoning-model requests to the pinned typed core — 2026-10-28

**Implemented bounded solver/core feedback; runtime/provider compatibility
experimental and unverified.** With explicit language-only network routing and
a host-attached typed broker, the opening solver phase can receive fixed
`cgf_core_*` function schemas for available math, inline SQL, sorting, ordering,
path and finite-domain computation. Only independently executable pinned provider
routes are advertised; graph-dependent providers require their existing graph
path. Schemas are host metadata, not capability grants. Qwen interface/presentation
and optional advisory experts receive no host function definitions.

`src/agent/core_tools.rs` parses at most four host proposals per response through
strict JSON and rejects mixed host/client batches, collisions with client tool
names and duplicate call identities. Named/required/disabled client tool choices
do not enable the host catalog. Client-only proposals retain the existing handoff
path. The reasoning adapter must support ordinary function-call proposals; this
change does not relax the intentionally single-target exact-token adapter.

`Plan::from_reasoning_arguments` validates fixed-capability arguments with existing
Work parsers and marks their broader meaning unverified. `sessions.rs` retains
an Arc of the task-opening typed snapshot, binds the entire batch before charges,
then individually authorizes and reserves operation and verifier intent slots
(two per operation) before polling. All calls retain original task/store/config,
wall, queue and width limits; no registry discovery or module/model selection is
accepted from function arguments. Execution reports distinguish reasoning-model
arguments from language translation and never promote a candidate computation
into proof of the original goal.

`collective.rs` intercepts host calls before client handoff, executes the admitted
typed batch, then resumes the same solver cycle using structured checked-result
memory. Model calls, output reservations and operation slots never reset. Host
functions are hidden when continuation/presentation call and minimum output
budgets are unavailable. Canonical fingerprints reject repeated computations.
Task memory is capped at 16 records/12,000 bytes, binds the original root, stores
exact result JSON strings plus argument/criterion/provider digests, and labels
model-selected semantics false. Known argument/metadata/checkpoint sizes are
preflighted before computation using a disposable sizing value that is never
published as a result. Actual feedback is input-admitted before memory publication.
Unknown result growth may still cause bounded terminal refusal after work was
charged; no quota is raised, data truncated, operation retried or charge refunded.
The unchanged combined checkpoint cap remains 32 KiB.

Only complete checked batches enter memory atomically with admission metadata.
Client continuations restore data without replaying computations; executable pins
are not recaptured on continuation, so new host-core requests remain opening-task
functionality. Original client messages/tools and kernel checks remain authoritative.
Standalone graph/provider failure, invalid data and unknown outcomes fail explicitly.
General mixed dataflow, interrupted-provider recovery and optional evaluated LoRA
remain Partial. Static formatting, locked workspace/all-target compilation, strict
Clippy and whitespace checks are the validation scope. No tests, benchmarks, live
provider calls, model activation, system installation or recovery execution run.

## Project checked subvalues into expert inputs — 2026-10-28

**Implemented bounded typed-value projection; runtime/translation semantics
unverified.** `expert_typed_inputs` retains whole-result strings and now accepts
selectors such as `{"operation_id":"totals","pointer":"/0/total"}`. For a
checked result shaped like `[{"total":5}]`, that pointer names the raw value `5`.
This illustrates pointer semantics, not an assumed SQL/provider output schema.
An expert capability can declare up to eight selectors, including different
pointers into one operation; duplicate operation/pointer pairs are rejected.
Empty lists remain independent and unspecified capabilities retain conservative
all-operation dependencies. Existing evidence/graph/reasoning-scope restrictions
remain unchanged.

`src/agent/typed_input.rs` validates the selector schema and graph-compatible IDs.
`translation.rs` checks every source against the same translated graph. Shared
helpers in `typed_experts.rs` and `typed_experts/graphs.rs` reuse bounded RFC 6901
parsing (256 bytes/eight segments), strict JSON validation, canonical array indices
and raw-value traversal. There is no filesystem or provider access in selection.
Missing paths, malformed escaping, scalar traversal and invalid array indices
fail explicitly rather than returning invented or silently missing values.

`src/agent/typed_progress.rs` projects only the requested subvalues into each
expert prompt, preserving raw JSON number spelling. Every projection retains
its source operation, pointer, selected-value digest, full-source digest and
criterion/checker/provider/query provenance. Selection does not independently
verify the broader meaning of the task. Gates in `collective/experts.rs` continue
to wait on real checked operation IDs, regardless of how many pointers use one
source. The actual projected turn is budget checked before its reservation.
The unchanged 32-KiB context cap can reject overlapping/large selections without
truncating them; final typed results and final-solver dependencies remain whole.

This reduces source-level context duplication when only a subvalue is needed;
no latency or quality improvement is measured. Model advice still cannot become
checked-only typed input or grant a capability. General mixed dataflow, interrupted
provider recovery and optional evaluated project LoRA remain incomplete. Static
formatting, locked workspace/all-target compilation, strict Clippy and whitespace
checks are the validation scope. No tests, benchmarks, live calls or recovery run.

## Live checked-input dependencies for model experts — 2026-10-28

**Implemented typed-to-model capability dependencies; general bidirectional mixed
dataflow and interrupted-provider recovery remain Partial; runtime unverified.**
Evidence-mode `typed_graph` translation accepts optional `expert_typed_inputs`,
a map from an additional reasoning capability to the typed operation IDs that
its model workflow needs. Keys must belong to the validated reasoning scope;
IDs must exist in the same graph and lists must be unique/bounded. An empty list
permits independent advice; unspecified capabilities and dependency-only expert
plans conservatively wait for all typed operations. The map is incompatible with
`expert_dependency: "independent"`, selects no implementation or permission, and
remains semantically unverified translation guidance.

`src/agent/typed_progress.rs` and `src/agent/sessions.rs` publish cumulative
host-owned checked results through a bounded latest-value Tokio watch channel.
Only successful operation checks and result audits release IDs. Failure is
published while admitted typed siblings drain; completion follows final assembly.
The channel cannot be populated by deserializing model output. Raw JSON numeric
spelling is retained, with surrounding value whitespace removed. Each operation
stays within its existing 16-KiB result bound; provider projections are bounded
at 2 KiB and per-expert progress context at 32 KiB. The watch stores one latest
snapshot, not an unbounded queue. Partial snapshots are never durable final proofs.

`src/agent/collective/experts.rs` wakes on both model completions and typed progress.
A workflow can begin once its declared checked inputs exist while unrelated typed
and model work continues. Each expert receives only its declared input values;
original messages and constraints remain. `src/agent/graph_schedule.rs` represents
external prerequisite blocking separately from in-flight work, so waiting nodes
consume no model slots. Actual running work still counts against group/per-group/
total limits, including the model partition beside typed execution.

Every newly ready call is rebuilt with actual input and policy/resource/request/
budget checked before reservation or backend preparation. Opening hypothetical
preflight is only an estimate for future dynamic data; a later oversized input
can be refused after earlier calls have been charged. Such failures stop refills
and drain admitted work without replay or refunds. Closed/failed progress fails
explicitly rather than spinning. The root joins both phases, discards temporary
progress context, admits the complete original typed evidence, and only then
continues the final solver/presentation path. Low-width execution remains ordered.
Model advice cannot feed checked-only typed edges or become a permission grant.

Files also include `src/agent/collective.rs`, `translation.rs`, and
`graph_translation.rs`. Static formatting, locked workspace/all-target compilation,
strict Clippy and whitespace checks are the validation scope. No tests, benchmarks,
live model calls, activation or recovery execution; no speedup or quality claim.

## Structured expert-advice working memory — 2026-10-28

**Implemented bounded advisory checkpointing; runtime/recovery unverified.**
`src/agent/expert_advice.rs` stores completed workflow summaries in a strict
versioned record with expert and step IDs, versions, dependencies where present,
partial-output markers and fixed untrusted-advice labels. The record binds its
canonical summary digest to the original query and frozen reference snapshot.
Single/multi-expert shapes reject unknown fields; summaries stay within the
existing 1,200-byte bound and downstream envelopes within 2,048 bytes.

`src/agent/sessions.rs` publishes the record and digest-only admission audit in
one task-claim/checksum transaction. Waiting client-tool checkpoints retain it;
terminal checkpoints clear it with their reference snapshot. Restore validates
root/reference/content identities and one bounded task publication record under
a one-second SQLite scan limit. It does not discover experts, call models, repeat
charges, grant capabilities or convert advice into verified facts. Legacy snapshots
without the optional field retain their original behavior.

`src/agent/collective.rs` supplies this separate structured record to reasoning
roles after workflow completion and on continuation. Planner/reflector changes
to short working advice no longer silently discard the expert results. Language
roles retain their existing reference-body boundary and original client messages.
`src/agent/collective/experts.rs` includes projected envelope metadata and bounded
working advice in solver input preflight before expert costs are incurred.
The unchanged 32-KiB combined checkpoint cap can still refuse large combinations
of typed evidence, references and advisory records; it is never raised or silently
truncated. Existing task input/call/token budgets continue to govern actual turns.

This is temporary task working memory, separate from curated immutable structured
project memory. Model advice is not automatically published as project truth,
used to train an adapter or promoted to neural weights. Optional project LoRA
remains unfinished and independently gated. General heterogeneous dataflow and
interrupted-provider recovery remain Partial. Static formatting, locked workspace/
all-target compilation, strict Clippy and whitespace checks are the validation
scope. No tests, benchmarks, live inference, activation or recovery execution.

## Rolling model-expert dependency execution — 2026-10-28

**Implemented completion-driven expert scheduling; runtime/performance unverified.**
`src/agent/collective/experts.rs` now refills expert capacity after each completed
result set instead of waiting for unrelated calls in the previous wave. Within-
expert step edges and cross-expert manifest prerequisites retain their original
meaning. In-flight coordinates count against shared group/per-group/global limits,
including the reduced model partition during concurrent typed work.

All requests and exact role routes in a launch batch are validated before its
first durable call reservation. Lifecycle preparation and inference then live in
locally owned futures that are polled together; preparing a new route cannot
suspend polling of calls holding admission permits. Per-call preparation and
generation retain their configured timeouts and original task deadline. Temporary
resource unavailability defers refill while active work can free capacity; an
unserviceable idle graph fails explicitly rather than spinning or changing limits.

An overlapping planner/explorer pair occupies two slots until both settle. Both
role results retain completion accounting even when a sibling fails. Later Jev-R
uses the recorded pair without duplicate inference. Per-step completion/failure
audits replace the whole-wave completion event; final workflow summaries retain
their bounded structured advice, trust labels and launch-batch counts. Model
outputs remain advisory and cannot satisfy checked-evidence dependencies.

Reservation, preparation, provider and audit failures stop new launches while
already-admitted owned work drains. Partial reservation failures remain charged
and never dispatch or replay their incomplete batch. Parent cancellation and
unknown physical provider outcomes retain existing cleanup limitations. Original
policy, presentation/solver budget preflight, typed-verifier slot holds and frozen
model selection remain. This removes model-wave barriers, not provider queueing
or native lifecycle serialization. General per-node heterogeneous dataflow and
interrupted-provider recovery remain Partial. Static formatting, locked workspace/
all-target compilation, strict Clippy and whitespace checks are the validation
scope. No tests, benchmarks, live model calls, activation or recovery run.

## Rolling typed dependency execution — 2026-10-27

**Implemented rolling typed scheduling; runtime/performance unverified.**
`src/agent/sessions.rs::PreparedTypedWork::execute` now polls a bounded set of
locally owned operations and refills available slots after each completion set.
A dependent operation can start after its own prerequisites pass compiled checks,
while unrelated slower operations remain in flight. It no longer waits for the
entire previous ready wave. All currently ready completions are processed before
new launches, so an observed sibling failure prevents further scheduling.

`src/agent/graph_schedule.rs` represents in-flight nodes explicitly and counts
them against total, group and per-group caps. Running work cannot be selected
again and never satisfies a checked dependency. `src/agent/collective/experts.rs`
keeps its existing wave behavior with no in-flight nodes in its snapshots.
Typed readiness snapshots include pending, running and checked-complete IDs;
checked outputs alone feed argument bindings. Original provider proofs, source
freezes, charge-before-dispatch accounting and final declaration-order assembly
remain. Bounds still come from the original admitted typed width, including any
reduction for concurrent model work.

Binding, provider, audit and clock errors are latched: new launches stop and every
already-admitted in-flight future settles before the first error propagates.
No operation is retried or refunded. Parent cancellation still follows existing
owned cleanup; a timeout does not independently prove physical provider stop.
This removes a source-level wave barrier, not provider queueing, worker limits or
lifecycle serialization. Model-expert workflows still use bounded waves, and
arbitrary heterogeneous per-node dataflow/interrupted-provider recovery remain
Partial. Static formatting, locked workspace/all-target compilation, strict Clippy
and whitespace checks are the validation scope. No tests, benchmarks, live calls,
model activation or recovery execution were performed; no speedup is claimed.

## Overlap independent model-expert workflows with typed work — 2026-10-27

**Implemented bounded independent workflow overlap; arbitrary mixed DAG Partial,
runtime unverified.** Evidence-mode interpretation accepts optional
`expert_dependency: "typed_results" | "independent"`, defaulting to typed results.
It applies only to optional model-expert advisory workflows. An independent
workflow must use original request/context inputs rather than the pending typed
outputs; the primary Jev-R solver still waits for both paths. This declaration
is advisory translation, not independent proof of semantic independence.

`src/agent/collective.rs` can join typed execution with the selected model-expert
workflow when at least two subtask slots and a nonempty additional-expert scope
are available. It partitions configured width between typed and model work, never
increases admitted typed width, and reuses the completed expert result afterward.
No duplicate workflow is attempted when the independent attempt returns no plan.
Serial/low-width and dependent requests retain their ordered path.

`src/agent/collective/experts.rs` clamps every expert ready wave and its optional
advisory overlap to the model partition. Pending typed-verification slots remain
held during expert-workflow tool admission. Each model wave retains its original
policy, route selection, durable call reservation and provider resource checks;
typed invocations are charged before the joined phase. Both futures settle before
failure propagates, without sibling-error cancellation, replay or refunds. Parent
request cancellation can still interrupt the joined futures under existing cleanup.

`src/agent/sessions.rs` admits evidence against the exact bounded count of already
charged independent model calls, preserving the original task counters.

After both paths finish, actual checked-evidence input is re-admitted before later
reasoning, and model advice retains its untrusted status. Qwen remains language
interface/presentation under explicit network routing. No provider concurrency or
latency improvement is established: queues and lifecycle locks may serialize work.
This supports concurrent typed and model workflows with a declared phase edge;
arbitrary per-node heterogeneous dataflow and interrupted-provider recovery remain
incomplete. Static formatting, locked workspace/all-target compilation, strict
Clippy and whitespace checks are the validation scope. No tests, benchmarks, live
model calls, recovery execution or model activation performed.

## Scope additional expert activation after typed coverage — 2026-10-27

**Implemented bounded routing partition; translation semantics/runtime unverified.**
Evidence-mode translation accepts optional `reasoning_capabilities` for additional
model-expert routing. By default this is the full required capability set minus
the typed computation's capabilities. An explicit list may overlap typed coverage
when additional reasoning is needed in the same domain. It must be unique, contain
at most 16 existing requested capabilities, and together with typed coverage cover
the entire request. The field is refused outside evidence usage and cannot name
models, implementations, providers or permissions.

`src/agent/typed_experts/translation.rs` validates and derives this partition.
`src/agent/collective.rs` uses a routing-only clone of the interpretation for
opening-catalog expert selection; `src/agent/collective/experts.rs` uses the same
scope when binding workflows. An explicitly empty scope skips optional model
experts instead of invoking the legacy unqualified fallback. Versions are retained
for included capabilities. The full original interpretation, constraints, request
and checked evidence remain available to primary Jev-R reasoning. Scope decisions
are recorded with hashes and an explicit unverified-semantics label.

This avoids scheduling optional model experts merely to repeat planned typed
coverage, without claiming translation proves the broader capability is solved.
Explicit overlap allows additional same-capability reasoning. Existing expert
dependencies, budgets, policies, concurrency limits and provider pins remain.
Client continuations retain frozen expert context and restored checked evidence;
this one-use selection scope does not trigger new expert discovery on continuation.
General mixed-node execution and interrupted-provider recovery remain incomplete.
Static formatting, locked workspace/all-target compilation, strict Clippy and
whitespace checks are the validation scope. No tests, benchmarks or live calls.

## Mixed capability coverage and relevant expert activation — 2026-10-27

**Implemented capability routing scope; general heterogeneous DAG Partial, runtime
unverified.** With `typed_usage: "evidence"`, the language interface may translate
a complete intermediate goal/batch/graph whose non-empty capability set is a
subset of the full request. The complete original interpretation and remaining
reasoning capability versions are retained for Jev+ expert selection and the
Jev-R workflow. Every typed capability must still be declared in that full set;
unknown selectors, clarifications and typed version requests remain refused.
Default/final usage still requires exact complete capability coverage.

For example, an interpretation may request `math.evaluate` plus a published
domain-reasoning capability, compute the fully specified arithmetic as typed
evidence, and select a compatible pinned domain workflow for the remaining
reasoning. This is a routing contract, not a benchmark or a guarantee that the
domain expert is installed, competent or independently verified.

`src/agent/typed_experts/translation.rs` and `graph_translation.rs` enforce the
usage-specific scope; `src/agent/sessions.rs` advertises matching instructions.
`src/agent/expert_workflow.rs` no longer activates an arbitrary dependency-free
workflow when explicit capability requirements have no compatible binding. The
legacy unqualified route remains only for interpretations without capability
requirements. Explicitly unmatched work stays with the primary reasoning path;
capability declarations never grant permissions or prove capability coverage.

This supersedes the exact-set restriction for intermediate evidence only.
Existing bounded overlap, checked-evidence restoration, task charges, deadlines
and language-model boundaries remain. Arbitrary mixed-node interleaving and
interrupted provider recovery remain incomplete. Static formatting, locked
workspace/all-target compilation, strict Clippy and whitespace checks are the
validation scope. No tests, benchmarks or live provider/model calls performed.

## Restore finalized checked evidence for client continuations — 2026-10-27

**Implemented bounded evidence restoration; interrupted provider execution and
general mixed DAG recovery remain Partial; runtime unverified.**
`src/agent/sessions.rs` now publishes the structured checked-result envelope and
its version-2 admission audit in the same task-claim/checksum transaction after all
per-operation verification charges. A combined-checkpoint size preflight precedes
those charges; neither the 16,384-byte envelope nor the 32,768-byte session limit
is increased. Partial charges retain only a blocking marker.

`src/agent/typed_evidence.rs` rejects duplicate/unknown fields, malformed identities,
unknown capabilities, duplicate operation IDs, invalid provider projections and
mismatched root/envelope/result digests. Raw values retain their exact JSON number
spelling; result hashes cover the value excluding surrounding JSON framing
whitespace. Original provider-output digests remain in the execution audits.
Results retain their original untrusted-data status; a digest is an integrity link,
not an independent signature or a fresh proof of computation.

`src/agent/collective.rs` restores validated evidence when continuing the original
client task, including tool-result continuations. The original message-history
binding, same-boot clock, calls, tool charges and root policy still apply. The
restoration query requires one task-specific finalized admission record, has a
bounded payload/row count and a one-second SQLite progress limit. Restoration
records a digest-only audit and never resolves providers, executes computations,
recharges checks, grants capabilities or refreshes task limits. Reasoning receives
the envelope again; language roles retain their existing reference-body boundary.

Legacy hash-only or partial admission, missing/ambiguous audit records and changed
evidence remain refused. This supersedes the blanket refusal of all finalized
typed-evidence continuations, not the refusal to replay interrupted execution.
The final natural-language answer remains semantically unverified. No automatic
model training or raw prose memory is added; the saved object is a bounded result
schema. Static formatting, locked workspace/all-target compilation, strict Clippy
and whitespace checks are the validation scope. No tests, benchmarks, live
inference, recovery execution, model activation or installation performed.

## Overlap typed work with independent Jev-R advice — 2026-10-27

**Implemented bounded cross-kind overlap; general mixed DAG and recovery Partial,
runtime unverified.** For translated intermediate typed evidence in RESEARCH mode,
`src/agent/collective.rs` may run typed computation alongside the independent
planner/explorer pair when configured subtask width is at least three. Both model
calls and every typed invocation are reserved before either path runs. Model route
preparation completes before inference; typed concurrency is reduced to the
remaining width (total minus two), never increased above its original admission.
Original task deadlines, policy, provider checks and cumulative budgets remain.

Both paths settle before failures propagate, and model completion accounting runs
even if typed work fails. The recorded pair is reused by the later Jev-R phase;
`src/agent/collective/experts.rs` refuses to schedule another pair while it is held.
Reasoning in the overlapping pair receives only original interpretation/context,
not unavailable typed outputs. Expert steps and the final solver receive checked
evidence afterward, with full actual-input admission.

`src/agent/sessions.rs` caps the owned typed executor's width and explicitly
accounts for zero or two already-reserved advisory calls when admitting evidence.
The optional expert overlap preview now returns an empty wave for zero remaining
slots, preserving serial/low-width behavior after shared scheduler integration.
This corrects a source-level zero-capacity scheduling regression.

Provider queues or lifecycle locks may still serialize work; no physical resource
isolation or speedup is established. Only this explicitly independent pair
overlaps typed work; arbitrary interleaved heterogeneous graphs and evidence
restoration remain incomplete. Static formatting, locked workspace/all-target
compilation, strict Clippy and whitespace checks are the validation scope.
No tests, benchmarks or live model/provider calls were run.

## Checked typed evidence in the reasoning workflow — 2026-10-27

**Implemented one-way typed-to-reasoning flow; continuation and unified DAG
Partial, runtime unverified.** The language envelope accepts `typed_usage: "evidence"`
with one complete typed goal, batch or graph. CGF executes it through the opening
provider snapshot, admits each operation's compiled-check result under the existing
per-operation verifier tool charge, and sends a bounded structured envelope to
reasoning roles. Raw JSON values are serialized without converting through `Value`,
so numeric spellings remain intact. Evidence includes operation and provider
identity, criterion and result digests, and an explicit untrusted-data label.

Jev-R/advisory model roles receive this evidence with their assigned input. The
language interface keeps its existing boundary. Successful operation checks do
not establish translation meaning or verify the model's final answer; the ordinary
conversation checker cannot promote a candidate to verified on this path. The
original call, token, input, wall and tool budgets apply, including final language
presentation reservation.

The opening checkpoint stores only the evidence digest. If a response hands off
to client tools, continuation currently fails closed because the checked envelope
is not restored; typed work is not silently replayed. A combined typed/model DAG
and durable evidence restoration remain incomplete. Static formatting, locked
workspace/all-target compilation, strict Clippy and whitespace checks only; no
tests, benchmarks or live inference.

## Checked typed evidence before model reasoning — 2026-10-27

**Implemented staged typed-to-reasoning bridge; general mixed DAG Partial, runtime
unverified.** The client interpretation envelope accepts optional
`typed_usage: "evidence"` alongside one existing typed goal/batch/graph. The default
remains `"final"`. Evidence usage requires complete, unambiguous intermediate
computations with all input data; it cannot select a model, provider or permission.
The stable public Interpretation schema is unchanged.

`src/agent/collective.rs` executes the admitted typed phase, then supplies checked
operation results to expert/model reasoning under the same task. Expert selection
uses the original interpretation. Typed nodes retain their bounded parallel waves;
model experts and eligible Jev-R advisory roles retain theirs. The bridge is an
explicit phase dependency: reasoning starts after typed checks. It does not yet
interleave independent model and typed nodes in one heterogeneous ready wave.

`src/agent/sessions.rs::admit_typed_evidence` validates opening generation, root
request, phase counters, remaining original clock, policy and held verifier slots.
It serializes a structured envelope capped at 16,384 bytes with operation IDs,
raw JSON values, criterion/result hashes, checker and provider identities. Raw
values serialize directly without a floating-point conversion; reasoning receives
the envelope as labeled data. Each operation consumes its existing verification
slot. Actual role requests, including evidence, still pass original cumulative
input/token/call admission; oversized evidence fails explicitly without truncation.
Qwen receives interpretation/presentation work, not these additional reasoning
reference bodies. Original client messages remain preserved.

Operation checks do not verify translation semantics, the complete natural goal,
or the final model answer. Mixed-path positive generic checks cannot promote its
final answer to independently verified. A durable hash marker is recorded with
the first evidence-check charge; repeated admission and unsupported subsequent
continuation are refused rather than rerunning providers or silently losing
evidence. Checked-envelope recovery and multi-turn client-tool continuation for
this new mixed path remain incomplete. Existing final-only routes retain their
prior behavior.

Files: `src/agent/typed_experts/translation.rs`, `src/agent/sessions.rs`,
`src/agent/collective.rs`. Static formatting, locked workspace/all-target
compilation, strict Clippy and whitespace checks are the validation scope. No
tests, benchmarks, live inference, installation or model activation performed;
no latency or quality claim.

## Shared dependency readiness with explicit evidence kinds — 2026-10-27

**Implemented shared scheduling foundation; heterogeneous execution Partial.**
`src/agent/graph_schedule.rs` owns bounded host-only node/edge readiness with
`Advisory` and `Checked` completion kinds. Checked edges refuse advisory results;
advisory edges accept either kind without granting permissions. Node indexes are
local to the validated host-built graph, not model-selected global identifiers.
The primitive validates references, duplicate edges and cycles, caps graphs at
16 nodes and ready waves at four, and preserves round-robin group/per-group caps.
A node selected in the current wave never satisfies another pending node's edge.

`src/agent/sessions.rs` uses it for typed dependency waves with checked-only
edges and stable operation order. `src/agent/collective/experts.rs` maps model
workflow steps and cross-expert prerequisites into the same primitive, preserving
the requirement that every prerequisite expert step finish before dependent work.
Routes, budgets, policy checks, dataflow resolution and output checks remain owned
by their existing executors. Completed typed nodes enter only after compiled
checks; successful model steps enter only as advisory. This shares readiness, not
verification claims.

A unified external mixed-plan envelope, cross-kind evidence bindings, combined
route admission and interrupted recovery remain incomplete. Existing typed and
model executors still dispatch separately. No tests, benchmarks or live model calls
were run; static formatting, workspace/all-target compilation, strict Clippy and
whitespace checks are the validation scope. No speedup or quality claim.

## Concurrent preparation across expert runtimes — 2026-10-27

**Implemented bounded expert scheduling; runtime unverified.**
`src/agent/collective/experts.rs::reserve_expert_wave` now prepares all selected
expert runtimes concurrently after the complete wave's durable call reservations.
The wave remains capped at four calls. Exact model routing, each expert's runtime,
resource admission and original task deadline are retained. All preparation
outcomes settle before error propagation or route assignment; any failure prevents
that wave's generation, without retries or call refunds. Native lifecycle locks
still serialize loads, and provider queues may serialize inference.

The model-advisory and typed-provider graphs remain separate schedulers. A unified
mixed DAG still requires host-owned node/result envelopes, frozen model/provider
routes and explicit cross-kind evidence dependencies; concurrent preparation alone
does not implement that bridge. Qwen remains restricted to language roles when
the explicit language profile is configured.

Static formatting, locked workspace/all-target compilation, strict Clippy and
whitespace checks passed. No tests, benchmarks or live model calls; no measured
speedup or reasoning-quality claim.

## Concurrent client route preparation — 2026-10-27

**Implemented scheduling mechanics; runtime and speed unverified.** Already
reserved client role waves now poll preparation concurrently for up to four pinned
routes (`src/agent/mod.rs`). Each preparation retains task authorization, resource
admission and the original deadline. All started preparations settle before any
error propagates; a failed wave starts no generation and is never retried here.
The owned-future join accepts borrowed routes (`src/reasoning/parallel.rs`), without
detached tasks. Native lifecycle mutation locks still serialize model loads;
backend queue limits may serialize other work. Cancellation can leave provider
outcomes unknown, as before. This does not add implicit model activation rights.

Validation uses formatting, locked workspace/all-target compilation, strict Clippy
and whitespace checks only. No tests, benchmarks or live provider calls. Mixed
neural/provider graphs and interrupted graph restoration remain Partial.

## Frozen typed provider prerequisites — 2026-10-27

**Implemented bounded routing mechanics; mixed neural/provider execution and
interrupted restoration Partial; runtime unverified.** Typed graph routes jointly
select opening-catalog providers, matching each manifest prerequisite against a
direct predecessor's selected expert ID/version. A 4,096-probe bounded search can
backtrack earlier selections. Immutable host-only route proofs precede dispatch;
checked predecessor outputs gate dependent calls and independent ready work retains
bounded concurrency. No prerequisite arguments or edges are invented.

Files: `src/agent/typed_experts/providers.rs`, `plans.rs`, `graphs.rs`,
`src/agent/typed_experts.rs`, `src/agent/sessions.rs`. Provider/result identities
are retained in bounded audits; existing task charges and deadlines remain.
This supersedes earlier blanket refusal of manifest dependencies, not the refusal
of simultaneous model/provider bindings. See
[provider prerequisites](COGNITIVE_NETWORK.md#frozen-typed-provider-prerequisites).
Validation: formatting, locked workspace/all-target compilation, strict Clippy and
diff whitespace checks. No tests, benchmarks or live inference run; no speed or
reasoning-quality claims.

## Natural graph translation and exact client call reduction — 2026-10-27

The client language interface can emit a strict optional `typed_graph` for bounded
dependent work, without implementation/version selectors. It is mutually exclusive
with other typed extensions and reuses the existing graph validator/scheduler.
Original natural-request digests and translation origin survive every resolved
node and final check; translated tasks remain semantically `unverified`.
Eligible exact typed inputs now skip redundant model interpretation and use only
one final presentation call; translated work still uses interpretation plus
presentation. Counter/charge guards distinguish those paths and typed phases use
the configured slots without a held language slot. This supersedes the former
exact-input language/expert overlap and natural-graph absence. Root budgets,
provider pins, source freezes and native ownership stay enforced. Mixed neural/
provider work, manifest dependencies and interrupted restoration remain incomplete.
No tests, benchmarks or live inference were run. See
[language-call scope](COGNITIVE_NETWORK.md#natural-language-graph-translation-and-language-call-scope).


## Exact typed dependency graphs and checked dataflow — 2026-10-27

`typed-graph:` now expresses up to four fixed-capability operations with validated
DAG dependencies and bounded JSON-pointer data bindings. Entire-plan validation and
invocation charges precede owned dependency-ready waves. Sources must pass compiled
checks before values enter dependent arguments; the resolved typed request is
validated again under the same pinned provider, capability, clock and budget.
SQL query bindings and arbitrary math expression strings are refused. Audits link
upstream result/criterion hashes to each resolved criterion. Consumed source values
are frozen exactly in final presentation, preventing inconsistent witness changes.
Declared operation order is restored for final checks even when execution followed
a different topological order. This supersedes the preceding absence of all typed
dataflow. Natural-language graph translation, manifest dependency/model execution,
interrupted recovery and measured performance remain incomplete. No tests,
benchmarks or live providers were run. See
[typed dependency graphs](COGNITIVE_NETWORK.md#typed-dependency-graphs-and-checked-data-bindings).


## Frozen manifest-bound client typed providers — 2026-10-27

Client typed sessions capture active provider declarations with opening exact
module pins, validate full descriptor/capability correspondence and resolve
without live registry reads. Automatic stable selection respects covering
bindings; declared but unavailable providers cannot silently become generic host
routes. Exact batch operations may choose a published expert version or capability
version requirement; model-translated arguments cannot choose implementations.
Bound calls reserve their exact retained module and audit the manifest/module/
capability/request linkage before broker intent, preserving existing task grants,
budgets, waves and final checks. This supersedes the preceding note that all client
manifest binding is absent. Provider dependency/model combinations, ranking quality
and interrupted typed recovery remain incomplete. No tests, benchmarks or live
provider operations were run. See
[manifest-bound typed providers](COGNITIVE_NETWORK.md#manifest-bound-typed-providers).


## Independent composite typed client requests — 2026-10-27

Exact `typed-batch:` requests and client-only `typed_batch` translations can group
one to four independent typed operations. Entire-plan route/budget validation and
all invocation charges precede dispatch. Owned waves share the opening snapshot,
original clock and typed deadline; total/expert concurrency limits apply, including
one held language slot during exact-input overlap. Failed waves settle all started
operations and stop before later waves. Final JSON keys and every per-operation
value are checked under per-operation charges. Exact batches preserve criterion
verification; translated batches remain semantically unverified. This supersedes
the earlier single-operation-only boundary. Typed dependencies/output substitution,
manifest-backed selection and interrupted typed restoration remain incomplete.
No tests, benchmarks or live inference were run. See
[independent composite requests](COGNITIVE_NETWORK.md#independent-composite-typed-requests).


## Client-only natural-language typed translation — 2026-10-27

The client language interface can emit one optional bounded `typed_goal` for an
opening host-pinned capability. Its strict wrapper preserves the existing public
Interpretation/Engine shape and legacy client replies. Original exact typed goals
cannot be rewritten; requested external tools, missing route/budget and ambiguous
translation retain their respective safeguards. Natural typed execution follows
interpretation, validates the translated arguments, uses the original budgets and
checks its final numeric/JSON output. Original-source and translated-criterion
hashes remain separate throughout dispatch/result/presentation audit. Such tasks
remain semantically `unverified`; a correct translated computation does not prove
fidelity to the original request. This supersedes the preceding note that all
natural-language typed translation is absent. Composite work, manifest bindings,
interrupted typed restoration and measured translation quality remain incomplete.
No tests, benchmarks or live provider calls were run. See
[natural-language typed translation](COGNITIVE_NETWORK.md#natural-language-typed-translation).


## Typed client capability routes and independent opening wave — 2026-10-27

The host-attached client typed route now covers exact arithmetic, task-local SQL,
sorting, dependency ordering, shortest-path and finite-domain assignment requests.
Inputs and checkers derive from the original goal. With at least two configured
slots, language interpretation overlaps the single typed operation after both
budgets commit. Qwen final output follows a strict numeric/JSON contract and is
checked against the retained result/original criterion in a charged final check.
Other model-backed reasoning roles are skipped for eligible typed inputs. Native
ownership and all original deadlines/limits remain. Worker semantics now agree
with the criterion on unreachable paths and checked i128 finite linear sums.
This supersedes the preceding arithmetic-only scope and unchecked-presentation
limitations. General natural-language typed translation, manifest-backed client
provider selection, composite requests and interrupted typed restoration remain
incomplete. No tests, benchmarks or live inference were run. See
[typed client experts](COGNITIVE_NETWORK.md#first-turn-typed-client-experts).


## First-turn typed client arithmetic — 2026-10-27

The explicit language-only client collective can attach a trusted same-Store
module broker and pin its opening routes. Exact original arithmetic goals use
interpretation, one charged typed provider invocation, then presentation, without
model-backed reasoning calls. The internal value must match the compiled parser;
presented wording remains unverified. The server connects the reviewed worker via
its existing worker flag or the approved native policy setting, with separate
native client/Engine registries and task-owned module binding. No model is loaded
by this addition. Other typed capabilities, natural-language input translation,
manifest-backed client provider selection and interrupted catalog recovery remain
incomplete. No tests, benchmarks or live inference were run. See
[the cognitive network guide](COGNITIVE_NETWORK.md#first-turn-typed-client-experts)
and the latest implementation-matrix entry for scope and static check results.



## Task-pinned alternate typed expert providers — 2026-10-24

**Implemented for trusted typed modules; broader adaptive scheduling remains
Partial.** Architecture sections 4, 6, 7 and 9: `src/fabric/modules.rs` permits
multiple active identities per capability and atomically captures a task's ordered
candidate sets. Engine snapshot v5 records exactly those pins. Queue reservations
precede dispatch intent; full/closed candidates may be skipped only before intent.
Reservations bind their original deadline and, for candidate routing, capability.
After intent, errors never trigger an alternate-provider retry. Exact-pin dispatch
retains deadline-bounded queue waiting and ordinary authorization remains required.

`src/recovery/continuation_modules.rs` and dispatch-history validation restore
original v5 identities, including retained closed candidates without treating them
as healthy; missing/retired/mismatched identities still refuse. Versions 1–4 retain
single-provider rules. Explicit expert bindings remain exact; verifier diagnostics
record the selected identity without replacing compiled acceptance criteria.
This supersedes the earlier single-provider limitation for trusted typed modules.

Fifty-one focused broker, recovery, registry, engine, CLI worker and Wasm checks
pass, including full-queue selection, atomic snapshot retention, no retry, grant
limits, cancellation and v4/v5 compatibility. Default/all-feature all-target build
checks, strict all-feature Clippy and formatting pass. The full all-target test
suite and reflection/live-model campaigns were not run. Selection probes at most
32 candidates, adds no model call or permission, and is not calibrated quality
ranking, executable isolation or automatic model residency. See
[typed provider selection](TYPED_PROVIDER_SELECTION.md).


## Per-call provider timing inspection — 2026-10-24

**Implemented; 16 focused unit/CLI checks passed.** Architecture sections 4, 7 and 9:
`src/recovery/inference_metrics.rs` links optional reported metrics to validated
call reservations and preceding reasoning hashes in the public operator audit.
Missing reports remain null; coverage is explicit and timings/rates are not summed.
`src/recovery/inference.rs` uses one read transaction and a shared five-second
cooperative deadline, clearing SQLite's progress handler after errors and success.
`src/main.rs` runs `inference-audit` against an existing read-only current-schema
Store before bootstrap, migration or engine construction. Recovery admission still
uses the original ledger validator, independent of advisory metrics.

No call is retried, refunded, verified or authorized by inspection. Physical
measurement attribution, provider accuracy and calibrated scheduling remain Partial.
See [provider metrics](PROVIDER_METRICS.md) for field coverage and boundaries.







## Gap-bound reviewed source catalogs — 2026-10-24

**Implemented catalog discovery; open-web discovery remains Partial.** Architecture
sections 4 and 8: `src/evolution/catalog.rs` validates a hash-pinned reviewed catalog,
selects exact-capability source declarations by stable priority/ID, deduplicates
content hashes and writes an unapproved acquisition plan. Initial/publication
checks bind the exact recorded gap and refuse available capabilities so existing
skills can be reused. Completed/failed gap tasks remain eligible.

The human-only `candidate discover-remote-sources` CLI uses an existing read-only
Store, no bootstrap or network. Strict catalog/history parsing, bounded source
counts, metadata bytes and SQLite inspection deadlines govern the operation.
`src/gaps.rs` rejects duplicate JSON keys and clears its deadline callback on
both successful and failed inspection. Selection creates no candidate, model
permission or evaluation result. Source priorities/review labels are assertions,
not measured usefulness, legal assurance or independent curation. See
[source catalogs](SOURCE_CATALOG.md). Twenty-six focused checks pass across catalog,
gap inspection and the existing acquisition/preparation/import pipeline. Formatting,
all-feature compilation and strict Clippy pass. Full reflection tests were not run.


## Acquisition lineage through candidate registration — 2026-10-24

**Implemented.** Architecture sections 7, 8 and 10: acquisition drafts bind exact
plan/report hashes; `src/evolution/acquisition/lineage.rs` strictly validates
bounded offline origin metadata without network access. Preparation reads fixed
metadata files through its retained directory descriptor, matches every selected
source to its acquired ID/path/hash/labels/bytes, and emits report version 3 with
exact acquisition snapshots. Unbound local reports retain version 2.

`src/evolution/prepared.rs` validates report-version/evidence consistency and source
correspondence during import and durable inspection; `src/store.rs` invokes that
same validator at transactional candidate registration. Existing audit failure
rolls back registration and its anchor. Complete reports remain capped at 65,536
bytes; metadata-heavy bundles can be refused before publication. This preserves
origin metadata, not held-out document text, across source-bundle removal.

Focused acquisition/pipeline, local preparation/import and persistence checks pass;
formatting, all-feature compilation and strict Clippy pass. The pipeline regression
also removes original/prepared bundles before reading durable provenance. The first
v3 import exposed a stale v2-only Store gate, fixed through the shared validator.
No full reflection suite was run.

No new tool/network permission, evaluation authority, neural promotion or model
call is introduced. Hash-bound local records are not proof of remote contact or
independent licensing/quality review against a hostile owner. See
[source acquisition](SOURCE_ACQUISITION.md) and [preparation](SOURCE_PREPARATION.md).


## Operator-selected remote source acquisition — 2026-10-24

**Implemented bounded acquisition; remote discovery and independent curation
remain Partial.** Architecture section 8: `src/evolution/acquisition.rs` fetches
approved hash-pinned HTTPS text into a private unreviewed source bundle. Entire
manifest preflight, public-only DNS validation/pinning, no proxies/redirects/auth,
strict framing, exact body hashes and document/aggregate/deadline limits govern
network work. At most four independent resolver threads retain permits through
libc completion; timeouts cannot trigger a later HTTP request. The original plan,
reviewer assertion, hashes and labels remain in private provenance files.

The human-only `candidate fetch-sources` command runs before runtime initialization.
Generated preparation drafts have no review or held-out assertions. Existing
preparation, quarantine, independent evaluation, promotion and rollback gates
remain required. No model network capability or new active weights are introduced.
Focused URL/DNS/framing/pin, private HTTP, bundle/preparation and CLI-refusal
checks cover supported mechanics; public HTTPS acquisition and independent
licensing/poisoning resistance remain unverified. Each source uses a separate
bounded resolution/client; this adds network/TLS work only when the operator
invokes acquisition. See [source acquisition](SOURCE_ACQUISITION.md).


## Runtime WAL growth boundary — 2026-10-24

**Implemented for runtime file connections.** Architecture sections 7, 9 and 10:
`src/storage/wal_vfs.rs` registers an explicitly selected, non-default SQLite VFS.
WAL writes/truncations beyond 16,000,000 bytes fail before the delegated file
operation. Preallocation hints cannot bypass the check, and WAL mmap fetch is
disabled. Store creation/continuation/inspection/migrations, token ledgers and
backup/restore staging select it. `storage-status` reports the actual connection's
marker and byte ceiling; in-memory/unwrapped connections report no hard limit.

Pinned-reader tests with two runtime writers stop at `SQLITE_FULL`, retain all
committed rows and resume after checkpointing. An external oversized WAL remains
readable and unchanged by a denied write, then recovers through checkpointing.
The adapter adds a file-size observation per WAL write and can reject large single
transactions. No task result is manufactured on storage failure. This bounds
cooperating connections' WAL growth, not total disk allocation, external writers,
SHM, backups or arbitrary artifacts. Workspace-wide physical quotas remain Partial.
The human-only `storage-checkpoint` CLI uses a short lock wait and reports busy
readers; inspection/maintenance avoid bootstrap writes. Focused storage, callback,
legacy migration, backup/restore, token-ledger/managed-worker and CLI checks pass,
with default/all-feature compilation and strict all-feature Clippy. The full
reflection suite was not run. See [storage limits](STORAGE.md) for exact commands.


## Token preparation requires a live task — 2026-10-24

**Implemented.** Architecture sections 7, 9 and 10: `src/token_admission.rs`
checks the task status through a bounded read-only database connection before
rendering or tokenization can expose a prompt to the provider. Missing and
terminal identities fail without native HTTP requests. The independent immediate
write transaction still rechecks status before reserving tokens and dispatching
generation; a task stopped during preparation cannot obtain that reservation.
Two new regressions in `tests/input_token_admission.rs` cover both boundaries.
This adds one local database read per admitted attempt, grants no permissions and
makes no claim that cancellation stops already-started remote preparation.
The WAL hard-size bound remains an open storage requirement.


## Exact wire-input token admission — 2026-10-24

**Implemented for the optional llama.cpp native adapter; general provider support
remains Partial.** `src/token_admission.rs` renders complete text history, counts
and submits identical token IDs, and durably charges a cumulative task allowance
before generation. Context admission includes the reserved output. Concurrent
reservations, malformed ledger history and audit failures fail closed; failures
never refund input. `Config.input_token_admission`, the human-only CLI setting,
typed bootstrap v3, backend capability gates and managed credential forwarding
preserve the limits across supported dispatch paths. Default behavior is unchanged.

Five loopback/managed-worker integration tests pass. They cover exact IDs, context
refusal, provider failure without refund, concurrent admission, audit-write failure,
chat history, early unsupported requests and dedicated credential separation.
See [input token admission](INPUT_TOKEN_ADMISSION.md) for commands and restrictions.
Tools, role profiles, non-none provider effort and native LM Studio lifecycle are
unsupported in this mode. Real llama.cpp inference, physical GPU/energy enforcement
and reasoning quality remain unverified. No reflection campaign was run.


## Kernel bounds for managed workers — 2026-10-24

**Implemented; focused resource-boundary verification passed.** Architecture
sections 4, 9 and 10 now have optional Linux per-process virtual address-space and
CPU-time limits for managed native chat, inference, arithmetic, SQL and compiled
typed workers. `src/process_limits.rs` restricts only the child before exec,
preserves stricter inherited limits, disables child core dumps and refuses setup
failure. `Config.worker_process_limits` defaults to None; the human-only
`configure-worker-limits` command validates and saves it. Existing services require
restart, and existing tasks preserve their admitted settings.

Typed module identities and reconstruction retain exact original limits. New
launch/ownership audits report requested bounds without claiming physical usage
measurement. See [worker limits](WORKER_LIMITS.md). This does not constrain an
external model provider, GPU, total task CPU, RSS, filesystem or network, and is
not a full sandbox. The logical scheduler's allocation declarations remain distinct.

Four subprocess regressions pass (one ignored fixture is explicitly invoked by
them), along with two configuration/permission tests, one original-limit recovery
test, seven bounded native-worker tests and the executable-upgrade worker test.
A temporary CLI workflow verifies typed arithmetic under limits and clearing the
setting. Default and all-feature workspace all-target checks and strict Clippy pass.
The NVML feature check also exposed and fixed an unnecessary return. No reflection, live reasoning-quality or
performance benchmark was run for this milestone.


## Same-model per-turn reasoning effort — 2026-10-24

**Implemented and focused verification passed.** Native `cogfabric-agent` requests
accept `reasoning_effort` (`none`, `low`, `medium`, `high`). `Turn` validates and
pins it; `protocol/agent.fbs` appends a typed enum with an unspecified legacy
default. Direct and managed adapters forward explicit effort over the saved
configuration default while retaining the same model and token/call/time ceilings.
Invalid values fail before inference; provider rejection does not trigger a model
switch. Collective aliases retain their separate single-goal contract.

Chat exposes `--reasoning-effort`, `COGFABRIC_CHAT_REASONING_EFFORT`, and interactive
`/effort` display/change/default commands. `/reset` preserves the selected effort
and spent budgets. OpenCode variants on the single `cogfabric-agent` model use
`settings.reasoningEffort`; the launcher accepts configured variant suffixes.
The existing local OpenCode config was extended without changing permissions.
See [chat](CHAT.md), [API](HTTP_API.md) and [OpenCode](OPENCODE.md).

Verification: pinned FlatBuffers 25.12.19 regeneration, workspace all-target check,
strict workspace all-target Clippy, formatting, five protocol tests, two provider
fixture tests, eleven HTTP tests, seven managed worker tests and the explicit
installed OpenCode high-variant read/edit/read regression pass. The latter checks
that tool turns carry `high` while the same API model remains selected. A local
chat command-only session verified environment selection, runtime changes, reset
preservation and return to the server default. No new live-model reasoning-quality
comparison was run; effort effectiveness depends on the configured provider/model.


## Configurable chat frontend — 2026-10-24

**Implemented; production compilation and strict chat-binary Clippy pass.** Chat model, endpoint, credential
variable name, timeout, temperature, turn/token/session budgets and input/history/
response capacities accept CLI flags and `COGFABRIC_CHAT_*` environment variables.
Flags take precedence; existing defaults remain. `src/chat_client.rs` exposes
validated `Options`/`Limits` while retaining the default constructor. The frontend
uses those same capacities for bounded input, history trimming and response reads;
its messages display the selected limits. Checked total token reservation, finite
outer bounds, loopback validation and tool refusal remain enforced independently
of the model. No server budget is raised by client settings. See [chat](CHAT.md).
No live chat or new test suite was run for this configuration-only milestone.


## OpenCode startup and real-client coverage — 2026-10-24

**Implemented; compatibility verified within the stated scope.** The local
provider configuration incorrectly used a credential value in OpenCode's `env`
list; the local file now references `COGFABRIC_SERVER_API_KEY`. Local OpenCode
config files are ignored by Git and `opencode.example.json` contains no secrets.
After reboot, neither API service was running. LM Studio was restarted on
loopback only; the helper starts/reuses an authenticated Cognitive Fabric API.

`scripts/opencode-local.sh` (installed as `cogfabric-opencode`) validates the
provider, saved backend/ceilings and upstream discovery, creates an ephemeral key
for its own service, passes the key to OpenCode and cleans up only that owned
service. Configurations and model weights are not automatically rewritten.
Private logs support diagnosis. See [OpenCode startup](OPENCODE.md).

The installed OpenCode **v2.0.32** completed **read → edit → read** with the real
local Qwen3-8B model in the isolated permission-limited smoke fixture
(`/tmp/cogfabric-opencode-smoke.GUS5Vt`). The new explicit opt-in regression
`crates/cogfabric-server/tests/opencode_client.rs` also passes against the real
OpenCode executable with a deterministic in-process backend. All **10 HTTP
compatibility regressions** pass. This proves the tested tool/stream/history path,
not general model quality or compatibility with every agent client.

A normal repository session additionally exposed a provider-context mismatch:
Qwen was actually loaded with 4096 context while OpenCode declared 32768. Model
advertisement alone is insufficient. The launcher now checks provider-reported
loaded context when native inventory is available and refuses a smaller value.
Unknown native inventory is reported explicitly for other providers. After explicit
operator approval, the same Qwen model was reloaded with 32768 context, one parallel
slot and partial GPU offload (automatic placement failed). The normal repository
OpenCode session then returned READY and successfully executed a Cargo.toml read
and answered with the correct package name. Ten HTTP tests and strict server
Clippy pass; GPU-option map simplifications fix warnings exposed by that check.
No general release-readiness or performance claim follows.


## Source setup and standalone chat — 2026-10-24

**Implemented; live installation/chat verification deferred.** `scripts/setup.sh`
builds the main CLI, `cogfabric-chat` and HTTP server from a selected source ref
with locked dependencies, no sudo and an explicit replacement flag. It preserves
runtime state and supports optional NVML observation. README and
[installation guide](INSTALL.md) document curl download, review and revision
pinning. This is a source installer, not a published binary release.

`src/bin/cogfabric-chat.rs` and `src/chat_client.rs` implement bounded interactive
multi-turn text chat through the authenticated loopback HTTP API. Tools are never
advertised or executed; tool proposals are refused. Context, input, output-token
reservation, HTTP response and session time limits apply. Nonblocking input permits
cancellation without a blocking stdin thread; terminal control characters in model
text are escaped. `/reset` clears context without refunding session budgets.
Chat and external agentic clients can use the service in separate processes,
subject to shared admission limits. See [chat guide](CHAT.md).

Shell syntax and formatting checks pass. Rust 1.99 production workspace checks
(`cargo check --workspace --bins --locked`, default and all features) pass.
No installer run, live chat or new test suite has
been executed for this milestone. Release readiness and model quality remain
unproven; see [completion audit](V1_COMPLETION_AUDIT.md).


## GPU sensor and explicit KV-cache placement — 2026-10-24

**Implemented; runtime verification deferred at the operator's request.**
Optional `nvml-gpu` adds a read-only dynamically loaded NVML sensor. Diagnostics
and comparison observations accept `--gpu-sensor nvml`; the default remains
`nvidia-smi`. A fixed same-image child returns a bounded, versioned FlatBuffers
frame (`protocol/gpu.fbs`). Both sensor children have time/output/device limits
and tracked cancellation cleanup. Missing metrics stay unknown; observations are
whole-device and do not establish task-attributed energy or direct CUDA inference.

Explicit `offload_kv_cache_to_gpu` is supported in approved residency loads,
plans and native lifecycle policies for LLM/decision models. The provider must
echo and subsequently report the requested boolean. Explicit settings use v2
receipts/plans, pin KV placement and reject drift; legacy v1 inventory hashes
retain their original field projection. This controls KV cache, not model-layer
placement, GPU isolation or physical resource enforcement. See
[GPU observations](GPU_OBSERVATIONS.md) and [native lifecycle](NATIVE_LIFECYCLE.md).
No new test, live GPU query, benchmark or readiness claim accompanies this work.


## Explicit immutable project profiles — 2026-10-24

**Implemented and verified.** Architecture §3 now has explicit
operator-declared project goals and typed presentation preferences, selected only
by exact `id@version`. `src/projects.rs` defines bounded strict manifests,
checksums and context; `src/store.rs` migrates SQLite to schema 9 and provides
immutable versioned publication/read/list APIs with fixed quotas. Management
permissions remain human-only. No preference inference or automatic learning is
claimed.

`project-publish`, `project-show`, `project-list` and `ask --project` expose the
feature. The optional FlatBuffers AskRequest field is propagated through the UDS
service. `src/engine.rs` records the exact selected profile before inference,
preserves default prompts when absent and conveys compatible presentation
requests without changing the original query or verification criteria.
`src/recovery/continuation.rs` validates v2 execution options and restores the
original bounded snapshot independently of later registry versions. Legacy tasks
retain their original continuation rules.

New store, wire, engine, recovery and CLI/socket regressions cover bounds,
immutability, quotas, schema migration, tampering, explicit selection, unchanged
deterministic answers, original prompt caps and exact snapshot continuation.
See [project profiles](PROJECT_PROFILES.md). Nineteen focused unit tests and four
CLI/socket integration tests pass, including exact recovery and prompt budgets.
Final serialized suites pass **1,048 default tests** and **1,057 all-feature
tests**, each across 72 targets, zero failed or ignored. Formatting, Rust 1.99
workspace check and strict default/all-feature Clippy pass. The suites completed
before the operator requested stopping further tests. Installed smoke scripts were
not rerun for this change; CLI/socket workflows are covered by the completed
integration suites;
no live-model adherence or general preference learning is proven.

## Exact Wasm registry reconstruction — 2026-10-24

**Implemented and verified.**
Architecture §6/7/9/10:
`recovery::continuation::reconstruct_wasm_registry` restores only operator-supplied
compiled guests matching one strict v4 snapshot. It preserves the original broker
UUID, capabilities, implementation hashes and queue/concurrency limits. The CLI
accepts explicit `resume-task --human-approve --restore-wasm-expert` manifests,
compiled before state initialization. No stored pathname supplies code. Exclusive
lease, bounded transaction/history checks and atomic audit refusal preserve the
existing unknown-dispatch and continuation gates. Native scopes, mixed registries
and earlier snapshots without original limits are refused by this reconstruction
route; retained legacy continuation remains supported.

New integration regressions cover reopen reconstruction with nondefault limits,
no approval, substituted identities, wrong versions/capabilities, old snapshots,
invalid/missing/unknown limit fields, duplicate snapshots, native scope, terminal
tasks, extra guests, uncertain history and audit rollback. CLI approval refusal
is checked before state initialization. See [Wasm recovery](WASM_EXPERTS.md).

Final serialized suites pass **1,032 default tests** and **1,041 all-feature
tests**, each across 71 targets, zero failed or ignored. Rust 1.99 workspace
check and strict default/all-feature Clippy, formatting, diff checks and final
offline CLI/socket/authenticated HTTP smoke checks pass. An earlier source-edit
fingerprint failure and cross-feature shared-executable test conflict were
resolved by frozen-source, serialized full reruns. Live model inference remains
unperformed; the local endpoint probe refused connection. This entry supersedes
the guest reconstruction limitation in the earlier milestone below.

## Optional bounded Wasm experts — 2026-10-24

The optional `wasm-experts` adapter registers hash-pinned compiled guests through
the ordinary typed broker and task grants. No imports/start functions, strict
memory/fuel/stack/table bounds, two retained worker permits and original deadlines
bound execution. CGWI/CGWO payloads retain FlatBuffers; responses are checked
before unpacking and against original requests, with host acceptance criteria.
Version-3 task snapshots bind artifact/settings identities for retained recovery;
builtin reconstruction refuses guest substitution. CLI `ask`/`serve` accept up to
eight strict operator manifests. Cross-process guest reconstruction remains
unsupported. See [Wasm experts](WASM_EXPERTS.md) for ABI and limitations.

The full all-feature suite passes **1,039 tests across 71 workspace targets**,
zero failed or ignored, including alias-amplification and recovery regressions.
Rust 1.99 workspace check and strict all-feature Clippy, formatting, exact pinned
FlatBuffers generation and offline CLI/HTTP smoke checks pass. The repeated
default suite passes **1,031 tests across 71 targets**, zero failed or ignored;
strict default Clippy also passes.

## Prepared RAG candidate provenance — 2026-10-24

`candidate create-prepared` now imports pinned version-2 preparation bundles with
strict hash, byte-framing, metadata, duplicate and held-out split validation. The
candidate and metadata-only provenance commit atomically; audit failure rolls back
candidate/version/anchor rows. Original source metadata stays attached to the
candidate and protected from task retention. `candidate provenance` is read-only.
Provenance grants no evaluation or promotion authority; independent tests and
explicit operator approval remain required.

Focused tests cover CLI import/inspection, embedded headings, held-out exclusion,
retention, promotion refusal, fifteen tampering cases and transaction rollback.
Combined checks pass **1,030 tests across 70 workspace targets**, zero failed or
ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check
and strict Clippy with default/all features pass. Offline CLI/HTTP smoke scripts
pass. See [source preparation](SOURCE_PREPARATION.md).

## Optional benchmark device observations — 2026-10-23

`compare --observe-gpu` now samples bounded whole-device metrics during each
execution window using a shared run-wide query cap. Reports include sampled
maxima, partial observed power intervals, missing-data/identity flags and query
cleanup overhead. No detached queries remain after a window. Default comparison
output and original execution budgets remain unchanged when observation is absent.
Task energy, true peak memory and full-window energy remain explicitly unknown.

Combined checks pass **1,027 tests across 69 workspace targets**, zero failed or
ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check,
strict Clippy with default/all features and offline CLI/HTTP smoke scripts pass.
The five deterministic window tests and actual CLI regression establish measurement/accounting
mechanics, not real-model performance, physical attribution or efficiency gains.
See [GPU observations](GPU_OBSERVATIONS.md).

## Local source discovery — 2026-10-23

`candidate discover-sources` now inventories a bounded operator-selected local
document tree on Linux using retained directory descriptors and no-follow reads.
It writes sorted hash/provenance records in a private draft bundle, without
initializing the runtime. Review is false and all sources remain reference-only;
the operator must review documents and choose held-out splits before preparation.
Counts, paths, depth, bytes and elapsed work are bounded; unsafe or incomplete
inventories refuse rather than returning a successful truncated draft.

Combined checks pass **1,021 tests across 68 workspace targets**, zero failed or
ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check,
strict Clippy with default/all features and offline CLI/HTTP smoke scripts pass.
Discovery/preparation tests cover no overwrite, required review, held-out exclusion
and unsafe inventory refusal.
Remote discovery and independently established source quality remain incomplete.

## Reviewed local source preparation — 2026-10-23

`candidate prepare-sources` prepares hash-pinned reviewed local sources without
initializing the runtime database. It uses bounded strict manifests and retained
directory-relative no-follow file descriptors, normalizes text, deduplicates exact
documents and refuses exact cross-split overlap. A new private bundle contains a
reference-only RAG artifact, separate held-out documents and provenance/split/hash
reports. It never creates/promotes a candidate or model. Existing independent
candidate evaluation and approval gates remain required.

Focused regressions cover CLI privacy/no initialization, no overwrite, changed
hashes, document deduplication and sixteen unsafe/ambiguous input cases. Combined
checks pass **1,019 tests across 67 targets**, zero failed or ignored, plus **four
optional Wasm tests**. Formatting, Rust 1.99 workspace check and strict Clippy with default and all features pass. See
[source preparation](SOURCE_PREPARATION.md).
Automatic discovery, semantic leakage detection, independent source curation and
neural/physical performance evidence remain open requirements.

## Completion audit and bounded native history — 2026-10-23

Read [the requirement-by-requirement audit](V1_COMPLETION_AUDIT.md) for the current
completion decision and remaining obligations. The entries below are historical
snapshots; later entries can supersede earlier implementation limitations and
test counts. Passing framework tests does not complete the scientific roadmap.

The audit found that native generation history had row/payload limits but lacked
an independent SQLite scan deadline. Its gate now installs a five-second progress
handler, checks elapsed time while validating rows, refuses expiry, and clears the
handler on success or failure. New regressions cover expired deadlines, subsequent
SQL usability, incomplete admissions and duplicate completions. Combined checks
pass **1,016 tests across 66 targets**, zero failed or ignored, plus **four Wasm
tests**. Formatting, Rust 1.99 workspace check, strict Clippy with default and all
features, exact generation of all ten FlatBuffers bindings and both offline
installed CLI/HTTP smoke scripts pass.

The earlier remaining-work summaries were too narrow. Independent live evaluation,
task-attributed physical telemetry, general untrusted expert/verifier isolation,
automatic source preparation and the architecture's research milestones still
have missing implementation or evidence. These gaps remain tracked in the audit;
no full Jev+/Jev-R completion or release readiness is claimed.

## Native-owner continuation — 2026-10-23

Supported original native checkpoints can now continue through the explicit
`resume-task TASK_UUID --human-approve --native-policy POLICY.json` path. A private
validated context retains exact original receipts, role targets and exclusive
ownership through normal checkpoint/resource admission. Original charges, route
maps and deadline remain enforced. The source-checked builtin worker borrows that
owner rather than acquiring conflicting leases. Each new model call has a durable
separate lifecycle anchor; unknown effects block mutation and cannot be replayed
by an operator stop acknowledgement. This path does not load or unload models.

The workspace passes **1,014 tests across 66 targets**, zero failed or ignored,
plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check and strict
Clippy with default and all features pass. Actual CLI/worker/loopback regressions
cover configured resource admission, exactly one new call, no reload/unload,
wrong-instance failure, operator resolution and refusal to replay uncertain work.
Each call adds a local lifecycle audit and fresh inventory round trip; no latency
or efficiency gain is claimed. The real server probe at `127.0.0.1:1234` refused
connection, so real-model quality, performance and GPU behavior remain unverified.
Unsupported/unknown checkpoints and arbitrary callback reconstruction still refuse;
general v1.0 completion requires a separate requirement-by-requirement audit.

## Native restoration inspection — 2026-10-23

The new read-only `native-resume-preview TASK_UUID --policy POLICY.json` checks
original policy, role/receipt bindings, completed generation admission, remaining
same-boot time and fresh inventory under exclusive local ownership. It changes no
task/model state, grants no permission and explicitly does not validate or admit a
continuation checkpoint. Native-owner continuation itself remains incomplete.

The workspace passes **1,012 tests across 66 targets**, zero failed or ignored,
plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check and strict
Clippy with default and all features pass. Library/CLI regressions verify no
generation/unload or history changes and refusal of altered policy, model, clock,
generation endpoint/phase and receipt-binding evidence. These are loopback and
framework checks; real-model performance and quality remain unverified.

## Source-bound builtin restart recovery — 2026-10-23

Version-2 typed snapshots now record declared source identities. Explicit
operator-approved reconstruction supports the fixed compiled builtin worker after
host restart; it validates exact source inputs, descriptors, original registry
identity, prior reservations and complete reaped-child history. It does not replay
requests or admit a checkpoint. The CLI opt-in is
`resume-task TASK_UUID --human-approve --restore-builtin-worker`; normal
continuation independently enforces its existing gates and original budgets.

The combined workspace passes **1,011 tests across 66 targets**, zero failed or
ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check
and strict all-feature Clippy pass. Real-process regressions verify both completed
child history without replay and a resumed typed worker in a new CLI host.
Native-owner continuation remains incomplete; arbitrary Rust callbacks require
their retained host objects. Source equality is not binary/compiler attestation
or evidence of real-model reasoning quality or performance.

## Native worker ownership verification — 2026-10-23

Native task catalogs are now forked from the service template before provider
mutation. Process adapters borrow idle ownership weakly and retain it strongly
only during child execution/reaping; expired adapters fail before launch.
Standalone native task scopes also retain the operator policy digest. This fixes
lease retention and duplicate worker registration across successive service tasks.

The updated workspace passes **1,007 tests across 66 targets**, with no failures
or ignored tests, and all **four optional Wasm tests** pass. Formatting, Rust 1.99
workspace check and strict all-feature Clippy pass. These results supersede the
previous snapshot below. Native-owner continuation after restart and real-model
quality/performance validation remain incomplete.

## Verified integration snapshot — 2026-10-23

The schema-8 recovery/archive, approved native lifecycle, typed module workers,
provider pins, recorded-wave reconciliation, optional Wasm verifier and offline
security-corpus runner have completed a combined automated verification pass.
This is framework evidence, not a claim of trained Jev networks or real-model quality.

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo test --locked --workspace --all-targets --no-fail-fast`: **1,005 passed
  across 66 targets**, no failures or ignored tests (Rust 1.97.1).
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`
  and its `--all-features` variant: passed.
- Optional Wasm target: **4 passed**, including fuel exhaustion without native
  stack overflow and independent compiled-checker authority.
- Exact `flatc 25.12.19` output matches all ten binding files. The reviewed-bundle
  compatibility/corruption tests pass.
- Offline CLI and authenticated HTTP smoke scripts pass against copied current
  binaries using fresh temporary state; no live inference is involved.

Verification found and fixed malformed-candidate diagnostic failures, CLI exit
before cancelled-worker reaping, idle Broker worker retention, and stale provider,
protocol-count and native-service fixtures. Typed sorting/ordering/path diagnostics
and opt-in finite assignments now have Engine-level regression tests proving
original charges and independent compiled checking.

**Typed registry restoration:** `RuntimeRestoration`, `preview_with_runtime` and
`resume_with_runtime` now accept the retained original Broker/Store and optional
original verifier/escalator. They restore exact retained versions across active
swaps and preserve earlier charges. Read-only preview, end-to-end continuation,
process audit completeness and unknown/forged history refusal are regression-tested.

**Remaining work:** arbitrary registry reconstruction across process restarts is
unsupported. Native lifecycle continuation requires restoring its exact owner and
bindings and remains refused by default.
A probe of `127.0.0.1:1234` found no inference server, so real model quality, GPU
behavior and comparable-budget performance remain unverified. Novel neural Jev
architectures and independent neural promotion remain research work.

Older entries below describe their own historical snapshots. The newest evidence
above supersedes earlier source-only and pending-verification notes.

### Track approved native loads before managed release — 2026-10-23

**Implemented and regression-tested; general lifecycle remains Partial.** Architecture sections 4, 6,
9 and 10 require sparse model lifecycle control with independent permissions.
A dedicated managed load/release API and CLI will bind local receipt UUIDs to
approved native requests, exact observed instance identities and full-inventory
digests. These receipts do not establish exclusive provider ownership, immutable
weights or instance-bound inference. Existing explicit manual administration stays
available; its mutations invalidate managed release eligibility before POST.

Schema 7 adds bounded receipt state and retained operation references without
adopting old provider instances. Known managed transitions atomically advance the
active endpoint cohort; uncertain effects retain blocking evidence and cannot be
retried through managed APIs. CLI mutation requires explicit approval and an
existing current-schema workspace; local receipt inspection does not contact the
provider. No model gains residency permissions or automatic cleanup authority.
Sources: `src/residency/managed.rs`, native manual-mutation integration,
`src/main.rs`, schema migration/restore and storage retention. See
[managed residency](MANAGED_RESIDENCY.md). Focused tests cover schema 1–6 migration
without adoption, atomic rollback, receipt integrity, bounded query plans and CLI/
loopback lifecycle behavior. General autonomous lifecycle remains Partial.



Verification on final frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**940 passed across 61 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Eleven CLI/loopback integration tests cover multi-model cohorts, provider-ID reuse
with fresh UUIDs, non-adoption, endpoint/approval/workspace gates, manual-plan
invalidation, before/after-POST audit failures, unknown outcomes, actual client
future cancellation during pending POSTs, resource leases, retained unknown loads
and capacity refusal with releases still allowed. Storage/helper regressions cover
schema 1–6 migration without adoption, atomic failure/data preservation, receipt
integrity, retention, malformed markers, indexed queries and SQL deadline cleanup.

The first full runs found historical migration fixtures requiring v7-object
removal and a general event index causing worker-recovery timeouts. The index is
now partial to native residency events; the unchanged 10,000-session worker gate
passes again. Test compile/lint fixtures were corrected without removing tests.
Malformed managed markers cannot downgrade receipt validation or erase retention
protection. Existing socket/process, mock, malformed-frame, policy-denial,
cancellation and candidate-rollback tests remain green. Canonical French
architecture and protocol schemas/bindings are unchanged. Bundled FlatBuffers
pairing/integrity checks pass; `flatc` is unavailable, so fresh generation was not
run. Port 1234 refused local-model discovery: no real provider load/unload,
inference quality, physical memory or efficiency result is claimed.

Jev+ and Jev-R remain **Partial**. The next verifiable lifecycle milestone is
explicit operator resolution of uncertain operations and reviewed receipt
retirement, followed by task-to-native-instance integration. No model gains
mutation permission and local receipts still do not prove exclusive ownership.

### Measure shortest-path revision plateaus — 2026-10-23

**Implemented and regression-tested; general reasoning remains Partial.** Architecture sections 5 and 9 require
stopping when independently measured improvement becomes marginal. The next slice
extends optional progress stopping to `shortest_path/v1` with a separate versioned
history, preserving the finite-domain policy. Only bounded declared-node arrays
are comparable; malformed answers reset streaks. Compiled shortest-path checking
remains the acceptance authority; a progress score grants no verification.

Fresh enabled tasks bind the selected policy in execution settings and immutable
reasoning context. Recovery must restore that exact policy and history; legacy
unmarked tasks cannot gain scoring midway through continuation. Original budgets,
permission checks, provider-veto handling and exact-recurrence stopping remain
independent. Controlled progress comparison accepts both supported structured
criteria with matching per-case verification labels. Sources: `src/verification/path.rs`, `src/reasoning/progress.rs`, Session/Engine
policy pinning, `src/recovery/continuation_path_progress.rs`, result-proof
deduplication and `src/benchmarking/progress.rs`. Runtime and recovery targets
pass 11 tests; focused helper tests cover score/checker equivalence, bounds and
malformed history. Final workspace results follow below. These are functional
regressions, not measured neural quality, latency or calibrated expected utility.



Verification on final frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**921 passed across 60 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
The 12 new integration tests cover runtime stopping, original-budget HTTP
continuation, repeated charged result rechecks with one proof, legacy behavior,
nine tamper/no-dispatch cases and a two-case HTTP comparison. Helper tests cover
exhaustive small-graph score/checker equivalence, bounds, history transitions,
criterion-label preflight and malformed/duplicate recovery evidence.

Tests exposed and fixed shortest-path result-proof deduplication missing from the
existing finite-only gate. Review also fixed duplicate cycle-zero proofs and
foreign/malformed progress audit acceptance. Initial fixture/compile/lint errors
were corrected; no tests were removed. Existing socket/process, migrations, mock,
malformed-frame, policy-denial, cancellation and rollback coverage remains green.
Protocol schemas/bindings are unchanged; bundled generator/runtime checks pass.
`flatc` remains unavailable, so fresh generation was not run. Localhost port 1234
refused discovery. No real-model quality, GPU, latency or energy gain is asserted.
Jev+ and Jev-R remain **Partial**: automatic ownership-aware provider lifecycle,
broader isolated executable verification and independent live-model effectiveness
measurements remain open. The next lifecycle step requires explicit provider
instance identity and durable ownership, without granting models mutation rights.

### Select eligible inference routes before pinning — 2026-10-22

**Implemented and regression-tested for declared eligibility. General Jev+/Jev-R remains Partial.**
With declared working profiles or residents, the Broker filters each supported
role against the immutable resource catalog before choosing by existing rank,
load and identity ordering. Ineligible preferred candidates can yield to an
eligible peer. Selection freezes the exact backend, module version and role map;
ordinary shared admission still controls actual capacity. Empty catalogs retain
the legacy path. Empty eligible maps permit deterministic tasks and deny an
actually requested inference role without provider dispatch.

Required intent precedes metadata callbacks; bounded selection and fixed exclusion
codes precede generation. Callbacks execute outside registry locks and publication
revalidates registry identities. Route scope binds policy evidence to the selected
role/module/target/profile map, excluding ephemeral pool identities and load
observations. Original budgets, permissions and unknown-outcome quarantine remain
unchanged. This is declared eligibility, not measured utility or physical residency.

Versioned recovery validates saved routing records and restores the exact supported
builtin role subset and route scope. Unmarked legacy histories retain their prior
rules; arbitrary custom modules remain unsupported for continuation. Sources:
`src/fabric.rs`, `src/engine.rs`, `src/resource_scheduler.rs`,
`src/reasoning/verification.rs`, `src/recovery/resource_routes.rs`.
Benchmark/evaluation details expose the stable route scope and bounded selected
module/role/profile identities with hashed targets, omitting endpoints and exclusion
errors. Empty-map recovery restores a deny-all backend only with no prior model
dispatch; it cannot acquire new inference roles. Regression coverage: `tests/resource_route_selection.rs` (12 tests),
`tests/resource_route_resume.rs` (3 tests), four empty-route recovery unit tests
and four route-report metadata unit tests. Existing `tests/working_profiles.rs`
retains direct admission audit-before-metadata and attempted-call accounting checks.
The queue regression observes an actual waiting parallel call before registry
replacement and verifies the original routes finish without provider fallback.
Automatic ownership-aware loading/unloading, broader isolated module lifecycle and
independent real-model quality/cost evaluation remain incomplete.



Verification on frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**899 passed across 57 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Initial new-test compilation/fixture errors and lint findings were corrected;
the queue test now uses actual same-task parallel work because the Store execution
lease prevents simultaneous task runs. Existing socket/process, migrations, mock
inference, malformed-frame, policy-denial, cancellation and rollback tests remain
green. Schemas/bindings are unchanged; bundled FlatBuffers pairing/integrity checks
pass. `flatc` is unavailable, so fresh generation was not performed. Localhost
port 1234 refused discovery; no real-model/GPU quality or speed claim is made.

### Admit heterogeneous model working sets under one resource pool — 2026-10-22

**Implemented (configured admission); Partial (general module lifecycle).**
Architecture sections 4, 6 and 9 require resource-aware coordination of replaceable
models. Optional
`resource_admission.working_profiles` binds bounded operator-declared allocations
to canonical local model targets and roles. An active table requires an exact
match; no unknown/unlisted/deterministic target may use the uniform fallback.
Absent/empty tables preserve legacy behavior and configuration bytes.

The runtime pins a backend before resolving its target and dispatches through
that same pin after selecting the allocation. Required profile intent/decision
audits bind the canonical catalog digest, role, target, profile and allocation.
Existing resident footprints, shared queue/capacity, task budgets, cancellation
and unknown-outcome quarantine remain authoritative. No model grants, automatic
provider load/unload or physical memory attestation are introduced.

Resource recovery now validates bounded per-reservation histories instead of a
single serial chain, allowing complete overlapping advisory waves. Every chain
must match an earlier charged same-role dispatch, original profile/target/
allocation and complete ordered admission/outcome audits. Saved configuration
retains authority; current config-file changes cannot replace it, and incompatible
live pools are refused.

Sources: `src/resource_scheduler/profiles.rs`, pool/adapter/admission integration,
configured Session/parallel wrappers, native ChatBackend snapshots and
`src/recovery/continuation_resources.rs`. Tests: `tests/working_profiles.rs`,
`tests/agent_working_profiles.rs`, `tests/working_profile_resume.rs`. See
[working profiles](RESOURCE_SCHEDULING.md#target-specific-working-profiles).
This addresses heterogeneous logical admission; broker eligibility/fallback,
automatic ownership-aware residency, untrusted executable isolation and independent
physical/quality measurements remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification after implementation: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**876 passed across 55 targets**,
none failed/ignored) and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass on the
final frozen sources. Nine admission/configuration tests cover strict bounds,
empty-table bytes, CLI roundtrip, per-role targets, real shared-pool pressure,
unknown/unlisted/deterministic targets, metadata-error redaction, audit failure,
exact quarantine and registry replacement while queued. Three native-turn tests
cover refusal without snapshot support, actual LocalChat HTTP dispatch and a
queued snapshot surviving a route change while new turns select the new route.

Three recovery tests validate profiled and legacy parallel histories, unchanged
original settings despite current config-file changes, exact retained calls/
tokens/cycles/allocations, nine altered audit-chain cases with no writes/HTTP, and
actual overlapping pending HTTP calls. The original first wave uses a bounded
barrier: both resource dispatches must precede either completion in the retained
checkpoint. No planner or completed subtask is replayed. Source review found and
fixed the native chat snapshot gap; initial build/test fixture errors (new struct
field, private Store access and moved values) were corrected without removing
tests or weakening resource/identity assertions.

Existing socket, database/migration, mock, malformed-frame, permission-denial,
cancellation, verifier, candidate-rollback and current-source-fingerprint checks
remain green. Protocol schemas/bindings are unchanged and bundled pairing/integrity
checks pass. `flatc` is absent, so fresh generation was not run. Local port 1234
refused connections; real-model/GPU performance was not measured. These fixtures
prove logical admission/reconciliation behavior, not physical memory use or a
quality/speedup advantage. Final logs: `/tmp/jev-working-profiles-{check,tests,clippy}-final.log`.


### Pin an optional trusted host verifier to structured reasoning — 2026-10-22

**Implemented (native extension); Partial (general executable verification).**
Architecture sections 4–6 and 9–10 require replaceable roles with independent
evidence and fixed permissions. Native hosts can select a `VerifierProvider` with
`Engine::with_verifier_provider`; sessions and Engine clones retain the captured
descriptor and callback instance. The provider receives bounded original objective/
candidate data and the original deadline. Its claim and the original compiled
checker must both pass. This covers the four existing structured criteria, solver
answers, direct search witnesses and matching published-memory reuse; ordinary
prose and generation-only comparisons gain no checker authority.

Existing tool authorization/reservation remains outside the provider. Required
snapshot, intent and outcome audits bind the identity, criterion and both hashes,
with separate provider elapsed wall time. Invalid claims/errors cannot fall back
to unchecked success. Cancellation drops the owned future without refunds.
Effective workflow/escalation/advice fingerprints include the captured descriptor,
preventing built-in-only evidence from silently matching a changed acceptance rule.
Recovery refuses custom-provider histories until their implementation can be
restored, rather than silently dropping the selected acceptance gate.

Sources: `src/verification/provider.rs`, Engine/Session integration, advice scope
construction and recovery admission guards. See
[host verifier contract](VERIFICATION.md#trusted-host-verifier-extension).
The digest is operator-declared, not binary attestation. Cooperative async timeouts
cannot isolate blocking native code. Arbitrary untrusted executable loading, new
criterion families, provider restoration and independent quality/physical-cost
measurements remain incomplete; Jev+ and Jev-R remain **Partial**.

Verification after implementation: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**861 passed across 52 targets**,
zero failed/ignored) and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Sixteen provider integration tests cover actual dispatch, exact retained budgets,
false acceptance, vetoes, errors/invalid claims, original-deadline exhaustion,
cancellation/drop, every required audit gate, captured descriptor/decision/history
scope, memory rechecks, direct-search fallback and recovery refusals. Two new
progress unit regressions preserve the original score-zero invariant and validate
veto cycle/scope/plateau behavior. Three saved-parallel-advice crash-window tests
also pass in this full run; the existing reconciliation implementation needed no
production change for that window.

Source review found and fixed unbounded JSON inspection in the new recovery guard.
Testing found and fixed a provider-veto/finite-progress composition issue before
the final gates. Test fixtures were corrected for their actual planner/solver
prefix, original wall ceiling, interrupted-intent semantics and retained search/
fallback checks; acceptance, accounting and no-dispatch assertions remain.
Default socket, persistence/migration, malformed-frame, permission denial,
cancellation, rollback and current-source-fingerprint regressions remain green.
Schemas/bindings are unchanged and bundled generator/runtime checks pass; `flatc`
is unavailable, so fresh generation was not run. Local discovery at port 1234
refused connections; no real-model/GPU, general quality or speedup result is claimed.
Final logs: `/tmp/jev-verifier-{check,tests,clippy}-final.log`.


### Continue completed parallel advisory waves without replay — 2026-10-22

**Implemented (completed-wave checkpoints); Partial (general in-flight recovery).**
Architecture sections 4, 5, 7 and 9 require persistent task progression under
original budgets. Operator-approved continuation now validates the original full
parallel schedule and a prefix of entirely recorded advisory waves. Every completed
node must match a unique charged cycle-zero solver result, original graph/order,
text hash and bounded result, with the original reservation/admission/publication
chronology. Later waves must remain pending. Running, unresolved, partial or
conflicting work cannot create a new inference allowance.

Resumed execution skips completed waves, retains the full original schedule and
its single marker, and keeps the original configured broker concurrency. It
preserves the original model/context/settings, calls, outputs, prompt bytes, tools,
cycles, deadline and shared-resource admission. A ready final-solver boundary can
reuse all recorded advice without rerunning planner or subtasks. Independent
whole-goal checking retains success authority; advice and history metadata cannot
grant permissions or establish a verified answer. No weights or capabilities are
promoted. General incomplete-wave reconciliation and independent live utility/
physical-cost evidence remain incomplete; Jev+ and Jev-R remain Partial.

Sources: `src/recovery/continuation.rs`,
`src/recovery/continuation_parallel.rs`, validated graph schedule access and
`src/reasoning.rs` and `src/reasoning/parallel.rs`. See
[parallel recovery](RECOVERY.md#completed-parallel-advisory-waves).

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**840 passed across 50 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Five HTTP integration tests cover a completed
non-scheduler-prefix wave, actual overlapping pending calls, original criterion
checking, exact final-solver boundary, pre-marker initialization, nine altered
history/state/clock/budget cases with no writes or HTTP dispatch, and admission
audit rollback. The completed first wave retains three prior calls; three new
calls finish with six total calls and 1408 reserved output tokens, matching the
uninterrupted authored fixture. This is replay/accounting evidence, not a live-
model efficiency or quality comparison. Nine helper tests include two new executor
regressions rejecting invalid saved ceilings/partitions or unmet dependencies
before any reservation. Independent source review found no blocking issue.

Initial borrow-checker errors were fixed; integration fixtures were corrected to
use a pending cancellation future and the retained ledger counts after rewind.
Assertions for actual overlap, budgets and independent satisfaction remain intact.
Existing socket, migration, mock inference, malformed-frame, policy-denial,
cancellation, shared-resource, recovery and rollback coverage remains green.
Protocol schemas/bindings are unchanged; bundled generator/runtime pairing and
integrity checks pass. `flatc` is absent, so fresh generation was not run. Approved
localhost discovery at port 1234 refused connections; no real-model/GPU measurement
or general reasoning-quality claim follows. Incomplete/unknown waves and broader
module recovery remain separate work.

### Execute dependency-ready advisory subtasks in bounded parallel waves — 2026-10-22

**Implemented (opt-in bounded concurrency); Partial (general module scheduling).**
Architecture sections 4, 5 and 9 require coordinated multi-role work under fixed
budgets. Optional `subtask_parallelism` (1–4, absent by default) groups the pinned
four-node planner graph into deterministic dependency-ready waves. Independent
subtasks can overlap; dependent nodes wait for prior-wave results. Existing
committed scheduler order determines wave selection and result reconciliation.
Each wave authorizes and durably reserves individual calls before backend polling,
retaining the final solver allowance and original deadlines. Shared resource-pool
admission remains authoritative. Local future ownership preserves cancellation;
unknown outcomes retain charges and do not establish provider stopping.

A required metadata-only parallel schedule binds the graph/order/ceiling/waves
with no permission or verification authority. Graph observation validates overlap
against that partition; default sequential auditing remains unchanged. Parallel
checkpoint continuation is explicitly refused pending separate reconciliation.
Sources: `src/config.rs`, Engine construction, `src/fabric.rs`,
`src/reasoning.rs`, `src/reasoning/parallel.rs`,
`src/recovery/{graph,graph_parallel,continuation}.rs` and integration tests.
See [scheduling](SCHEDULING.md#opt-in-parallel-advisory-execution).
No new external capability, active weight change or physical GPU allocation is
introduced. General module scheduling, wave recovery and independent usefulness/
efficiency measurements remain incomplete; Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**833 passed across 49 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Seven helper tests cover configuration compatibility,
wave selection/caps, malformed orders and local future cleanup; three broker tests
cover concurrent cap enforcement, queueing, shutdown/drain and invalid caps.
Six Engine integration tests prove actual backend overlap, reverse completion with
dependency gating, deterministic result order, exact reservations/final-solver
reserves, cancellation/unknown usage, serial compatibility, insufficient budgets
and both schedule/partial-wave reservation audit failure. Four graph tests cover
bounded observation, 14 marker alterations, premature results/wave crossing and
read-only continuation refusal. Existing socket, migration, mock inference,
malformed-frame, policy-denial, recovery and candidate-rollback coverage remains green.

The first overlap test exposed the legacy broker serialization; the implementation
was extended rather than weakening that test. Broker panic behavior was restored
after the existing health regression caught JoinSet isolation changing it. Lint
findings were fixed without suppression. An intermediate full run detected source
edits during execution through the existing fingerprint check; the final frozen
rebuild and full rerun pass. Protocol schemas/generated bindings are unchanged;
bundled generator/runtime pairing and integrity tests pass. `flatc` remains absent,
so fresh generation was not run. Approved localhost discovery at port 1234 refused
connections; no live-model/GPU efficiency or quality result is claimed.

### Prune impossible partial finite-domain assignments — 2026-10-22

**Implemented (bounded deterministic pruning); Partial (general solving).**
Architecture sections 5 and 9 require algorithmic reasoning under fixed compute
ceilings. Jev-R now rejects impossible partial linear ranges and assigned-value
collisions in all-different constraints before enumerating their remaining
variables. Signed i128 arithmetic and once-per-search true domain extrema keep
bounds conservative, including sparse domains and i64 extremes. Disequality only
prunes an interval fixed to the forbidden value. Declaration-order traversal and
the independent complete-assignment witness checker remain intact.

The root and each visited partial assignment still consume one node; unvisited
pruned descendants consume none. The existing 4096-node ceiling, cancellation
polls, original tool reservations and final stop check remain authoritative.
Contradictions can now be established earlier; a limit is still inconclusive.
Receipt schemas and recovery budgets are unchanged, and historical receipts are
not replayed. No external execution, new permission or neural adaptation is added.
Sources: `src/verification/finite.rs`, its unit tests and
`tests/finite_search_pruning.rs`. See [search contract](FINITE_SEARCH.md#search-and-outcome-contract).
This is necessary-condition pruning, not full domain propagation, Hall-set
filtering, a general SAT solver or evidence of improved model intelligence.
Jev+ and Jev-R remain Partial; general search recovery and independent quality/
cost evaluation remain incomplete.

Verification: formatting, locked all-target workspace build, **813 tests across
48 targets** (none ignored) and Clippy with warnings denied pass. Fourteen focused
finite-domain unit tests and six new integration tests cover every relation,
wide signed arithmetic, sparse domains, deterministic exhaustive comparisons on
72 generated cases (both satisfiable and unsatisfiable), original checker events,
root accounting, exact witness/cap edges, cancellation and the hard 4096 ceiling.
The independent legacy traversal reference visits 10 nodes versus the new 4 for
a sparse linear contradiction and 31 versus 11 for an all-different contradiction.
These are logical search-work examples, not latency, live-model quality or
physical-efficiency measurements. The initial test fixtures were corrected for
sign and earlier pruning; the cap fixture now uses a 16-variable parity case that
intervals cannot resolve. Three test-only lint findings were fixed, with no
suppression or gate removal. Existing socket, migration, mock inference, malformed
frame, denial, cancellation, recovery and rollback coverage remains green.
Schemas and generated bindings are unchanged; bundled generator/runtime pairing
and integrity checks pass. `flatc` is unavailable, so fresh generation was not run.

### Find a complete declared capability cover within pinned route limits — 2026-10-22

**Implemented (bounded exact complete coverage); Partial (general calibrated routing).**
Architecture section 4 requires dynamic capability discovery and robust routing;
section 9 requires sparse activation under fixed ceilings. Jev+ now uses
`bounded-cover/v3`: preserve initial route order/pins and version exclusions, then
find a full compatible available cover with the fewest additional providers when
one fits the remaining four-route capacity. Equal coverage masks share a stable
representative. A two-provider-union table plus superset transform is exact for
complete sets of at most four additions; ties are deterministic, not a claimed
globally lexicographic optimum. If no full cover fits, the old greedy partial rule
remains and is not presented as maximum partial coverage.

Bounds remain 4096 skills/manifests and 16 requested IDs. Compact indices cap
pair enumeration at 8,386,560 visits and tables at 65,536 entries each; cheap full
cover/impossible-union paths avoid unnecessary transforms. The engine yields
before/after synchronous resolution so cancellation/deadline can stop publication
and later dispatch, without claiming mid-loop preemption or physical CPU limits.
The additive declaration-only `coverage_search` audit records feasibility, provider/
mask/work counts, additions and fallback with no permission authority.

Sources: `src/capabilities.rs`, `src/capabilities/cover.rs`, Engine audit dispatch,
resolver unit regressions and `tests/capability_cover.rs`. See [capability selection](CAPABILITIES.md#selection).
No tool grant, model call, network probe, active-version mutation or new provider
execution is added. Existing workflow/resource/verification gates still apply.
Minimum declared route count is not measured inference cost, backbone sharing,
physical memory saving or calibrated usefulness. Jev+ and Jev-R remain Partial;
independent provider utility/cost evidence and broader resource-aware scheduling
remain subsequent milestones.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**804 passed across 47 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Eighteen focused capability tests cover the greedy
trap, independent exhaustive small-case minimum-cardinality oracles, order
invariance, retained routes (including unavailable pins), version/availability
filters, ablation exclusions, fast paths and the full 4096-provider/16-bit bound.
The actual Engine integration exercises interpretation, fixed metadata audits,
pinned bindings and workflow dispatch across Full/Collective/Experts profiles.
Two unoptimized selection-kernel observations were 1089.067/1083.882 ms with
8,386,560 pair visits and 786432 bytes of table entries, excluding fixture/Cargo
and total-process overhead. These are synthetic local timings, not a model or
physical-efficiency comparison. See the reproducible [cost check](CAPABILITIES.md#deterministic-cost-check-2026-10-22).
Initial fixture expectations were corrected for a provider's compatible subset;
three test-only allocation lint warnings were fixed without suppression, followed
by a complete final rerun. Existing socket, database migration, mock inference,
malformed frames, permission denial, cancellation, continuation and rollback tests
remain green. Protocol/bindings are unchanged; exact bundled pairing/integrity
checks pass. `flatc` is unavailable and fresh generation was not run.


### Persist and independently recover direct search witnesses — 2026-10-21

**Implemented (durable direct witnesses); Partial (general search-phase recovery).**
Architecture sections 5, 7, 9 and 10 require persistent reasoning evidence with
independent checks and retained budgets. New `finite_search` witness receipts use
format 2: the exact bounded integer assignment and SHA-256 are one durable event
before the separate original verifier check. Non-witness receipts retain exact
format 1. SQLite audit receipt versioning does not replace typed FlatBuffers IPC.

The existing operator result-recovery API can now select this durable candidate
without a model solver event. A strict bounded validator checks original settings,
context, authorized search policy/charge lineage, unique receipt, assignment hash,
node limits and allowed later verifier work. It exposes only metadata/hash history;
the answer body remains in the owner-local task trace. Every fresh recovery check
uses another original tool allowance and checks the complete original criterion.
Prior search/checker reservations are retained, even if an original proof was lost.
No model or search replay, permission grant, fresh reasoning cycle or wall budget.

Successful fresh proof/status/audit writes commit atomically; a malformed receipt
or exhausted allowance writes nothing, and audit failure rolls back the new charge.
A hash-consistent but failing candidate stays unverified and cannot trigger model
revision. Saved receipt flags and old verifier events cannot complete a task.
Sources include `src/reasoning.rs`, `src/recovery/{finite_search_witness,finite_search,result}.rs`,
recovery module wiring and `tests/finite_search_witness_recovery.rs`. See
[direct witnesses](FINITE_SEARCH.md#persist-and-recover-direct-search-witnesses).
Legacy body-less witness receipts, contradictions and search-phase crashes before
receipt persistence remain unsupported. Jev+ and Jev-R remain Partial; resumable
search state and independent quality/cost evidence remain subsequent milestones.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**792 passed across 46 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen files. Eight new integration tests cover all three durable
receipt/checker crash windows, actual CLI recovery, fresh-check charging and
rollback, altered/duplicate/legacy receipt and phase rejection, task-answer distrust,
metadata redaction and a valid exhausted ledger with zero remaining allowance.
The focused eight-test binary also passes. Initial unused test imports were removed;
the exhaustion fixture was corrected to prove budget refusal rather than duplicate
charge refusal, followed by a complete final rerun. Existing limit/model recovery,
progress, socket, migrations, mock inference, malformed frames, policy denial,
cancellation, continuation and candidate rollback checks remain green. Exact
bundled FlatBuffers pairing/integrity checks pass; `flatc` is unavailable and fresh
generation was not run. No real-model, GPU/energy or general efficiency experiment
was performed for this change.


### Reconcile saved model candidates after bounded search — 2026-10-21

**Implemented (receipt-bound model-result reconciliation); Partial (general search-phase recovery).**
Architecture sections 5, 7, 9 and 10 require evidence-aware persistent reasoning
under independent permissions. `recover-verified-result` validates a completed
original search-limit receipt before reserving a fresh independent verifier check.
It recovers only a recorded ledger-backed solver candidate and checks the full
original criterion. A satisfying advisory candidate is still a genuine witness;
role labels and model assertions alone cannot complete a task.

Reconciliation never repeats search or inference and retains all original charges.
Failed checks remain charged, while identical recovery proof metadata may be reused.
Rejected advisory proofs are validated against their actual recorded solver text
and independent failure without advancing whole-goal progress. Failed whole-goal
checks may create a ready revision only under existing recurrence/plateau, graph,
escalation, resource and original deadline gates. Atomic audit failure rolls back
new proof/checkpoint/charge writes. Reports expose bounded `finite_search_history`.

Sources: `src/recovery/{finite_search,result,continuation,continuation_progress}.rs`,
progress regression tests and `tests/finite_search_result_recovery.rs`. See
[saved search candidates](FINITE_SEARCH.md#reconcile-a-saved-model-candidate-after-the-search-limit).
No capability grant, network access, generated code, protocol, migration, model
weight or acceptance threshold changes. Explicit proof reconciliation retains its
bounded SQL transaction; it does not create a new task wall allowance. Direct
search results and incomplete/skipped search phases remain unsupported. Jev+ and
Jev-R remain Partial; durable search-result recovery and independent quality/cost
measurement remain subsequent engineering milestones.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**784 passed across 45 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on frozen sources. Nine new integration tests cover receipt/candidate checks,
atomic audit rollback, repeated charged recovery, proof/assessment crash windows,
advisory witnesses and rejected-advice graph continuation, plateau refusal and
expired-clock reconciliation. Actual loopback HTTP confirms resumed call counts.
The eight focused progress-validator tests and six existing result-recovery unit
tests pass, including strict advisory proof tampering and legacy no-call proof
compatibility. Initial fixture errors (subtask state/plan shape, checkpoint count
and plateau length) were corrected against runtime semantics; production budgets,
thresholds and existing tests were preserved. Existing socket, migrations, mock
inference, malformed frames, denials, cancellation, continuation and candidate
rollback tests remain green. Exact bundled FlatBuffers pairing/integrity checks
pass; `flatc` is unavailable and fresh generation was not run. No real-model,
GPU/energy or general quality/performance measurement was performed for this slice.


### Continue model reasoning after a recorded search limit — 2026-10-21

**Implemented (validated limit-receipt continuation); Partial (general search recovery).**
Architecture sections 5, 7, 9 and 10 require persistent reasoning progress under
original budgets and independent permission checks. `src/recovery/finite_search.rs`
strictly validates a bounded original-context → policy → search reservation →
`limit_reached` receipt chain before restoring an existing reasoning checkpoint.
The exact objective hash, configured node ceiling, exhausted node count, event
sources/order, unique charge and non-authority flags must agree. Search history
cannot appear in a configuration/objective that did not enable it.

Supported ready solver/advisory-graph continuation retains all original tool,
call, cycle, token and prompt charges, context/model pins and deadline. It resumes
the saved strategy and never traverses the search tree again. Preview and durable
continuation reports expose bounded `finite_search_history` metadata. Repeated
admission revalidates the same receipt; it cannot create a new search budget.
Missing, duplicate, altered, oversized, post-boundary or incomplete search history
fails before new dispatch. No external solver, permission, migration, protocol or
neural promotion is added.

Search-phase crashes, terminal search witnesses/contradictions/deadlines,
budget-skipped searches and saved-result reconciliation remain unsupported.
Metadata consistency does not authenticate a malicious owner or prove physical
provider stopping. Jev+ and Jev-R remain Partial. Sources also include continuation
admission/reporting and `tests/finite_search_resume.rs`; see [search recovery](FINITE_SEARCH.md#continuation-after-an-incomplete-search).
The next search milestone is receipt-bound saved-result reconciliation and broader
phase recovery, followed by independently curated budget-matched quality studies.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**774 passed across 44 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on frozen sources. Eight helper unit tests validate receipt/cap/permission
shape, preserved verifier reserve, unknown/duplicate JSON and bounded histories.
Three integration tests exercise repeated actual HTTP solver continuation,
partially executed graph continuation, original model/configuration pins and 18
independent tampering cases with no admission writes. The focused helper and HTTP
suites also pass. Existing socket, database migration, mock inference, malformed
frame, permission denial, cancellation, continuation and candidate rollback checks
remain green. Exact bundled FlatBuffers generator/runtime pairing and integrity
checks pass; `flatc` is unavailable and fresh generation was not run. No real-model,
GPU performance or provider-stop measurement was made for this change.


### Search explicit finite-domain tasks under independent limits — 2026-10-21

**Implemented (opt-in bounded deterministic search); Partial (general solving and recovery).**
Architecture sections 5 and 9 require algorithmic search under enforced budgets.
`Config.finite_search.max_nodes` admits 1–4096 visited partial assignments, including
the root. Rust DFS respects declared variable/domain order, prunes only fully
assigned constraints, checks the original deadline and never runs generated code.
A separate Jev+ `constraints.solve` grant and tool charge precede traversal; a
second tool slot is preserved for independently checking a witness or fallback.
Default absence and the LLM-only profile retain their original behavior.

A witness must pass a separately charged original-criterion check. Exhaustive
contradictions stop with `proven_unsatisfiable` but remain rejected under the
satisfying-witness contract; witness-only evaluations cannot score them as solved.
Limit exhaustion retains charges and falls back to existing bounded inference.
Fixed-metadata search audit failures prevent further work. Saved enabled finite-domain tasks
cannot continue until pre-model search evidence has a validated recovery format;
no traversal or inference is replayed and no charge is refunded.

Sources: `src/verification/{finite.rs,finite/tests.rs}`, `src/reasoning.rs`,
`src/reasoning/finite_search.rs`, configuration, security grants, continuation
admission and `tests/finite_search.rs`. See [search contract](FINITE_SEARCH.md).
No model weights, protocol schema, migration or acceptance threshold changes.
Deterministic oracle/runtime checks establish this limited algorithmic behavior,
not held-out neural quality, physical efficiency or general intelligence. Jev+
and Jev-R remain Partial. The next search milestone is validated durable recovery
and a budget-matched evaluation on independently curated unseen problems.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**763 passed across 43 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Eleven finite-checker unit tests and ten runtime
tests include independent small-domain enumeration, exact node bounds, deadline
hooks, audit denial, ready cancellation, fallback accounting and all 17 public
cases: 14 checked witnesses and three exhaustive contradictions, with no model
calls. Configuration and saved-result recovery denial tests also pass. Existing
socket, migrations, mock inference, malformed-frame, policy denial, cancellation,
continuation and candidate rollback tests remain green. The first full run failed
the source-fingerprint consistency test while the final permission fix changed
source; a complete frozen-source rerun passed. Exact bundled FlatBuffers pairing/
integrity checks pass; `flatc` is unavailable and fresh generation was not run.
No real-backend inference or physical efficiency study was run for this slice.


### Retain and independently check the public progress-policy diagnostic — 2026-10-21

**Measured public integration; independent utility remains Partial.** Source
08f6217 completed 17 paired RESEARCH cases (34 tasks) on existing Qwen3 8B Q4_K_M
weights with the same collective and budgets. The disabled/enabled arms found
14/13 witnesses, used 85/78 calls and accumulated 55.87/48.44 seconds task latency.
The corpus has 14 satisfiable cases and three exhaustive-oracle contradictions.
The separate Rust enumeration checks all 34 final scores, retained report hash,
resource sums and audit projection consistency without the production verifier.
Raw report and provenance are immutable in `evaluation/results/`.

Only one plateau stop was recorded, on unsatisfiable case 16. The satisfiable
regression at case 4 did not trigger plateau stopping; different planner/advice/
solver paths and one public run prevent causal policy-effect claims. Lower cost
with fewer witnesses is not a demonstrated efficiency win. Default policy remains
off. No larger baseline, repeated trials, held-out/independent curation, provider
weight attestation or attributed physical resource proof exists. Jev+ and Jev-R
remain Partial; matched counterfactual/repeated independent studies remain open.

The initially off/unloaded loopback server was temporarily started with existing
weights (requested GPU ratio 1, context 4096, parallel 1); its initial state was
restored and checked after the 104.37-second run. No weight download, persistent
policy installation, capability promotion or neural change occurred. See
[public results](../evaluation/results/README.md#public-finite-domain-progress-ablation--2026-10-21).

Verification: formatting, locked workspace/all-target build checks,
**745 tests across 42 targets** (none ignored), and Clippy with warnings denied
pass on final unchanged sources. The focused two-test corpus/evidence regression
also passes. Existing socket, migration, mock inference, malformed-frame, denial,
cancellation, recovery and rollback checks remain green. Protocol/generated
bindings are unchanged; exact bundled pairing/integrity tests pass, while `flatc`
is unavailable and fresh generation was not run. Real-provider observations are
retained separately from deterministic mocks; all 34 final answers are checked
by a separately implemented exhaustive oracle. General quality, causal utility,
independent human review and physical safety claims remain unproven.


### Measure progress stopping with paired collective runs — 2026-10-21

**Implemented (explicit controlled workflow comparison); Partial (independent utility evidence).**
Architecture sections 5, 9 and 13 require measurable reasoning under comparable
budgets. `compare-progress` and `src/benchmarking/progress.rs` run the same collective
with the stopping option absent/present, using the same backend, frozen corpus,
configured resource ownership and original ceilings. Cases alternate which arm
executes first. Effective configuration, scope fingerprints, task identities,
paired quality changes and actual costs are reported; unmatched cases cannot
contribute paired benefit. Cancellation and the overall deadline preserve partial
reports. An enabled adaptive advice guard is refused to avoid live-history bias.

Only strict bounded explicit finite-domain cases are accepted, with no supplied
expected assignments. Scoring requires an actual independently checked witness;
model impossibility claims never pass. Public workflow reports retain bounded
content for external checking; held-out reports omit objective/answer bodies.
No report installs a policy, changes persistent configuration, promotes a model
or expands permissions. Provider/cache/thermal state and physical resource use
remain outside the comparison's control.

The new 17-case CC0 public corpus covers linear relations, signed/wide arithmetic,
all-different, alternative witnesses and contradictions. Its manifest binds hashes
and exact solution counts. `tests/finite_domain_corpus.rs` independently enumerates
at most 256 assignments per case without invoking the production checker. This is
an independent computational oracle, not independent human curation or held-out
quality evidence. Scripted loopback tests expose both saved calls and a missed
later witness; the actual CLI is exercised with the deterministic demo adapter.
Sources also include `tests/progress_ablation.rs`, `tests/progress_ablation_cli.rs`,
main command wiring and [benchmark instructions](BENCHMARKING.md#controlled-progress-stopping-ablation).
General utility calibration and real-model benefit remain unproven; Jev+ and
Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**744 passed across 42 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on the final unchanged source. Seven focused tests cover the exhaustive
corpus oracle, scripted paired tradeoff, strict preflight, held-out body omission,
pre-dispatch cancellation, partial deadline accounting and actual CLI deployment.
Initial compilation/fixture failures (JSON macro/report assembly, test borrowing
and private store access) were corrected; no tests or limits were removed.
Existing socket, migration, mock inference, malformed-frame, denial, cancellation,
continuation and rollback regressions remain green under approved local socket
access. Protocol/bindings are unchanged; exact bundled pairing/integrity checks
passed. `flatc` is unavailable and fresh generation was not run. These checks
establish runner behavior, not a real-model efficiency or quality improvement.


### Stop finite-domain revisions using independent progress evidence — 2026-10-21

**Implemented (opt-in finite-domain plateau stopping); Partial (general progress/utility).**
Architecture sections 5 and 9 require bounded evidence-aware revision and marginal
improvement stopping. Optional `Config.progress_stopping.plateau_cycles` (2–4)
tracks the number of violated declared constraints for complete, in-domain `csp:`
assignments. Shared strict parsing and bounded i128 checks preserve full acceptance.
Malformed/incomplete/out-of-domain output is incomparable. Improvements reset the
streak; equal/worse scores increment it; incomparable outputs reset only the streak.
Full independent verification runs first, and valid witnesses complete immediately.
A plateau retains the latest rejected answer/charges and stops before another
solver reservation; default absence retains legacy behavior and serialization.

Required fixed-metadata `reasoning_progress` audits precede escalation. Supported
continuation recomputes progress from recorded solver answers and matching failed
checks, validates audit ordering/content and the saved compact state, and refuses
missing/altered/gapped/future or already stopped histories. Operator result recovery
retains fresh-check accounting without inference replay. No new capability, model,
weight, tool allowance, migration or protocol schema is introduced.

Sources include `src/reasoning/progress.rs`, Session/configuration, shared finite
verification, `src/recovery/continuation_progress.rs` and result reconciliation.
Unit/scripted regressions and `tests/progress_stopping.rs` cover score correctness,
optional configuration, plateau/improvement/incomparable behavior, witness precedence,
audit failure and continuation with one new actual HTTP solver call. See
[progress stopping](PROGRESS_STOPPING.md) for bounds and limitations. Constraint
counts are not distance-to-solution, impossibility or calibrated marginal utility.
Real-model quality/latency gains and unrestricted reasoning progress remain
unproven; Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**737 passed across 39 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final unchanged sources. The focused progress selection passed 21 tests;
the two actual HTTP continuation tests also passed, including both operator
recheck crash windows. Existing socket, database migration, mock inference,
malformed-frame, denial, cancellation, continuation and rollback tests remain green.
Protocol/generated bindings are unchanged; reviewed exact generator/runtime pairing
and integrity tests passed. `flatc` is unavailable; fresh generation was not run.

An initial restricted test selection could not bind a service socket. Approved
host-socket runs passed after repairing new test fixture ordering/identity errors
and a missing import/helper attribute. The first full test pass was followed by a
Clippy match-style warning; its suggested simplification was applied and all gates
rerun successfully. Tests, limits and acceptance thresholds were retained. No live
model-quality, speed, GPU-use or held-out effectiveness result is claimed.


### Deploy reviewed workflow policies through the HTTP host — 2026-10-21

**Implemented (explicit host deployment); Partial (general policy lifecycle).**
Architecture sections 4, 9 and 10 require replaceable coordination decisions under
independent permissions. `crates/cogfabric-server/src/main.rs` now accepts paired
`--workflow-study` / `--workflow-thresholds` options. The existing bounded strict
no-follow loader validates and pins the policy before configuration/state/backend
initialization. Hidden worker entry points reject host policy options before
worker startup. Omission retains default tag routing.

Only the collective Engine receives the policy; native conversation turns retain
their existing path. Requests cannot select evidence files or thresholds, replace
the host router or raise budgets. Policy-file replacement after startup does not
change the retained policy. No persistent configuration, capability grant,
executable, model weight or provider lifecycle changes. Reviewed synthetic studies
still do not establish held-out quality, calibrated utility or physical efficiency.
Pre-routing checkpoint restoration and broader independent evaluation remain open;
Jev+ and Jev-R remain Partial.

The actual server-process regression covers default routing, evidence-matched
acceptance, exact accepted/declined policy identities, collective requests after
policy-file replacement, a stricter policy in a fresh host, native-turn isolation,
HTTP override rejection without inference, and invalid/partial/worker options
before state creation. These are synthetic loopback integration checks, not live
model-quality or speed measurements.

Sources and reproduction: server main, process integration in
`crates/cogfabric-server/tests/workflow_policy.rs`,
[workflow deployment](EMPIRICAL_WORKFLOW_ROUTING.md#direct-cli-and-local-service-deployment)
and [HTTP setup](HTTP_API.md#reviewed-collective-workflow-policy).

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**719 passed across 38 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The focused real-process test also passes. Its initial restricted-sandbox
run could not bind TCP (`EPERM`); the approved host-socket retry and full suite
passed. Existing socket, database migration, mock inference, malformed frame,
policy denial, cancellation, continuation and rollback regressions remain green.
Core protocol/generated bindings are unchanged; bundled exact-pairing/integrity
checks passed. `flatc` is unavailable, so fresh generation was not run. No live
model, GPU or independently held-out effectiveness claim follows from this change.


### Plan reviewed resident-model reconciliation — 2026-10-17

**Implemented (bounded operator planning/application); Partial (autonomous/physical lifecycle).**
`src/residency/planning.rs` plans a desired installed model/context set, reuses exact
loaded matches, loads only missing models and releases only explicitly selected
instance IDs. Canonical ordering and projected inventory hashes make decisions
reproducible. Invalid targets, incompatible unreleased contexts, expired/tampered
plans and stale inventories fail before mutation. Requests/actions are bounded to
eight; a 300-second review lifetime and 120-second mutation phase bound execution.

`residency-plan REQUEST --api-base ...` reads native inventory without initializing
state. `residency-apply PLAN --api-base ... --human-approve` uses an existing store,
rechecks explicit endpoint/inventory and persists an atomic one-attempt receipt.
Each step rechecks operator policy and expected inventory, audits its child operation,
and requires only the expected projected transition. Drift/uncertainty stops later
operations without retry or claimed rollback. Parent reports preserve confirmed
steps and explicit non-attestation flags. Existing credential/response/endpoint
limits remain; no new provider API or model actor grant is added.

All residency mutations now acquire configured-resource ownership before runtime
ownership, blocking changes under an idle live pool as well as active inference.
Retention protects plan receipts and linked operation tasks against replay-gate
removal. Receipt inspection is bounded to 1,024 records/32 KiB each under a five-second
SQL deadline. Retirement/compaction remains unimplemented and this quota is explicit.

Sources include residency/planning, existing mutation refactoring, CLI and retention.
Unit and actual HTTP/CLI fixtures cover reuse, exact unload/load, independent next-step
checks, expiry/content changes, corrupt/replayed receipts, no-state preview, approval,
audit rollback, unknown outcomes, idle ownership and retention. Native provider/weight
ownership, real load/inference cost, automatic per-task residency and physical GPU
limits remain unproven/incomplete. Jev+ and Jev-R remain Partial. Verification
passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**629 tests across 31 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. The first
five focused integration cases passed before adding two more CLI/drift cases and
expiry/receipt regressions; the final full suite includes all of them. Local
TCP/Unix fixtures required elevated socket access. Existing protocol pairing,
malformed frames, migrations, mock inference, denial, cancellation, recovery and
candidate rollback gates remain green. `flatc` is unavailable; schemas/bindings
were unchanged and fresh generation was not run. No real native load/GPU/quality
performance gain is claimed.


### Restore original configured resource admission during continuation — 2026-10-16

**Implemented (supported ready checkpoints and compiled configured pools); Partial (general recovery).**
Continuation now restores original `resource_admission` settings, working allocation,
resident catalog and target checks instead of refusing every configured task. A
bounded strict validator requires complete per-call intent/admission/dispatch/
completion chains, matching roles, identities, capacities, resident binding and
coverage. After reasoning context, targets must also match the pinned default/role
configuration. Earlier completed advisory targets must match declared coverage;
their untrusted results are not replayed. Unresolved workspace dispatches block
restoration. Native pools absent from execution settings remain unavailable.

Read-only preview validates history and probes resource ownership outside Store
locks. Approved resume performs a read-only preflight, restores/reuses a matching
compiled pool outside Store locks, then revalidates configuration and checkpoint
in the write admission transaction. Both preparation phases share one five-second
admission deadline. Original task deadlines, calls, output/prompt/tool/cycle charges
and model/skill snapshots remain in force. No fresh task budget or executable is
loaded from metadata. Resource admission and its original history are included
in continuation reports and start audits; the restored pool is attached to Session
before any missing graph/solver work.

Sources: `src/recovery/continuation_resources.rs`, continuation orchestration,
`src/resource_scheduler/{configured,resident}.rs` and shared binding helpers.
`tests/resource_resume.rs` exercises actual HTTP, actual direct/managed CLI resume,
current-config drift, ready cancellation, admission rollback, initial solver,
partial advisory graph, legacy empty catalogs, ownership/approval denial and 13
corruption/unresolved-history mutations. The seven focused tests pass. The expanded
fixture initially missed a `mut` binding; corrected without production changes.

Only fully recorded ready checkpoints are supported. Old empty-catalog records may
omit all resident metadata fields; partial metadata is rejected. Nonempty catalogs
require durable target checks. Uncertain in-flight calls cannot be refunded or
approved by a resource-release acknowledgement alone. General custom executable
pool recovery, provider stop/weight attestation, physical limits, dynamic lifecycle
and independent model-efficiency evidence remain open. Jev+ and Jev-R remain
Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**617 tests across 30 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Local
TCP/Unix fixtures required elevated socket access. Existing protocol pairing,
malformed frames, migrations, mock inference, denials, cancellation and candidate
rollback regressions remain green. `flatc` is unavailable; schema/binding files
were unchanged and fresh generation was not run. These fixture runs do not
establish real-model quality, GPU efficiency or provider-stop guarantees.


### Bind resident declarations to selected inference targets — 2026-10-16

**Implemented (trusted adapter/configuration coverage); Partial (provider/weight attestation).**
Nonempty resident catalogs now require the selected adapter to identify its actual
configured endpoint/model (including role overrides), or explicitly identify a
compiled deterministic backend. Unknown, malformed and unlisted targets fail before
pool queueing, provider contact or managed worker launch. Empty catalogs retain
legacy behavior without invoking new metadata callbacks.

`InferenceBackend::inference_target` and `ChatBackend::inference_target` default to
unknown. Local and same-image managed adapters derive metadata from their immutable
configuration; native conversations use the actual native model rather than role
profiles. Pinned Fabric routes and scheduled wrappers forward the selected module's
metadata. Scheduled dispatch pins a dynamic broker before checking coverage, so a
registry replacement during observation/waiting cannot substitute another backend.
Resident consumers now share the bounded canonical `backend::ModelTarget` type.

Target intent precedes the trusted metadata callback. Coverage/denial audit must
commit before resource reservation or execution. Accepted records bind the canonical
target, resident identities and catalog digest with no weights/permission attestation;
rejected records exclude raw invalid target/error data. Metadata callbacks run
outside Store/pool/registry locks. Task charges already committed by Session remain
charged on denial; there is no retry, alternate route, budget refund or extra grant.

Tests cover reentrant callbacks, unknown/unlisted/invalid targets, audit failures,
unchanged empty-catalog behavior, role selection/canonical endpoints, route mutation
without target substitution, and actual CLI refusal before HTTP/worker launch.
Existing native/collective HTTP and bound-expert coverage exercise listed targets.
The existing resident-test doubles now explicitly declare their synthetic target.

This checks declared configured-target coverage, not provider-returned model labels,
actual loaded weights, catalog memory completeness or real backbone sharing. Native
operators can misdeclare metadata; raw Unix/custom adapters remain unknown unless
the host implements the contract. Dynamic catalog/lifecycle integration, measured
residency, physical limits and independent real-model studies remain open. Jev+
and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**610 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Full tests
required elevated local TCP/Unix fixture access. An initial new CLI fixture wrongly
expected exit success for a failed task; corrected to assert the existing nonzero
exit and retained charges. New native error assertions were also corrected for the
existing non-Debug TurnResult type without changing production types. Final protocol
compatibility/malformed frames, migrations, mock inference, denial, cancellation,
recovery and candidate rollback regressions passed. `flatc` is unavailable;
schemas/bindings were unchanged and fresh generation was not run. No real-model
quality, physical memory or GPU-efficiency result is claimed.


### Resident backbone memory admission — 2026-10-16

**Implemented (operator-declared, pool-lifetime memory accounting); Partial (measured/dynamic residency).**
Optional `resource_admission.residents` declares immutable backbone IDs/versions,
weight digests, host/device footprints and consumer endpoints/model names. The pool
reserves each unique declared backbone once before request admission; multiple
consumers do not multiply its cost. Working requests must fit the capacity remaining
after resident charges. Completion, cancellation, quarantine resolution and shutdown
never refund resident memory. Snapshots separate total, resident and working charges.

Bounded validation rejects duplicate/conflicting identities, consumer aliases,
malformed versions/digests, oversized catalogs and impossible resident-plus-working
allocations. Canonical catalog/consumer ordering makes pool identity insensitive to
input order; live catalog changes are rejected. Existing empty/omitted catalogs keep
legacy configuration bytes. Each resource intent records the canonical catalog hash
and resident total with `residency_attested:false`; configured execution snapshots
retain the declared catalog. No model load, grant, endpoint contact or weight read
is performed by declaring a catalog.

Implementation: `src/resource_scheduler/{resident,resident_tests}.rs`, pool/state
initialization, configured pooling and admission metadata. Coverage includes shared
consumer accounting, actual memory-constrained FIFO dispatch, cancellation/release,
live-catalog pinning, invalid metadata, CLI validation/execution, primary/bound expert
calls and mixed HTTP native/collective contention. The HTTP fixture now has two
compute slots; the resident memory charge is what prevents simultaneous dispatch.

This enforces declared arithmetic, not measured provider RSS/VRAM or actual weight
sharing. Consumer declarations do not attest that every runtime backend/model is
covered. Automatic load/unload, measured catalog completeness, provider ownership,
physical isolation, task-pool restoration and independent real-model efficiency
studies remain open. Jev+ and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**603 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Focused
resident tests also passed all four cases. Full tests required local TCP/Unix
fixture access outside the restricted sandbox. Existing protocol pairing, malformed
frames, migrations, denials, cancellation, recovery and candidate rollback gates
remain green. `flatc` is unavailable; schemas/bindings were unchanged and fresh
generation was not run. No live-model quality or GPU efficiency gain is established.


### Configure and operate Jev+ shared admission — 2026-10-16

**Implemented (configured workspace and trusted-host scope); Partial (physical/global GPU scheduling).**
Optional `Config.resource_admission` validates fixed working allocations and bounded
pool settings. `configure-resources FILE` / `--disable` updates operator config
without database bootstrap. Engine clones share one retained pool; Session applies
it to every role and bound-expert call, including managed inference. Native agent
turns use the same executor, and HTTP Server construction shares the Engine pool
across both request types. Original task budgets/charges/deadlines remain in force.

Configured pools hold a private exclusive workspace resource lease. Another
configured host cannot start fresh counters while the owner is live. Bounded strict
lifecycle validation rejects unresolved prior dispatches at startup. Human-only
`resource-status` reads an existing database; `resource-release ID --human-approve`
takes resource/runtime leases, validates exact identity and records an operator
stop acknowledgement before removing the restart gate. No provider stop is inferred.
Retention preserves unresolved resource audits and excludes live configured owners
when applying deletion. Original configured-task continuation still requires explicit
restoration and is denied, even before a first resource intent exists.

Implementation: `src/resource_scheduler/{configured,admission,adapter}.rs`, Config,
Engine, Session, AgentRuntime, CLI, HTTP Server, retention and continuation guards.
Tests cover real CLI configuration/reconciliation, managed process ordering/reaping,
primary/bound calls, shared native/collective requests over actual loopback HTTP,
workspace ownership, audit tampering and retention. The first new HTTP fixture
compile exposed a missing `IntoFuture` import; fixed in the test. Historical config
bytes retain the omitted optional field. See `docs/RESOURCE_SCHEDULING.md`.

General cross-workspace/device arbitration, resident/backbone accounting, physical
VRAM/RSS enforcement, task-pool restoration and independent reasoning/efficiency
studies remain open. Jev+ and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**598 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Socket
fixtures required elevated local TCP/Unix access. Both ordinary mock and managed
inference, protocol compatibility/malformed frames, migrations, denials, cancellation
and candidate rollback regressions are included. `flatc` remains unavailable;
schemas/bindings were unchanged and fresh generation was not run. No real-model
quality/latency improvement, GPU enforcement or general reasoning superiority is
established by these fixture runs.


### Shared Jev+ inference resource admission — 2026-10-16

**Implemented (trusted native, process-local scope); Partial (general GPU scheduler).**
`src/resource_scheduler.rs` and `src/resource_scheduler/adapter.rs` add a shared
FIFO pool across independently registered inference backends/brokers. Fixed
operator allocations reserve compute slots, host bytes and device bytes together;
queue and execution deadlines, bounded inventory and scoped cancellation prevent
unbounded pending work. Backend pins retain original allocations and transport
identity. Per-task kernel budgets and charges remain enforced independently.

Intent/admission/dispatch audits precede provider work. Success requires a durable
completion before freeing capacity; failed/timed-out/dropped dispatches quarantine
the allocation. Only an explicit audited operator stop acknowledgement can release
it. No prompt/answer/credential/error body is included in pool events. Native-only
`resources.configure` and `resources.resolve` reject model actors. Default recovery
rejects original pool usage instead of silently continuing without its limits.

Tests in `src/resource_scheduler/tests.rs` and
`src/recovery/continuation_policy_tests.rs` cover three-dimensional contention,
FIFO fairness, cancellation races, queue overflow, deadlines/shutdown, audit
failures, preserved transport pins, independent Engine brokers, original charges,
and pool-bound recovery denial. Initial compilation identified a private role
helper and a MutexGuard dereference; both were corrected using the existing role
mapping without duplicating authority. See `docs/RESOURCE_SCHEDULING.md`.

Settings and allocations are host declarations, not physical GPU/RSS enforcement.
Default CLI/HTTP configuration, resident/backbone accounting, provider ownership,
durable/cross-process allocation and pool restoration remain open. Jev+ and Jev-R
remain Partial; no model training, quality gain or live GPU benchmark is claimed.
Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**589 tests across 27 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Full tests
required elevated local TCP/Unix fixture access. Existing protocol pairing,
migrations, malformed-frame, denial, cancellation and rollback regressions remain
green. `flatc` is unavailable; schemas/bindings are unchanged and fresh generation
was not run. The focused first pass also passed all 11 then-present pool tests;
final tests include subsequent cancellation/state/recovery regressions.


### Restore pinned executive policies during continuation — 2026-10-16

**Implemented (supported checkpoint and trusted-host scope).**
`recovery::continuation::{preview_with_escalation_policy,resume_with_escalation_policy}`
accept an explicitly restored native policy. Every saved descriptor must match;
strict parsing rejects ambiguous policy snapshots. A supplied handle cannot claim
legacy compatibility merely by using the built-in descriptor. For a revised
solver, restoration additionally requires the original durable intent/allow pair
bound to candidate, mode, cycle, checker, original configuration, compiled prompts,
and current source fingerprint, with exact prompt-chain costs and sufficient
recorded remaining resources. Missing/denied/altered allowances fail before
backend construction. An existing allowance is validated, not invoked again.

The restored handle is carried into the Session, so subsequent unresolved answers
use the original policy under original budgets and independent checking. New
CLI `--retry-study` and `--retry-thresholds` options on `resume-preview` and
`resume-task` reconstruct only the compiled empirical policy from bounded
operator-selected data; there is no executable loader, implicit policy swap or
promotion. Resume approval is checked before file/configuration access. The CLI
opens an existing current-schema database without creation, migration or bootstrap.
Default/legacy recovery remains available through the existing built-in path.

Sources: `src/recovery/continuation.rs`, `src/recovery/continuation_policy.rs`,
`src/escalation.rs`, `src/escalation/empirical.rs`, `src/main.rs`, `src/store.rs`.
Regressions in `src/recovery/continuation_policy_tests.rs` and
`tests/policy_resume.rs` cover native and actual CLI/HTTP restoration, immutable
study identity, reentrant callbacks, missing approvals, live leases, audit rollback,
ready cancellation, original call/output charges, no replay, and legacy/schema/
symlink/source-drift denials. The CLI fixture initially attempted a private Store
method and was corrected to use its own SQLite connection; a lint correction
made its test-lock scope explicit without weakening production checks.

Jev+ and Jev-R remain Partial. General custom reconciliation, executable policy
attestation/provisioning, automatic policy catalogs, global GPU scheduling and
independent utility studies remain open. There is no new model grant or claimed
quality improvement. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**574 tests across 27 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. The full
test run required local socket access outside the restricted sandbox. Tests use
scripted/loopback backends; no fresh real-model quality or speed result is claimed.
Schemas were unchanged; `flatc` remains unavailable, so fresh binding generation
was not run.

### Empirical retry evaluation and executive cost gate — 2026-10-16

**Implemented (bounded host API and operator inspection); Experimental (utility
policy).** `src/escalation/empirical.rs` aggregates reviewed single-retry outcomes,
including failures, and uses exact success-rate/cost-per-success thresholds.
Task requests bind mode, cycle, compiled criterion, effective configuration,
role prompts and runtime-source fingerprint. Matching evidence can veto or admit
one solver/checker retry under the existing deterministic kernel limits; unknown
scope, unverified prose, higher cost or insufficient time decline further work.
No model/task permissions, model loads, resource ceilings or recovery approvals
are widened. `retry-study-evaluate` inspects bounded operator-selected files
before any state initialization; only the operator receives `evaluation.retry`.

Study/threshold content fixes the immutable policy identity. Build-time source
hashing includes Rust, schemas/bindings and locked build inputs. Hashes establish
local byte consistency, not executable/weight attestation. Review and inclusion
of failures remain operator-attested. Public workflow reports stay frozen and
are not relabeled as held-out studies. No real utility gain is established.
Custom policy recovery still refuses unavailable policies rather than replacing
them. Jev+ remains Partial: general GPU scheduling, independently measured
routing/utility and safe executable provisioning remain open. See
`docs/EMPIRICAL_ESCALATION.md` for all six component statuses and exact bounds.

Regressions: `src/escalation/empirical/tests.rs`, `tests/retry_study.rs` and
`tests/runtime_fingerprint.rs` cover exact threshold edges, all failure costs,
scope and source drift, budget denial, ambiguous/unreviewed data, immutable
identity, actual Engine admission with independent rechecking, CLI no-state
inspection and symlink/actor denials. Formatting and workspace all-target check
passed. All 562 tests across 26 targets passed with local TCP/Unix fixture access;
the restricted-sandbox attempt first failed 99 socket tests with EPERM (397 passed).
The reviewed FlatBuffers bundle/pin checks, migrations, malformed frames, policy
denials, cancellation and rollback regressions are included. Native flatc is
unavailable, so exact regeneration was not repeated; protocol files are unchanged.
Warnings-denied workspace Clippy and final whitespace review passed. No fresh
live-model study, neural training, GPU attribution or quality gain is claimed.

### Replaceable executive escalation policy — 2026-10-16

`src/escalation.rs` introduces a trusted versioned `EscalationPolicy` installed
through `Engine::with_escalation_policy`. After hard resource admission, typed
hash/metadata requests can allow or decline a revised solver or an unverified
critic/reflection/solver sequence. A policy cannot grant tools, raise limits or
change evidence. Future independent verification must fit the tool allowance
before another rejected-answer solver is bought. No new model, external probe or
always-resident inference backend is introduced.

Reasoning snapshots identity and audits intent before callbacks and validated
metadata decisions before extra inference. Store reentry is lock-free; failures
and invalid decisions fail closed. Independent success, repeated-answer stopping
and baseline profile isolation precede custom policy use. Recovery refuses
unavailable custom or altered/spoofed policy snapshots; matching built-in and
legacy built-in tasks remain eligible under normal admission. Native callbacks
are trusted, nonblocking host code, not sandboxed or implementation-attested.
See `docs/ESCALATION.md`, `src/engine.rs`, `src/reasoning.rs` and recovery tests.
Jev+ remains Partial: measured utility, custom-policy restoration and general
resource scheduling are not established by this interface.

Verification: 551 workspace tests (490 library including server, 61 integration),
formatting, workspace all-target check and warnings-denied Clippy passed with
`FLATC` unset. Seven new regressions cover policy and recovery boundaries. Early
test-fixture corrections supplied the proper Store search argument, used explicit
non-demo inference status, and isolated checker budget from Full-profile retrieval.
Production permission checks and acceptance thresholds were not weakened.
Prior source `4974acb` and installation-evidence source `ad00a3e` passed hosted CI;
this new feature is not yet hosted/optimized-install/live-model verified.

### Capacity-aware admission to module routes — 2026-10-16

New inference and arithmetic/SQL pins exclude modules whose bounded queue is
observed full. Among candidates with room, operator rank still dominates sampled
queued/processing load and ID breaks ties. A shared queue-pressure sampler reads
available slots once per candidate; selection never reserves a slot. Existing
pins retain their exact module/version: inference still rejects a full queue,
while tools retain deadline-bounded waiting. No dispatch is replayed or refunded.

Regressions fill four buffered slots behind a gated active call, select an approved
higher-rank alternative for new pins, refuse selection when all available routes
are full, and restore preferred routing after draining. Both arithmetic and SQL
are exercised, including cancelled waiting on an old handle. Existing rank,
pin, failure, permission and callback-lock tests remain. This advances local
scheduling; it does not establish global provider/GPU capacity, fairness or a
general module scheduler. The first tool test incorrectly waited for the full
30-second dispatch deadline; it was corrected to observe/cancel the existing wait
without changing production backpressure or limits.

Verification: 544 workspace tests (483 library including server, 61 integration),
formatting, all-target workspace check and warnings-denied Clippy passed with
`FLATC` unset. Sources: `src/fabric.rs`, `src/fabric/tools.rs`; regressions:
`src/fabric/tests.rs` and tool tests. Previously installed `df4cf05` evidence remains
source-scoped; this new routing source is not installed/live-model proven yet.

### CLI and managed-worker revision recovery — 2026-10-16

`tests/resume_worker.rs` now exercises approved `recover-revision-result`, read-only
preview and resume through actual CLI processes for saved solver, critic and
reflector phases. Each phase starts exactly one bounded same-image inference
worker, preserves original `reasoning_effort=none` despite changed current settings,
performs only missing calls, and reaps its child. Successful outcomes remain
unverified with five total calls / 1280 reserved outputs. Three SIGINT cases retain
one unknown result and all charged reservations, close the fixture connection,
reap the child and leave no unfinished worker audit session. No provider/GPU stop
or live-model quality is established by a loopback fixture.

The suite can target an explicitly installed executable via
`COGFABRIC_TEST_INSTALLED_BINARY=/absolute/path/to/cogfabric cargo test --test resume_worker`.
The override applies only to integration-test process launches, requires an
existing absolute file, and adds no production setting or authority. Earlier
Core-only revision evidence is now supplemented with real CLI/worker coverage.

Verification: all 542 workspace tests (481 library including server, 61
integration), formatting, all-target workspace checking and warnings-denied Clippy
passed with `FLATC` unset. The first test build referenced a private role allowlist;
the fixture was corrected to explicit known role names without changing visibility.
Optimized installation verification is the next separate evidence step.

### Recover a recorded whole-goal solver before critique — 2026-10-16

Approved `recover-revision-result` now reconciles a recorded cycle-positive solver
answer into `ready_critic` for unstructured DEEP/RESEARCH tasks. Normal admission
checks original snapshots, stopping hash, schedules, same-boot deadline and enough
remaining capacity for critic/reflection/revised solver (three calls and 640
reserved outputs, plus bounded prompts). No prior inference or cycle is replayed.
The answer remains unverified; a non-admitting or invalid critic stops with the
saved answer and only one new critic charge. An admitting critic proceeds through
the missing reflection and solver under original limits.

HTTP regressions cover the three-call path (five total / 1280 reserved outputs)
and both critic stop cases (three total / 768 reserved outputs). Additional tests
cover exact call/output/input budget refusals without writes, and another crash
after a recovered critic: a recorded result can form `ready_reflector`, whereas
an absent result retains unknown outcome and denies recovery. Other-boot,
unknown inference outcomes, and arbitrary backend/subtask recovery remain partial.

Verification: 540 workspace tests (481 library including server, 59 integration),
formatting, all-target workspace check and warnings-denied Clippy passed with
`FLATC` unset. Initial stop-test failure was an incorrect trace-field lookup;
assertions now read the persisted nested task answer and pass for both stop cases.
Hosted run `38827055986` passed for prior source `71f5e1f`; run `38827410095`
for `44706cd` was still running when checked. New source is not yet hosted-proven;
no new optimized installation or live-model claim is made.

### Continue after a recorded critic — 2026-10-16

`recover-revision-result --human-approve` now also reconciles a completed
same-cycle solver/critic pair into `ready_reflector`. Strict critique content
must match the checkpoint. Continuation revalidates that pair and reserves no
new allowance: at least two original calls, 512 output tokens and bounded prompt
capacity must remain for reflection plus a revised solver. Original settings,
module/model pins, stopping hash, schedule and aging same-boot deadline still
apply. Resume invokes only the missing reflector and solver; no prior inference
is replayed and the outcome remains unverified.

Tests cover actual HTTP (two new calls, five total / 1280 reserved outputs),
insufficient call/output/input allowance, changed critique reason, and a second
crash after recovered reflection. A recorded reflection can form `ready_solver`;
a reserved reflection without its result still fails closed without writes.
This extends the completed-reflection path; arbitrary within-cycle positions,
unknown provider outcomes and other-boot recovery remain incomplete.

Verification: 536 workspace tests (477 library including server, 59 integration),
formatting, all-target workspace checking and warnings-denied Clippy passed with
`FLATC` unset. The previous runtime's hosted run `38827055986` was still running
when checked; no new hosted success, installation or live-model claim is made.

### Reconcile a recorded revision handoff — 2026-10-16

`recover-revision-result TASK --human-approve` links a fully recorded same-cycle
solver/critic/reflector sequence to the next solver boundary without inference
replay. The prior boundary, consecutive calls, strict admitting critique and
original stopping-state hash are checked. Normal continuation admission validates
all original snapshots, remaining budget and same-boot deadline before committing
the new boundary and operator audit together. Missing, altered or unresolved
results, non-admitting critiques, live leases and terminal tasks fail closed.
Structured independently checkable objectives retain their separate result path.

The report omits answer/strategy text and claims no verification or resume grant.
Actual local HTTP continuation uses the recorded reflection with exactly one new
solver call, five total calls and 1280 reserved outputs. The answer stays
unverified. CLI denial precedes state initialization; audit-write failure rolls
back the boundary. The first broad test pass exposed missing original stopping
hash validation; that check was added before final verification. General unknown
provider outcomes, recovery after only a critic, and other-boot/consumed-cycle
positions remain unsupported. No permission, call, deadline or cycle is renewed.

Verification: all 533 workspace tests (474 library including server, 59
integration), formatting, workspace all-target checking and warnings-denied Clippy
passed with `FLATC` unset. The preceding graph commit's hosted run `38826523808`
failed the existing paused-clock idle-worker/socket test; test-only commit
`5c41eed` resumes real time after simulated idle, preserving production limits.
The focused idle test and full suite were rerun with that fix. Hosted verification
of the new source remains pending; no fresh live-model or installation claim.

### Recover interrupted graph initialization — 2026-10-16

Approved `resume-task` now initializes missing pending nodes after a saved planner
result and graph, including a partially persisted initialization. Preview reports
`initial_nodes_to_record` and leaves state untouched. Admission still validates
original model/module/context/settings, charged planner identity, committed
schedule, same-boot deadline and remaining budgets. Missing nodes alongside any
advanced node state remain rejected. Missing transitions and the continuation
start commit in one lease-protected transaction before worker/backend setup.

Regression coverage includes no-approval refusal, read-only preview, write failure
after an earlier initialization write (full rollback), missing state after prior
execution, and actual local HTTP continuation of both crash windows with custom
schedule. Exactly three new solver calls finish the task; the original planner is
not replayed and total reservations stay four calls / 1024 output tokens. No
permissions, deadline or resource allowance is renewed. Unknown provider outcomes,
other-boot continuation and general consumed-cycle recovery remain incomplete.

Verification: all 529 workspace tests (471 library including server, 58
integration), `cargo fmt --all -- --check`, workspace all-target check and Clippy
with warnings denied passed with `FLATC` unset. Checks used locked cached
worktree dependencies; no new live-model or installed-binary result is claimed.

### Include executing arithmetic and SQL work in routing — 2026-10-16

Equal-rank tool candidates now compare buffered requests plus the one currently
executing request before the deterministic ID tie-break. A drop guard clears the
processing observation on completion, cancellation, errors, shutdown and worker
unwinding. Both arithmetic and SQL regressions verify empty-buffer busy routing,
unchanged old handles, cancellation/completion cleanup and return to the original
tie-break; existing rank-priority/queued-load tests remain. Worker-panic tests also
assert cleanup. All 527 workspace tests (469 library including server, 58
integration), formatting, all-target checking and warnings-denied Clippy pass with
`FLATC` unset. No protocol, permission or queue ceiling changed. This is local
sampled scheduling, not reserved capacity, provider/GPU occupancy or a general
module scheduler. Previous installed and live evidence remains source-scoped.

This is the implementation snapshot, not the target specification. The complete
French vision is preserved in `ARCHITECTURE.md`. The branch is `develop/v1.0`,
package version `1.1.0-alpha.1`; it remains a pre-release despite its historical
version numbering. Rust 2021 and Tokio own the runtime; Python is offline only.

The seven responsibilities currently map to these modules:

| Layer | Implementation |
|---|---|
| Language Interface | `language.rs`, `backend.rs`, `config.rs`, `agent/`: advisory intent, explicit history, synthesis, loopback inference and bounded native conversation turns |
| Jev+ | `engine.rs`, `planning.rs`, `routing.rs`: task lifecycle, replaceable trusted expert selection, capability gaps, bounded dependency scheduling |
| Jev-R | `reasoning.rs`: shared-budget model roles, revisions, stagnation detection and independent verification status |
| Expert Network | `registry.rs`: immutable reference manifests, dependencies, lifecycle and task version snapshots |
| Cognitive Fabric | `wire.rs`, `service.rs`, `store.rs`, `agent/protocol.rs`: typed owner-local IPC, durable state/evidence and SQLite/FTS5; optional `crates/cogfabric-server` HTTP transport builds on the core |
| Evolution Engine | `evolution.rs`, candidate storage: quarantined immutable artifacts and explicit lifecycle |
| Security & Evaluation Kernel | `security.rs`, `evaluation.rs`: task-bound dispatch authorization and independent deterministic gates |

Inference now traverses a bounded FlatBuffers Tokio broker with runtime-owned
registration, discovery, actor/task validation and pinned adapter versions. Other
layer interactions use direct Rust interfaces. Typed task progress now supports bounded local subscriptions and durable catch-up.
General module events and independent process registration/authentication remain incomplete.
Reference experts are functional; they are not loaded neural specialist models.

A task snapshots active skill metadata and retrieved content before inference.
Planner JSON may describe up to four acyclic dependent subgoals; ready nodes run
sequentially under shared ceilings, followed by an original-goal solver. Invalid
or unaffordable plans fall back to whole-goal execution. Critic advice can trigger
bounded reflection/revision. Model claims and subtask answers remain unverified.
The arithmetic parser provides an independent verified execution criterion.

The local service authenticates the owner UID, caps pending connections at eight,
and permits one active task. Typed controls can query status, cancel current work,
or stop/join the service. Active task UUIDs enable targeted cancellation and
bounded progress pages over the socket. Socket disconnects also cancel work durably. It is a
single-user local service, not a multi-tenant sandbox.

SQLite migrations preserve alpha data through schema 6. RAG candidate imports use
content-addressed quarantine, explicit provenance, quotas and integrity checks.
Quality requires operator-attested held-out data. Promotion checks quality,
cumulative approved-version retrieval regressions and unchanged versions transactionally.
Rollback restores approved stored snapshots. Neural promotion stays disabled.

Current verification (2026-10-14): 526 workspace tests, formatting, all-target
checking and Clippy pass. Bundled generated protocol sources and the Rust runtime
are pinned to FlatBuffers 25.12.19; default builds require no installed compiler.
Current CLI/service arithmetic and real Qwen generation smokes pass. Public graph
and Rust diagnostics measure particular workflows; no independent held-out quality,
weight attestation, hot-swap, energy or model-superiority claim follows.
See `IMPLEMENTATION_MATRIX.md` for every remaining engineering requirement and
`QUALITY_REVIEW.md` for actual command evidence.

Optional managed worker mode now launches the same Rust executable lazily per
task, provisions authenticated typed settings and closes/reaps it on completion
or cancellation. Three actual CLI integration tests cover demo role execution,
zero-process arithmetic and Ctrl-C during pending HTTP inference. This is process
coordination, not an executable sandbox or model-loading proof. See `WORKERS.md`.

Persistent workers also hold shared runtime leases. Operator-only recover-workers
marks open audit sessions interrupted under exclusive ownership, without task
result changes, PID killing or inference replay. Tests include an actual child
crash and transactional rollback; stale temporary-directory cleanup remains open.

`benchmarking.rs` now runs explicit execution ablations under common operator
ceilings, rotates arm order, excludes expected answers from inference, records
actual resource totals and cancels durably on a run deadline. The diverse study
dataset and live-model comparison remain outstanding.

Operator-published episodic memory and independently recomputed arithmetic snapshots
are retrievable with provenance, hashes, revocation and bounded payload storage.
See `MEMORY.md`; arbitrary model output remains unverified.

Persistent Engine tasks hold kernel execution leases. Operator `recover-tasks`
marks abandoned running tasks interrupted in an audited transaction; it refuses
live leases and never automatically replays work. See `RECOVERY.md`.

Immutable local model registrations now record source/license, connection identity,
declared limits and optional unverified weight hashes. Operator selection persists
an exact version reference; task traces pin it and disabled registrations reject
new tasks. Physical model residency remains server-owned; see `MODELS.md`.

Typed progress events use a dedicated FlatBuffers schema, with bounded subscriber
queues, explicit gap reporting and cursor-based durable catch-up (`EVENTS.md`).

SQLite page limits, transactional global task admission and explicit preview/apply
retention bound logical storage growth. Memory/conversation sources are protected;
WAL physical size remains outside a hard runtime quota (`STORAGE.md`).

Comparison arms now share a consistent read-only SQLite retrieval snapshot, with
size/hash/timing recorded in reports and durable task traces (`corpus.rs`). Live
results persist separately; live model availability/weights remain unfrozen.

Explicit `order:` objectives now use an independent bounded Rust constraint checker.
Each check is individually authorized and charged against the tool budget; failed
checks inform subsequent solver cycles. Passing checks produce completed outcomes
and suppress answer rewriting. See `VERIFICATION.md` for syntax and exact scope.

Operator `diagnostics` reports storage/schema/registered-selection checks without
creating a task. Optional bounded local model discovery reports unavailable or
missing model IDs and produces a nonzero CLI exit; it does not perform inference
or attest weights. See `DIAGNOSTICS.md`.

Recorded capability gaps now support bounded operator inspection/preparation and
quarantined gap-guided RAG imports. Candidate metadata and its evidence link commit
atomically after revalidating the recorded gap hash; source tasks are protected from
retention. Automatic source discovery/cleaning remains absent. See `EVOLUTION.md`.

Module brokers measure per-call queue waiting independently of backend reports.
Optional queue timing crosses module/owner IPC and accumulates in task/comparison
resources; cancelled partial totals remain unavailable. GPU/load timing is absent.

Expert manifests now include bounded prompt/workflow checklists. Full/experts
execution can invoke one routed, version-pinned workflow through the shared solver
backend, reserving budget for whole-goal reasoning. Advice is unverified and grants
no permissions. Bounded multi-step and registered-model advice are implemented; executable
experts and measured domain usefulness remain incomplete.

Operator SQLite backups now use a pinned read view, bounded copy/hash loops,
private files and serialized admission. They preserve committed WAL state and
publish a checksummed read-only standalone database. Full artifact/configuration
restore and automatic replay are not implemented. See `STORAGE.md`.

Offline training preflight now has nine standard-library tests, frozen input hashes,
strict provenance/deduplication, a no-supervised-token termination guard, disjoint
new output and bounded adapter inventories. This improves the experimental script;
no actual ML training, isolation, neural evaluation or promotion is proven.

Explicit `sql:` tasks now execute bounded read-only queries over fresh task-local
inline tables without model calls. Task permissions, worker/VM/data/output bounds,
denied external access and hashed verification evidence are covered by tests.
This built-in capability is distinct from dynamically loaded external experts;
see `SQL.md` for its limited verification scope.

Workflow experts now accept bounded acyclic advice-step graphs as an alternative
to legacy text checklists. They execute dependencies with the shared backend and
task budgets, record step identities/hashes and preserve pinned versions. Results
remain unverified advice; measured model usefulness and executable workflows are
not proven.

`restore-database` now restores a saved-hash-verified schema-6 SQLite snapshot into
a new private directory and records provenance. Existing workspaces, corrupt or
live-WAL inputs and incompatible schemas are refused. Running tasks are preserved
without replay; external artifacts/configuration and older-schema migration are
not included.


Comparison reports disclose per-case bound expert model snapshots and flag the
loss of the same-model configuration control, including refused/failed expert
advice. This prevents separately configured experts being described as ordinary
same-model ablations; backend weights remain unattested.

Offline training also requires reviewed hashed train/validation/test split
metadata and rejects normalized prompt overlap before ML imports. Split identity
is recorded, but semantic independence and neural acceptance remain unverified.

SQL comparison cases can use structural JSON result scoring against reviewed
expected labels, preserving cell types and row/column order while ignoring object
field order and whitespace. Status alone cannot make a wrong answer pass.

The evaluation directory now supplies a provisional 100-case authored corpus and
hashed inventory. Ten categories include deterministic tools and ambiguity/policy
reasoning. Independent review, held-out status and live performance remain unproven;
no benchmark accuracy or security result is inferred from format/oracle tests.

Optional decomposition/exploration now preserves the first whole-goal solver's
call, output-token and prompt-byte capacity. Conservative dependency escaping
bounds and audited skips prevent advisory stages consuming its reserved budget.

Optional operator GPU diagnostics use one fixed bounded NVIDIA query and report
whole-device observations separately from task metrics. The actual driver is
unavailable here; live GPU values, energy and task attribution remain unverified.

Operator diagnostics can inspect enabled registered model endpoints with bounded
deduplicated discovery, including potential workflow experts. Advertised IDs are
not loaded-weight or successful-generation evidence. Local model files were found,
but LM Studio's local API startup timed out; real inference remains unverified.

Comparison CLI supports an optional reviewed dataset hash pin, rejecting changed
bytes before execution and recording the operator pin separately from actual
content identity. This does not establish held-out independence or label quality.

A later host-access check succeeded where sandboxed checks failed: actual framework
GPU telemetry sampled a 12 GiB device, and LM Studio loaded an existing quantized
model for a bounded test. Its authenticated HTTP server returned 401 for discovery
and generation; no successful model answer is claimed. The owned test model was
unloaded and the server returned to its prior stopped state. Protected explicit
operator API credentials remain an integration gap; none were accessed or stored.

Opt-in local bearer authentication now binds the dedicated operator environment
credential to one local API scope, including managed workers and discovery.
Synthetic direct/managed generation and escaped-echo regression tests pass.
Live authenticated generation remains unverified; see `AUTHENTICATION.md`.

New database backups include bounded effective configuration under the same
checksum. Restore defaults to database-only; `--restore-configuration` explicitly
validates and creates private settings for the new workspace. External artifact
files and older-schema migration remain separate recovery work. See `STORAGE.md`.

Optional `backup --include-rag-artifacts` and restore `--restore-rag-artifacts` now
preserve checked RAG candidate files with private quarantine and rebased paths.
Candidate status and promotion boundaries are preserved. Neural and unrelated
external artifacts remain excluded; see `STORAGE.md` for scope and failure handling.

Recognized schema-1–5 snapshots now support explicit `--migrate-schema` restoration
through bounded existing migrations on a new staged copy. Source and destination
schema versions are recorded; original files and task state are preserved. Unknown
and unversioned snapshots remain rejected. See `STORAGE.md`.

Trusted embedding hosts can register versioned event-module descriptors and
subscribe to selected progress stages for one task. Handles pin filters/versions
and share bounded FlatBuffers queues with owner observers. Registry state is
process-local; cross-process actor provisioning remains incomplete. See `EVENTS.md`.

Full task trace reads now have event/raw-byte ceilings and strict JSON validation.
`task-events` exposes bounded complete audit pages with durable cursors for larger
histories. Comparison identity reporting avoids full-trace allocation. These changes
support safe recovery inspection without replay; see `RECOVERY.md`.

New reasoning results carry their charged call identity. `inference-audit` reads
bounded durable reservations/results without exposing prompts/answers, and keeps
legacy or unrecorded usage unavailable. It grants no resumption authority; see
`RECOVERY.md` for limits and remaining checkpoint requirements.

Independent structured verification now also supports integer sorting, in addition
to dependency ordering, arithmetic and inline read-only SQL. `sort:` goals declare
one to 256 signed 64-bit integers and optional descending order; Rust checks exact
multiplicity and ordering before verified completion. Failed candidates can be
revised within the existing cycles/tool/call ceilings. This is a bounded domain
checker, not a general correctness evaluator or a measured live-model improvement.

For exact explicit `sort:` and `order:` goals, Full/Memory execution can reuse an
operator-published episode only after a fresh independently charged Rust check.
A valid retrieved answer completes without inference; rejected answers remain
advisory and normal reasoning continues within remaining limits. Unrelated goals,
revoked entries, explicit conversation references and other ablation profiles do
not gain this shortcut. A memory's prior status is never accepted as proof.

`recover-verified-result` now reconciles abandoned explicit sort/order tasks whose
saved solver answer passes a fresh independent check. It requires exclusive
runtime ownership, a fully correlated bounded ledger and remaining verification
allowance, then atomically commits proof/status without repeating inference.
Unsupported or unresolved tasks remain interrupted; general continuation is still
incomplete. See RECOVERY.md for failed-check charges and transactional behavior.

The Expert Network now supports checker manifests binding reviewed expert metadata
to compiled integer-sort/dependency-order verifiers. Existing immutable lifecycle,
dependency validation and task pinning apply; actual proof events record the
selected manifest version separately from the compiled criterion version. No
imported code or new permissions are accepted. See EXPERTS.md.


Explicit `resume-task --human-approve` now resumes validated pre-solver advisory
work using original resource charges and same-boot deadline evidence. Completed
nodes remain advisory; pending nodes and whole-goal verification use fresh kernel
authorization. Default direct Full/Collective tasks are tested through local HTTP.
Managed/registered/custom backends and general multi-phase/repeated resumption
remain unsupported. See `RECOVERY.md`; current mandatory suite has 232 tests.


Registered direct models are now supported by advisory continuation when the
original enabled registry version matches its saved manifest and configuration.
Changed/missing/disabled versions are refused without substituting a newer model.
HTTP fixtures verify the requested model identity; loaded weights remain unattested.


Managed same-binary inference workers now support advisory continuation. Admission
converts exclusive ownership to shared execution inside its write transaction,
then provisions only remaining worker budgets and reaps the fresh child on success
or cancellation. Actual CLI/process/HTTP tests cover both paths. Host enforcement
retains the precise original deadline; worker shutdown has a bounded cleanup grace.
Custom backend versions and later-phase recovery remain incomplete.


Solver cycles now persist bounded reasoning boundaries with previous answer,
revision strategy, repeat hash and cumulative reservation anchors. An unconsumed
boundary following a completed advisory graph can resume without resetting cycles
or repeat detection. Tampered/consumed boundaries and unresolved calls are refused.
No-graph, within-cycle and general repeated resumption remain incomplete.


Unconsumed solver-boundary recovery now also covers FAST tasks, malformed-plan
whole-goal fallback and explicitly budget-skipped graphs. Valid planner output
without its graph record is refused. Actual HTTP and budget/stopping regression
tests pass; within-cycle and repeated recovery remain incomplete.


Repeated recovery at later unconsumed solver boundaries now preserves the original
context/settings/deadline and cumulative counters. Each new context assembly must
match the original bytes and its chronological call/cycle anchors; history is
bounded to 17 assemblies. Tests cover two resumptions, unchanged settings, exhausted
cycles and tampering. Arbitrary within-cycle repair remains incomplete.


Failed structured saved-answer recovery can now report `revision_ready` and retain
an admitted next-solver boundary after a fresh charged independent check. It never
repeats the saved inference or treats rejection as success. Repeated failed answers,
unsupported state and unavailable budgets do not receive a revision; checkpoint
write failures roll back verification charges. Arbitrary within-cycle recovery
and unknown inference outcomes remain unsupported.


Host embedding APIs now deliver task-scoped progress to authenticated separate
processes using versioned COGS/COGE FlatBuffers, pinned module/stage grants and
bounded queues/timeouts. Actual child-process and negative transport tests pass.
Socket/child provisioning remains host-owned; general executable registration,
command routing and automatic IPC replay are still incomplete.


Authenticated process module streams now support COGS v2 durable replay followed
by deduplicated live delivery. Saved cursors advance through filtered pages and
host-retained grants restore task/version/stage scope after registry changes.
Version-one live output is preserved. Receiver frame/time limits and version/cursor
validation are tested; provisioning and general command routing remain incomplete.


### Typed arithmetic module dispatch

`src/fabric/tools.rs` and `protocol/tool.fbs` implement replaceable host-registered
arithmetic modules over verified COGT FlatBuffers request/response frames. Tasks
pin immutable module versions before execution. Registration/discovery, conflict
rejection and unregistration preserve existing pins; shutdown revokes retired pins
and cancels active/queued futures. Four queued requests, 8 KiB frames, 120-byte
expressions, 256-byte answers and a 30-second dispatch ceiling bound this adapter;
the Engine's original task deadline and charged tool allowance still apply.

Only `math.evaluate` from task-scoped `jev-plus` is supported. Registration is a
trusted Rust host API, not an IPC permission grant. The Engine independently
recomputes arithmetic before marking evidence verified; module output alone is
never proof. This adds no inference calls. Backend errors become generic failures.
Service shutdown also stops this broker. These are in-process typed queues, not
an authenticated process tool transport, executable sandbox or general command
registry. SQL and other capabilities remain outside this adapter. General module
command routing/provisioning and executable lifecycle remain **Partial**.


### Authenticated arithmetic process transport

`src/fabric/tools/unix.rs` extends COGT command dispatch to an already connected
Unix socket. Both sides authenticate trusted UID/PID expectations. Immutable,
non-deserializable host grants restrict one task/module/version, 1–64 cumulative
calls and a 1–300 second session. The adapter carries actual broker task identity;
unscoped dispatch is denied. Four-byte big-endian lengths bound verified frames
to 8 KiB; partial frames and writes have two-second deadlines. Each exchange has
at most 30 seconds and both sides enforce their fixed session deadline.

Worker authorization is audited before execution; audit failure prevents dispatch.
Correlation replay, changed tasks/versions, exceeded calls, malformed frames and
pipelining fail closed. Cancellation discards the client stream permanently;
worker disconnect/shutdown/expiry drops pending backend futures. Backend failures
are generic and audits exclude expressions, answers and backend error bodies.
A separate child-process regression executes actual arithmetic. This is a trusted
embedding API: automatic task-grant provisioning, executable launch/reaping,
service/CLI tool-worker setup and other tool capabilities remain incomplete.
Dropping a future is not proof that arbitrary external side effects stopped;
this transport provides no executable sandbox or model attestation. Abrupt worker
termination may omit the terminal audit record. Original Engine tool allowances
and independent arithmetic verification still apply; grants cannot expand policy.


### Managed built-in arithmetic workers

`configure-tool-worker --enabled true` persists `arithmetic_process` (default
false); restart a running service to apply it. Disable with the same command and
`--enabled false`. Direct and socket `ask 'calc: 6*7' --mode fast` now lazily launch
the same Rust binary as an arithmetic worker, using the existing bounded broker
and COGT commands. No inference worker/model call is needed for arithmetic.
The default arithmetic registry is required; custom versions are rejected before
task creation rather than silently replaced. Other capabilities remain in-process.

`protocol/tool_bootstrap.fbs` defines verified COTB v1, bounded to 2 KiB with
two-second read/write deadlines. Both peers authenticate approved UID/PID before
bootstrap. Trusted setup grants exactly one task, built-in module/version and one
arithmetic call; its lifetime is at most 30 seconds and conservatively rounds up
remaining task seconds. The host still enforces its original precise deadline.
The worker opens the matching private workspace database with the pinned quota;
no mutable generation settings, model credentials or inherited environment are used.
Permission/tool-budget admission precedes lazy launch and the host independently
verifies the returned arithmetic answer.

Launch intent, owned PID, bootstrap delivery and confirmed reaping are distinct
audit records. Ownership begins at spawn and survives cancelled/failed handoff;
normal task cleanup closes the connection, waits two seconds, then kills/reaps
with a further two-second limit. Directory/socket cleanup checks inode/device
identity. Cleanup errors fail the task. This also applies after execution errors
and cancellation. Process/runtime crashes can still leave stale files or unknown
cleanup outcomes; no automatic crash cleanup or executable sandbox is claimed.
This supersedes the arithmetic-specific provisioning/CLI lifecycle limitations
above. General executable provisioning and non-arithmetic command routing remain
**Partial / Not implemented**. Process overhead is not a measured speedup.


### Typed SQL capability routing

`src/fabric/tools.rs`, `src/fabric/tools/sql.rs` and `protocol/sql_tool.fbs` extend
the shared tool registry beyond arithmetic. Default `sql.local` 1.0.0 implements
`sql.query`; capability lookup pins SQL and arithmetic separately before execution.
Trusted Rust hosts may register immutable SQL replacements. Active capability
conflicts and changing an active module's capability are rejected. Unregistration
preserves existing pins, while broker shutdown cancels current/retired work.
Registry history remains bounded to 128 versions, with one active module per
supported capability; each module has four queue slots and a 30-second dispatch
ceiling under the original task deadline. Arbitrary capabilities are not accepted.

COGQ v1 carries structured tables/rows/typed cells and structured results, not JSON
strings inside IPC. Arithmetic COGT v1 remains unchanged. SQL frames are bounded
to 256 KiB; verification caps depth at 16, table visits at 25,000 and apparent
bytes at 1 MiB. Decoded cell budgets (32,000 request bytes / 16,384 response bytes)
stop aliased FlatBuffers references from amplifying allocations. Existing input,
identifier, table/row/column, cell and canonical result limits also apply.

The Engine charges `sql.query` before dispatch, records the selected version and
reruns the constrained SQLite checker before publishing verified evidence. This
uses a fresh execution of the same compiled SQLite checker, not an independently
implemented SQL engine. Wrong/malformed module output cannot establish proof.
One authorized command includes both execution and checking, potentially two
bounded SQLite runs (100 ms each), sharing the existing two blocking-worker slots.
No model call or new permission is added. The default backend still uses only
inline task-local data and denies host databases, mutations and unsafe functions.
Registered Rust backend code is trusted host code and is not automatically sandboxed.
SQL process transport/provisioning and broad executable tool loading remain
**Partial / Not implemented**; arithmetic worker setup remains arithmetic-only.

SQL Unix RPC is now implemented as a trusted-host embedding API; see
`PROTOCOL.md` and `src/fabric/tools/sql/unix.rs`. Structured COGQ exchanges enforce
UID/PID, task/version/call/session grants, replay rejection and audit-before-work.
An actual child-process test executes SQLite. Cancelled/malformed exchanges close
permanently and metadata excludes query/result bodies. Automatic SQL process
launch/bootstrap/reaping from CLI/service is still not implemented; the installed
smoke evidence above predates this transport. Current mandatory checks pass with
301 tests (281 library and 20 integration); real inference remains unverified.

Managed SQL launch/cleanup is now implemented for direct CLI and socket service
through opt-in `sql_process` / `configure-sql-worker`. It reuses owned startup and
bounded reaping, adds SQL-only COTB v2 setup, and keeps arithmetic-only v1 intact.
Custom SQL registries and exhausted tool budgets cannot silently launch a default
worker. Actual CLI/service tests verify SQL execution and unsafe-query cleanup;
independent host SQLite verification remains mandatory. This supersedes the SQL
provisioning limitation above. Crash cleanup and executable sandboxing remain open.

Inference worker startup now owns the child/socket/lease before handoff awaits,
records launch intent and owned PID separately, and reaps failed/cancelled startup
through normal bounded cleanup. Retry of an unfinished startup is refused. A real
child regression covers cancellation, ownership-audit failure, retry refusal and
socket-directory removal. Crash cleanup and external GPU cancellation remain unproven.

Worker audit recovery now includes arithmetic and SQL sessions in the same
exclusive-lease transaction as inference. Mixed batches preserve task results and
charges, validate bounded metadata and record interruption without replay/PID
killing. Live tool sessions block recovery; malformed metadata and audit failures
roll back the batch. Legacy unversioned tool audit starts remain supported.
Filesystem cleanup and broader module lifecycle remain incomplete.

Journaled managed-worker socket cleanup now has verified operator preview/apply
commands and actual host-death coverage. It removes only identity-matched sockets
and empty private directories under exclusive runtime ownership; extra files,
symlinks, stale/tampered plans and malformed metadata fail closed. All 318 tests
and mandatory checks pass. See `RECOVERY.md` for pagination, partial audit/filesystem
failures, Linux requirements and unjournaled/hostile-owner limitations. Automatic
crash recovery and broader module lifecycle remain incomplete.

Current verification: 321 Rust tests (300 library, 21 integration), formatting,
all-target checking and Clippy pass. Trusted tool registries now support immutable
version rollback, including SQL, without changing existing task pins. Capability
identity survives unregistration; inactive backend references remain count-bounded
until shutdown. Real direct and managed-worker Ollama generation succeeded, with
independent Rust sorting verification and confirmed child reaping. See
`LIVE_INFERENCE.md` for measured limits; broader live studies and module lifecycle
remain incomplete. Earlier unavailable-inference statements are historical.

Inference brokers now also support host-only rollback to retained immutable
backend versions. Original queues and in-flight role snapshots are preserved;
active role conflicts fail before mutation. All mandatory checks pass with 324
Rust tests (303 library, 21 integration). See `MODELS.md` for retained-resource,
expired-transport and external weight identity limitations.

Transport-bound inference descriptors are now preserved by initial broker
construction and validated during registration. Changed module IDs, versions or
roles fail before inference; actual child-process discovery now reports its fixed
connected scope. All mandatory checks pass with 325 tests (304 library, 21
integration). This is trusted metadata consistency, not loaded-weight attestation.

Event module registrations now retain immutable metadata for host rollback and
reactivation. Future observers use the selected filter; existing live/replay and
process grants retain pinned versions. Active/history ceilings remain enforced.
All mandatory quality checks pass with 327 tests (306 library, 21 integration).
This adds process-local metadata lifecycle, not executable loading or persistence.

Structured goal interpretation now has an operator-selected bounded output
reservation (`interface_tokens`, default 128, 1–4096), configurable through
`configure-local --interface-tokens`. The original cumulative call/token/time
limits still govern admission. All mandatory checks pass with 329 tests. Live
interpretation quality remains separately evaluated; this option does not turn
empty/invalid model responses into success or grant permissions.

Local inference now supports an optional strict provider `reasoning_effort`
override, configurable with `configure-local --reasoning-effort`; default requests
are unchanged. Managed workers preserve it through explicitly versioned COGB v2,
with legacy v1 and malformed pairing tests. All mandatory checks pass with 330
tests. This requests provider behavior; it does not attest hidden reasoning or
prove successful structured interpretation.

Planner admission now preserves the first whole-goal solver against prior
interface/expert charges, checking remaining calls, output reservations and prompt
bytes and auditing skipped planning. All mandatory checks pass with 331 tests.
This corrects the two-call live trial's scheduler defect; valid real-model
structured interpretation is still not established by that correction.

The language interface accepts one exact Markdown `json` fence around the same
strict bounded object, preserving unknown-field rejection and the user's original
goal. Multiple objects, surrounding prose and malformed wrappers remain rejected.
All mandatory checks pass with 333 tests. This handles an observed provider format,
without making advisory intent a permission or proof of an answer.

The corrected bounded real language/solver path now also succeeds in a recorded
STANDARD-mode trial: two calls, accepted advisory structured intent, planner skip,
whole-goal explanation and confirmed managed-child reaping. The explanation stays
unverified. Optimized installation/smoke also pass for this source. See
`LIVE_INFERENCE.md` and `RELEASE_AUDIT.md`; robustness and benchmark gates remain open.

The six-arm comparison runner now supports structural `integer_sort_v1` scoring,
checking exact integer multiplicity/direction without trusting runtime verdicts
or answer whitespace. All mandatory checks pass with 334 tests (313 library,
21 integration). Public sorting examples are demonstration data; live reviewed
comparison remains outstanding. See `BENCHMARKING.md` for shared-verifier limits.

Interface/planner/critic JSON now shares one bounded exact-fence parser, preserving
strict schemas, graph checks and advisory status. Planner graphs and critic revisions
accept the same observed model wrapper as interpretation. All mandatory checks
pass with 335 tests; live recursive quality and gain remain separate research evidence.

Public comparison workflow checks now use the explicit `--workflow-check` flag.
The runner rejects missing/conflicting purposes and reports public results without
held-out review claims. All 336 tests and mandatory quality checks pass. The
existing held-out benchmark gate is preserved; independent datasets and intended
small-model quality measurements remain missing.

Inference version inventories now expose local adapter availability without remote
probes. Permanently closed Unix connections cannot be reactivated or routed;
pinned requests fail before dispatch. Unknown/connected/in-use observations are
not remote health guarantees. All 337 tests and mandatory quality checks pass;
optimized installation must be refreshed after this runtime change.

Arithmetic and SQL brokers now mirror inference local availability handling:
closed Unix streams remain diagnosable but cannot be pinned, reactivated or
queued. Actual malformed-response regressions cover both capabilities. All 337
tests and mandatory checks pass; no remote-health, grant-renewal or reconnection
claim is introduced.

Optimized installation and the full installed smoke workflow passed for source
`bb3c987`, closing the pending installed-source gap for inference/arithmetic/SQL
availability handling. See `RELEASE_AUDIT.md` for binary hash and actual evidence.
Grant expiry and call allowances remain independently enforced; present streams
are not health or renewable-authority attestations.

Unix arithmetic/SQL availability now includes expired/exhausted grants, excluding
these adapters from pinning, activation and pinned dispatch without refund or
renewal. Both actual two-call and paused-clock zero-call regressions pass. All
339 tests and mandatory quality checks pass; optimized installation proof awaits
refresh after this runtime change.

Managed worker wrappers now propagate initialized adapter availability; expired
unlaunched tool deadlines are visible without starting children. Actual socket and
same-binary cleanup regressions cover inference, arithmetic and SQL. All 341 tests
and mandatory quality checks pass; optimized installation refresh remains pending.

Optimized installation and installed smoke passed for source `aa95552`, refreshing
proof after grant-aware routing and managed availability propagation. Exact build
and evidence identities are recorded in `RELEASE_AUDIT.md`. Automatic residency
and general plugin provisioning remain partial; no stable release is declared.

Managed shutdown is now terminal even before first use, with initialization/handoff
coordination before cleanup inspection. Later valid requests cannot launch stopped
inference/arithmetic/SQL workers. All 343 tests and mandatory checks pass; the
optimized installation evidence needs refreshing for this runtime change.

Optimized installation and installed smoke passed for source `99c939f`, refreshing
proof for terminal managed shutdown and its concurrency assertions. See
`RELEASE_AUDIT.md` for exact binary/evidence identities. General lifecycle,
residency ownership and independent held-out evaluation remain unfinished.

Trusted hosts can now register explicitly ranked alternative inference modules for
the same role. New pins choose deterministically among available candidates;
existing pins never migrate or silently retry. Ordinary exclusive registrations
retain overlap denial. All 345 tests and mandatory checks pass; ranks are declared
policy, not learned cost estimates. General provisioning/residency remains partial,
and installed verification must be refreshed after this runtime change.

Optimized installation and installed smoke passed for source `d66dc07`, refreshing
proof after ranked inference routing; see `RELEASE_AUDIT.md` for exact identities.
Arithmetic/SQL capabilities still allow one active module each; explicit ranked
tool alternatives and their active-capacity policy remain to be implemented.

### Ranked tool alternatives (2026-10-12)

Trusted hosts can explicitly approve multiple arithmetic or SQL adapters with
bounded immutable ranks. New task pins select lowest rank then module ID; existing
pins retain original versions. Exclusive registration, transport identity checks,
authorization and rollback remain enforced. Active modules are capped at 32 and
historical versions at 128. Three new regressions pass; the full 349-test suite
passes sequentially. Two default-parallel runs exposed different existing failures
(SQL execution and recovery lease contention); see QUALITY_REVIEW.md. Automatic
plugin provisioning and adaptive cost scheduling remain incomplete.

### Reference instruction separation (2026-10-12)

Nonempty solver reference contexts are JSON-quoted and followed by the original
objective; all inference roles receive fixed untrusted-evidence advice. A live
Qwen attack contaminated answers before the change and with system advice alone;
the combined prompt resisted the same attack once. Answers remain unverified.
Empty-context cost/checkpoint hashes are preserved; earlier nonempty-context
checkpoint hashes fail closed under changed prompt serialization. Full tests now
pass 351 sequentially and in the final default-parallel run; see QUALITY_REVIEW.md
for intermediate failures and LIVE_INFERENCE.md for actual security trial scope.

### SQL concurrency admission (2026-10-12)

Inline SQL now provides bounded backpressure: six admitted requests per process,
two execution slots, one-second queue timeout and cancellation-safe waiting.
Blocking work retains permits after caller cancellation until actual completion.
Input bounds precede admission; read-only inline-data restrictions and existing
SQLite execution limits are unchanged. Managed workers have separate process-local
limits. Dedicated fixtures verify exhaustion, deadline, cancellation and queued
completion without relying on global parallel-test scheduling.

### Typed host tool discovery (2026-10-12)

Local and socket `tool-modules` expose immutable arithmetic/SQL versions, active
selection, routing ranks and availability through append-only typed FlatBuffers.
Legacy replies default to an empty tool inventory. Unknown states, conflicting
registries and excessive inventories fail closed. Borrowed string bounds precede
copying and envelope verification bounds shared-reference traversal. Discovery
starts no workers and grants no permissions. All 358 tests pass, including real
socket discovery; expanded CLI/service smoke passes on the debug binary.

### Inference lifecycle observation (2026-10-13)

`inference-modules` and its socket variant now expose retained immutable adapter
versions, active selection, local availability and ranks through typed, bounded
FlatBuffers. Existing active-only `modules` remains compatible. All 361 tests and
quality checks pass, including real socket and debug CLI/service smoke. Discovery
does not launch inference, grant permissions or attest physical model residency.

### Compiled prompt consistency on continuation (2026-10-13)

New execution settings capture the default compiled role/system prompt fingerprint.
Continuation requires it to match, in addition to the existing context, model,
budget and module checks. Legacy missing or changed fingerprints are not migrated
automatically; read-only inspection and independent result recovery remain
available. All 364 tests and mandatory checks pass; this is version consistency,
not provider or executable attestation.


Inactive inference backend retirement is implemented through trusted-host
`Broker::retire` in `src/fabric.rs`. Active retirement and identity reuse fail;
existing task pins remain usable, while future rollback/routes cannot select the
retired version. `src/fabric/tests.rs` verifies backend release after the last pin
and queue close. This improves runtime handle lifecycle without provider unload,
new grants or general executable provisioning. Mandatory formatting, all-target
check, Clippy with warnings denied and all 366 tests (344 library, 22 integration)
passed on 2026-10-13 with flatc 25.12.19. Test log:
`/tmp/cogfabric-inference-retirement-tests.log`. Optimized installation evidence
predates this new API and must be refreshed before final release.


Tool backend retirement is implemented through trusted-host
`fabric::tools::Broker::retire` in `src/fabric/tools.rs`. Existing arithmetic/SQL
pins continue after retirement, while future selection/rollback and identity reuse
are denied. Metadata tombstones preserve module capability identity even after
all retained backend versions disappear. A regression verifies both actual weak
reference releases, authorized execution, denied actor access, cross-capability
reuse rejection and registration of a fresh same-capability version.
Mandatory formatting, all-target check, Clippy with warnings denied and all 367
tests (345 library, 22 integration) passed on 2026-10-13 with flatc 25.12.19.
Local test log: `/tmp/cogfabric-tool-retirement-tests.log`. No new permissions,
provider unloading, owned-child cleanup guarantee or performance gain is claimed.
Optimized installation verification must be refreshed for these retirement APIs.

The optional `cogfabric-server` workspace crate now builds on the core library
for authenticated loopback OpenAI-compatible Chat Completions. `src/agent/`
provides a transport-independent bounded native solver turn; OpenCode owns its
file/tool loop and approvals. Collective profile aliases retain Engine execution.
Actual OpenCode v2.0.26 with Qwen3 8B passed a disposable read/edit/read workflow.
See `HTTP_API.md` for setup, buffered SSE semantics and remaining native worker
and API limitations. Current workspace verification: 391 Rust tests, formatting,
all-target checking and Clippy with warnings denied. Earlier counts above are
historical milestones, not the current suite.

Native conversations now support opt-in same-image managed workers, including
registered models, scoped upstream credentials and one-task FlatBuffers grants.
Authenticated Unix exchange, audit-before-dispatch, cancellation, worker capacity
and normal/drop-path child reaping are verified. Actual OpenCode + Qwen3 8B also
passed a managed read/edit/read workflow. This supersedes earlier native-worker
limitations; role overrides and general independent executable isolation remain
unsupported. See HTTP_API.md, WORKERS.md and the latest quality evidence.

### Collective dropped-future cleanup

Collective inference/arithmetic/SQL process owners now retain child/socket/lease
ownership through asynchronous reaping after a task future drops. A shared
collective ownership drain is integrated into HTTP shutdown, including expert
inference owners. Cleanup failures disable new collective process admission
and fail shutdown for that host lifetime. Native and collective drains remain
separate; no new tool permissions or third-party sandboxing are introduced.
WORKERS.md describes deadlines and host-termination limits.

### Current installation verification

Source `a9933b3` passes a fresh Arch setup with Rust/Cargo 1.99.0 and FlatBuffers
25.12.19: 417 tests, all quality checks, optimized installations of both packages
and offline installed CLI/socket/HTTP workflows. Hosted CI also passed the new
HTTP smoke. Host managed OpenCode/Qwen evidence is separate from this clean-OS
run, which exercised no native inference or GPU. See INSTALLATION_VALIDATION.md
and RELEASE_AUDIT.md for exact scope and reproduction.

### Residency operation observation

`residency-inspect TASK_UUID --api-base NATIVE_BASE` now compares recorded
operator intent with fresh native inventory through read-only startup. It
reports matching instance IDs and observed desired state without resolving the
unknown outcome, changing task state or authorizing cleanup. Malformed/ambiguous
intent and endpoint mismatches fail before HTTP. Ownership-aware automatic
residency scheduling remains incomplete; MODELS.md specifies the limits.

### Basic benchmark preflight

`bench` now validates every dataset row before any task executes, rejects unknown
fields and empty/oversized cases, and records dataset identity plus runtime status
and charged model/tool task counts. Zero-call exhaustion remains distinguishable
from legitimate zero-model deterministic tool execution. Expected labels stay
outside inference prompts; scoring and comparison behavior are unchanged.

### Basic benchmark run cancellation

`bench --max-run-seconds` now bounds the run as well as each task. Ctrl-C or expiry
returns a partial report, durably cancels active inference and stops later cases;
the CLI exits unsuccessfully. Executed/unexecuted counts and run stop reason
distinguish cancellation from score failure. Existing Rust `benchmark` calls
retain a default 1800-second ceiling; `benchmark_until` accepts cancellation.
Cleanup grace and external provider continuation remain outside strict GPU-stop
guarantees. BENCHMARKING.md documents partial accuracy semantics.

### Benchmark identity disclosure

Basic benchmark reports now retain declared configuration and its hash, package
version and compiled role-prompt hash. Both runners share a bounded identity
reader for backend labels, expert bindings and primary registration snapshots,
without copying reasoning text. Configuration/weight identity remains explicitly
unattested; hashes and registrations describe metadata, not neural proof.

### Replaceable Jev+ expert selection

`routing::ExpertRouter` now separates executive skill selection from Engine
orchestration. The default tag heuristic remains unchanged. Trusted replacements
receive pinned skill metadata outside the SQLite lock; at most two distinct
enabled snapshot IDs are accepted, with policy version/audit disclosure. Old
Engine clones and admitted decisions retain their original handles/snapshots.
This adds no model/tool permissions or model calls. General native isolation,
policy catalogs, scheduler replacement and calibrated utility remain incomplete.

### Skill inventory bounds

Legacy and manifest registration now share a 4,096 retained-ID capacity under
immediate transactions. Skill snapshot/inventory reads cap rows and SQL field
bytes before Rust copies, strictly parse tags and reject invalid state/versions.
Valid legacy empty tags remain readable; malformed metadata is not silently
converted or repaired. The router uses the same inventory ceiling. EXPERTS.md
documents compatibility and the independent active/version limits.

### Manifest read integrity

Manifest snapshot/activation/idempotent reads now guard SQL field sizes before
Rust copies and verify bounded rows incrementally. Duplicate active identities
fail instead of silently overwriting a graph entry. Existing-version registration
checks stored body integrity before returning idempotent success. Corrupt data
remains unchanged and cannot be implicitly repaired or promoted by these reads.


### Explicit atomic expert disable sets

`skill-disable-set ID...` / `registry::disable_set` accepts 1–256 distinct
canonical IDs under the existing human-only `skill.disable` permission. The
operator explicitly selects the damaged expert and any dependents to contain.
Remaining enabled manifests, projections and dependencies must validate. No
cascade is inferred; unknown/already-disabled targets and late audit failures
roll back every enable change and audit in the immediate transaction. Bodies,
hashes and retained inventory capacity are preserved. Normal activation stays
strict; this is not executable isolation or automatic recovery. Invalid retained
inventory metadata can still require reviewed repair/restoration.


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


### Revision-chain resource admission

Before unverified deep/research answers enter critique, Jev+ now checks that an
additional cycle, three calls and 640 output-token reservations remain for the
critic (128), reflector (128) and revised solver (384). It also checks cumulative
prompt-byte upper bounds and each backend prompt's 32,000-byte ceiling. Bounds
use the exact candidate truncations, the validated 1,000-byte critique reason,
and a 4,000-character strategy bound of 16,000 UTF-8 bytes. No new allowance is
granted or charged by this advisory admission; each subsequent call still uses
normal durable reservations, authorization, deadline and cancellation checks.
Elapsed provider time can still prevent later stages after initial admission.

The durable `schedule` event precedes critique and records `admit_revision` or
`skip_revision`, required costs, uncharged prompt bounds and
`expected_utility_measured: false`. A denied chain preserves the current
unverified candidate with `insufficient_revision_budget`, avoiding critique or
reflection that cannot support another answer. Independent executable-criterion
revision retains its direct checker-to-solver path. This is feasibility-based
resource control, not calibrated expected utility, general scheduling, semantic
verification or evidence of improved model accuracy.


### Replaceable, recovery-pinned dependency scheduling

**Implemented (trusted Rust host scope):** `src/scheduling.rs` provides a versioned
`SubtaskScheduler` and default stable topological order. Engine installation
captures policy metadata; sessions validate full dependency-respecting selections,
audit intent before callback and persist order/hash before any subtask events.
`src/recovery/graph.rs` validates the committed schedule and sequential execution;
continuation prefix checks and actual graph continuation use that order without
reinvoking a policy. Legacy unmarked graphs retain default behavior; marked graphs
cannot silently fall back. Benchmark metadata discloses policy and no attestation.
See SCHEDULING.md. General parallel/module/GPU scheduling, untrusted policy
provisioning and measured expected utility remain incomplete. No grants, budgets,
protocol schemas or model-loading authority expand.


### Optional advice wall-time containment and shared timeout fallback

Optional legacy/structured workflow advice now has one cooperative phase deadline:
one third of the task's remaining original wall time, capped by the configured
inference timeout. The `expert_phase_budget` event records that ceiling before
advice execution. This is a deterministic feasibility heuristic, not measured
expected utility or a guarantee that later inference fits. Structured steps share
one phase allowance rather than resetting it per step. Original task deadline,
call/output/prompt/tool ceilings and individual authorizations remain in force.

A typed runtime inference timeout permits shared-backend advice to return control
to whole-goal reasoning, matching the existing bound-model fallback intent.
The spent call/output reservation remains charged; partial advice is discarded,
reported usage/queue totals become unavailable, and `expert_failure` must persist
before any main-solver work. Failure-audit errors stop execution. Other shared
backend errors, malformed results and kernel/audit denials remain fatal. Existing
bound-model failure fallback semantics are unchanged. Provider computation may
continue after client cancellation; `provider_stop_proven` remains false. This
adds no retries, new model loads, tool grants or fresh task allowances.

The real partial Qwen study at `22dbc5a` remains frozen and is not rescored or
rewritten. Its six expert timeouts motivated containment; this change has not
established improved Qwen accuracy or resolved its transport failure. Shared
backend recovery after a dropped managed exchange still depends on that adapter's
fail-closed session lifecycle; no automatic worker restart is introduced.


### Bound-advice failures remain fail-closed

Fallback now accepts only the private typed runtime timeout for shared and bound
advice. Previously the bound-model path caught every error, allowing policy/audit
or result-integrity failures to be treated as optional provider unavailability.
Unknown bound-provider errors now visibly fail the task; no primary inference
follows. Error text is never used as a classifier. Missing/disabled bindings still
skip before dispatch, and typed timeout fallback retains original spent charges,
unknown usage, mandatory failure auditing and remaining task limits.

This intentionally narrows availability fallback until a trustworthy typed error
contract can distinguish provider faults from kernel/transport integrity failures.
There is no retry, permission expansion, new process or model-loading authority.
Operator-trusted Rust remains outside an untrusted-code sandbox guarantee.


### Durable inference reservation accounting

`Session::call_with_backend` now records `model_dispatch` before mutating call,
output-reservation or prompt-byte counters and before taking prior usage metrics.
A rejected database insert therefore leaves existing session charges and reported
usage intact and cannot invoke the backend. A committed reservation still counts
if progress publication fails: `Store::event` marks that post-insert failure with
a private `PersistedEventError`, allowing the session to retain the durable charge
while returning the error before inference. Error text is not the classifier.

No committed reservation is refunded or replayed. Terminal task failure and
recovery audits retain their conservative unknown-usage semantics; preserving
prior in-session metrics does not fabricate a complete failed-task usage report.
The event schema, ceilings, authorization and backend execution order are otherwise
unchanged. A test-only observer poison hook exercises the post-commit boundary;
it is absent from production builds. Observer failure is not provider availability
and cannot enter the typed-timeout advice fallback.


### Unambiguous JSON at agent boundaries

Agent HTTP request bodies, native provider response objects, decoded native tool
schemas and tool-argument objects now reject duplicate decoded JSON keys at every
nesting level. This includes escaped spellings of the same name and duplicate
objects inside arrays. Byte caps remain 512 KiB for HTTP request bodies, 256 KiB
for provider responses and 32,000 bytes for schemas/arguments; ordinary JSON
recursion limits and trailing-data rejection remain enforced. The reusable Rust
`strict_json::value` helper preserves accepted JSON values without echoing the
rejected key. Raw valid tool argument strings are not rewritten.

Duplicate HTTP bodies fail before native inference. Ambiguous provider objects
cannot supply a completion; ambiguous tool proposals and correlated history are
also rejected. This eliminates a class of parser disagreement, not all prompt
injection or schema violations. Tool JSON Schema semantics and action permissions
remain the client's responsibility; Fabric proposes tools and executes none through
this API. No remote reference resolution, host capability, new tool or model
permission is introduced. Internal FlatBuffers framing/schema is unchanged.


### Strict provider and advisory JSON

Ordinary OpenAI-compatible reasoning responses now use the bounded duplicate-key
parser before selecting answer/model/usage fields. Structured advisory parsing
applies the same rule after validating its optional JSON fence, including nested
objects inside arrays. Native residency responses and inventory decoding also
reject duplicate decoded names before their values can be normalized or observed.
Existing response/role byte caps, JSON recursion limits, trailing-data checks,
credential echo checks, loopback-only HTTP and redirect/proxy denials remain.
Unique unknown native metadata stays forward-compatible; duplicate metadata fails.

Ambiguous initial inventory cannot authorize a residency POST. If an approved
POST has already occurred and its reply is ambiguous, the task remains interrupted
with `outcome_unknown`; no confirmation request, retry, inferred ownership or
cleanup follows. Model/provider content still confers no permissions or factual
attestation. Internal FlatBuffers and stored benchmark results are unchanged.


### Versioned declared capabilities and discovery

Expert manifests may now declare up to 16 distinct capability identities with
independent canonical semantic versions:

```json
"capabilities": [{"id":"domain.sort","version":"2.1.0"}]
```

This field is optional and omitted from canonical serialization when empty,
preserving legacy manifest hashes. Each expert's own ID implicitly provides its
manifest version; redeclaring that same ID is rejected. Capability identities use
the existing bounded project identifier syntax. Duplicate IDs, invalid versions,
unknown capability fields and excessive entries fail manifest validation. Aliases
are immutable with their manifest version and copied into task snapshots. Expert
dependencies still refer to concrete expert IDs/versions; aliases do not rewrite
or relax dependency constraints.

`skill-lookup domain.sort --requirement '^2'` lists active manifest providers whose
declared capability version matches the requested semantic range. The command
uses human-only `skill.inspect`; it grants no runtime authority. Results explicitly
say `declaration_only`, `permissions_granted: false` and `provider_health_proven:
false`. Unversioned legacy categories remain listed by existing skill inventory,
not fabricated as versioned lookup matches. Multiple compatible providers remain
visible rather than being silently activated or loaded.

Advisory language interpretation can name a declared capability separately from
an expert ID. The engine resolves it against the pinned enabled snapshot and
adds its provider ID within the existing four-refined-route ceiling. Resolution
prefers an exact active manifest, then the lexically first reviewed alias provider,
then legacy category metadata. This prevents a legacy placeholder from hiding a
reviewed provider. Current gap preparation reads enabled declarations and manifests
in one transaction, so aliases count as available metadata without creating a
candidate or grant. Capability versions are exposed by lookup; language intent
still names capabilities without version requirements.

Large interface prompts exceeding 32,000 bytes explicitly audit
`skip_interpretation` and preserve whole-goal reasoning rather than dispatching an
invalid optional prompt. The catalog is not silently truncated. These changes do
not prove operational expertise, document retrieval quality, provider availability,
weights, executable isolation or calibrated expected utility. Declaring a sensitive
name such as `shell.execute` cannot grant that permission. No model/tool loader,
FlatBuffers schema change, database migration or neural promotion is introduced.
Public Rust struct-literal callers must provide the new `capabilities` vector;
legacy serialized manifests remain compatible.


### Pinned capability route bindings

`route_refined` now records one bounded binding for each requested advisory
capability: requested ID, resolved provider, provider manifest version, declared
capability version, route-selection state, declaration-only status and no granted
permissions. Values come exclusively from the task's pinned snapshot. Replacing
an active declaration during interpretation cannot change the recorded old
binding. Legacy unversioned declarations retain null versions; missing capabilities
have null providers/versions and remain explicitly unselected with a gap record.

A compatible provider can be available but unselected when the existing four-route
ceiling is reached. `selected` describes route membership, not executed advice,
provider health, factual correctness or granted authority. Existing dispatch,
budget and verification records remain authoritative for those separate claims.
No route ceiling, permission, model call or protocol schema changes.


Ordinary builds no longer require an installed flatc: verified pinned Rust bindings
are included for all nine unchanged FlatBuffers schemas. Explicit FLATC still
validates exact 25.12.19 regeneration. Schema/binding drift fails; see PROTOCOL.md
for updating the bundle and INSTALLATION_VALIDATION.md for package evidence.


### Exact native tool-choice and text-part controls

Named `tool_choice` objects accept exactly
`{"type":"function","function":{"name":"declared_tool"}}`. Unknown outer or
nested fields, non-object function controls and undeclared names are rejected.
String choices `auto`, `none` and `required` retain existing semantics. This avoids
forwarding extra control fields in direct mode that native managed transport would
otherwise drop. Strict-schema declaration belongs to the tool definition, not the
named choice; the client still validates schemas and approves actions.

HTTP text parts accept exactly `type: text` and a string `text`. Extra image or
other fields are explicitly rejected rather than silently removed while flattening
text. These unsupported shapes return 400 before inference. Valid named choices
and ordinary text arrays remain supported. No tool execution, permission, budget,
conversation role or FlatBuffers schema is added.

### Fresh planning/decomposition phase ceiling

Fresh collective planning plus advisory subtask execution now share one phase
ceiling: one-third of the remaining original task wall time, capped by the
configured inference timeout. `planning_phase_budget` persists the ceiling before
planner dispatch. No subtask resets it. A typed timeout drops pending local work,
accounts elapsed inference, retains reservations, marks usage/queue totals unknown,
marks unfinished subtasks skipped and records `planning_failure` before proceeding
with the original objective and remaining budgets. Partial strategy is discarded;
completed subtask audit results remain explicitly unverified. Policy, scheduler,
provider and audit failures other than typed inference timeouts stay fatal.
Cancellation remains distinct and follows existing terminal task handling.

This leaves time for subsequent whole-goal work, not a guarantee of provider
completion, remote inference termination or measured expected utility. Trusted
synchronous callbacks cannot be preempted by this Tokio phase timeout. Saved graph
continuation keeps its existing original-task ceilings; this new fresh-phase
ceiling is not a general persistent phase-deadline recovery implementation.
Research exploration has its existing separate admission and is not covered by
this planning ceiling. General cost-calibrated scheduling remains Partial.

Paused-clock regressions cover a stalled planner, a completed subtask followed by
a stalled dependent step, retained calls/outputs, discarded partial strategy and
fatal failure-audit denial. Final integer-sort answers still require compiled
verification. Live quality/latency after this change is not yet measured; previous
installed binaries and graph reports retain their original source scope.

### Research exploration phase ceiling

The fresh RESEARCH explorer call now receives a separately audited ceiling of
one-third of remaining original task wall time, capped by configured inference
timeout. `exploration_phase_budget` is persisted before dispatch. Only a typed
inference timeout may become `exploration_failure` and continue the original
whole-goal solver within remaining budgets. Charged calls/output reservations are
retained, elapsed inference is accounted and reported usage/queue totals remain
unknown after interruption. No alternative is appended on timeout. Successful
exploration is still unverified strategy, not evidence or permissions.

Policy/provider/audit errors remain fatal, including a backend error whose text
says "Inference timeout" without the typed timeout condition. Failure auditing
must succeed before the final solver can start. This supersedes the earlier
fresh-planning note that excluded research exploration; it does not claim general
persistent phase-deadline recovery, synchronous-callback preemption, physical
provider cancellation or calibrated expected utility. Original task deadlines,
calls, tokens and permissions remain unchanged. Live effect and current optimized
installation after these phase changes are not yet verified.

The optional Wasm experiment now has a Core task audit bridge (`module_tasks` and
`experiments/wasm-experts/src/core_audit.rs`, feature `core-audit`). Trusted operator
embeddings can create fresh bounded tasks with pinned identities, original clocks,
recovery leases and transactional allocation/dispatch/outcome records. Duplicate
executor wrappers cannot reuse a recorded allocation. Cancelled or expired tasks
cannot acknowledge further results. Abandoned tasks recover as interrupted with
uncertain dispatch records preserved; guest resumption is unsupported. The bridge
adds no model capability, production interpreter dependency or execution endpoint.
An in-memory Store is useful for tests but does not provide disk durability.

Explicit `recover-advisory-result TASK_UUID --human-approve` now handles the
crash window between durable advisory inference and its graph-state write. It
links only the latest charged cycle-zero result after validating the complete
prospective continuation checkpoint. Advice remains unverified; task status,
original budgets/deadline and permissions remain unchanged. Unknown outcomes,
expired clocks and unsupported phases still cannot resume through this operation.

An operator-approved native LM Studio load/unload workflow now has one real
embedding-model lifecycle result in addition to fixtures. The Nomic instance was
loaded and unloaded through Core, preserving the already-loaded Qwen instance and
context. The script defaults to inspection and stops without cleanup on unrelated
residency drift or uncertainty. This is not generative model switching or an
ownership-aware automatic scheduler; see `MODELS.md` and the frozen native report.

Expert scheduling now distinguishes refusal before admission from an attempted
workflow. It can skip an unaffordable routed workflow in favor of the next routed
candidate while admitting at most one workflow. An admitted timeout/failure never
adds another expert attempt or resets budget. This addresses budget selection;
calibrated usefulness and general model/module scheduling remain incomplete.

Workflow admission now precedes backend setup. A refused model-bound candidate
creates no expert model snapshot, worker handle or advice-phase allocation. Actual
admitted workflows retain their original prompt construction and shared resource
ceilings. Diagnostic phase counts no longer include newly refused candidates;
historical reports retain the semantics of their recorded source revision.

Equal-rank inference routing now counts a currently processed request as well as
queued requests, allowing a newly pinned task to choose an idle peer when the
first module's queue is empty but busy. Operator rank and existing pins are
preserved. Typed inventory carries optional queued/processing observations;
legacy frames retain unknown values. Processing state is cleared on cancellation
and shutdown. This is local broker scheduling, not GPU ownership or global
provider scheduling.

The inference broker now excludes a closed local worker queue even if the backend
advertises Connected. Existing pins fail without rerouting, while future pins can
select another approved candidate. Registry health callbacks run outside locks;
activation revalidates retained identity so retirement during observation cannot
restore a removed version. This is local failure containment, not a proof of
external provider termination or isolated native execution.

Arithmetic and SQL brokers now share the inference broker's local failure rule:
closed queues override optimistic backend health, existing handles fail without
rerouting, and future handles may select another approved candidate. Tool health
callbacks can inspect registry metadata outside locks; activation revalidation
prevents restoration of a version retired during that observation.

## Reviewed optional workflow routing (2026-10-17)

Optional native workflow ranking now runs after capability refinement, with
strict paired evidence, immutable scope identities, explicit cost/benefit
thresholds and main-solver reserves. Audit failures and invalid returned orders
stop before expert admission. Native callbacks remain trusted host code; operator
attestations do not prove independent outcomes. The inspection CLI does not
install policies. No capabilities or model quality claims are added. See
[empirical workflow routing](EMPIRICAL_WORKFLOW_ROUTING.md).

### Explicit CLI workflow-policy deployment

Direct `ask` and Unix `serve` accept paired `--workflow-study` and
`--workflow-thresholds`. Files are validated once before state/backend initialization
and the resulting policy is immutable for that host. Later file replacement does
not alter tasks. Socket clients cannot override the service policy. This does not
change persistent configuration, install executables or grant model capabilities.
HTTP-server policy options and pre-routing checkpoint continuation remain incomplete.
See [workflow-policy deployment](EMPIRICAL_WORKFLOW_ROUTING.md#direct-cli-and-local-service-deployment).

## Finite-domain Verifier-R witnesses (2026-10-17)

Explicit `csp:` tasks now support bounded finite integer domains, all-different
and exact linear eq/ne/le/ge constraints under `finite_domain/v1`. Every check
is individually authorized/charged as `constraints.verify`, and existing bounded
revision, checked-answer preservation and published-memory rechecking apply.
Benchmark scoring accepts alternate satisfying assignments without a fixed expected
answer. This proves only declared constraint satisfaction, not optimality,
unsatisfiability or model quality. See [verification](VERIFICATION.md#finite-domain-assignments).

## Recovery lease diagnostics and fork-sensitive tests (2026-10-17)

All seven recovery entry points now retain the actual acquisition error chain.
Contention, refused symlink opens and unsafe lease metadata remain distinguishable
and stop before state writes. A controlled fork regression proves inherited
close-on-exec descriptors keep ownership until child exit, and resource-resume
fixtures exclude CLI fork/exec from concurrent immediate-acquisition assertions.
Production lock behavior and budgets remain unchanged; no provider-stop guarantee
is inferred. See [recovery details](RECOVERY.md#lease-acquisition-diagnostics-and-fork-inheritance).

## Joint capability coverage (2026-10-17)

Jev+ now resolves interpreted capabilities jointly over the pinned enabled registry
and model metadata. Existing routes are reused; bounded greedy coverage adds at
most four total providers with deterministic ties, excluding model-bound workflows
whose registration is unavailable. Audits distinguish missing declarations,
unavailable bindings and route-capacity/profile exclusions. No grant, provider
probe, model load or utility claim is added. See [capability resolution](CAPABILITIES.md).

## Pinned capability reference refinement (2026-10-17)

Full/Experts interpreted tasks can now retrieve selected-provider references from
a bounded detached snapshot made with the original task metadata. One charged,
audited scoped batch adds at most four untrusted snippets if main-solver/prompt/
verifier reserves fit. Oversized snapshots skip optional refinement; corrupted
content/audit failures stop. Existing evaluation corpora are reused and admitted
reference identities extend workflow study scope. See
[retrieval limits and evidence](CAPABILITY_RETRIEVAL.md). No relevance or quality
gain, physical memory bound or broader snapshot recovery is claimed.

## Solver-preserving optional interpretation (2026-10-17)

Interpretation now requires call/output/prompt room for itself and the first
whole-goal solver. Its phase is capped to one-third of remaining original time;
only typed timeout can fall back with charges retained and usage unknown. Parsed
advisory text is separately admitted against actual encoded solver prompt bytes.
Policy/provider/audit errors remain fatal and cancellation starts no fallback.
See [interpretation admission](INTERPRETATION_BUDGETS.md). No real-model utility
or provider-stop guarantee is claimed.

## Role-priority resource admission (2026-10-17)

Optional operator `settings.scheduling` prioritizes waiting inference roles with
FIFO ties and a bounded number of later reservations bypassing older work.
Protected large requests block smaller newcomers until capacity fits; active work
is not preempted. Default FIFO bytes remain compatible. Priority/bypass audit and
status metadata are validated during original-policy continuation, without changing
resource budgets, resident checks or quarantine. This remains local declared
scheduling, not physical/global control or a measured efficiency gain. See
[resource policy](RESOURCE_SCHEDULING.md#optional-role-priority-with-bounded-bypass).

## Interpreted capability version requirements (2026-10-20)

Optional `capability_versions` narrows declared capability coverage with bounded
SemVer requirements. IDs must already be requested; direct IDs use manifest
versions and aliases use capability versions. Resolution distinguishes incompatible
declarations and excludes wholly mismatched initial routes before optional workflow
admission. Mixed routes receive only compatible credit. Legacy JSON stays unchanged;
no activation, grants or success proofs follow from a metadata match. Role-prompt
fingerprints change and existing mismatched-checkpoint refusal remains in force.
See [capability versions](CAPABILITIES.md#optional-declared-version-requirements).

## Whole-goal candidate recurrence (2026-10-20)

Jev-R now stops nonadjacent repeated whole-goal answers, such as A → B → A,
before buying another revision. Independent verification runs first; successful
checks still complete. First-per-cycle hashes exclude planning and recursive
critic/reflector advice. Required progress audits fail closed. Continuation rebuilds
bounded history from existing validated nodes; an already recurrent checkpoint
cannot restart the search, including through revision-result reconciliation.
Resumed independent rejection is reported as `rejected`, with verification-attempt
and stop-reason fields. No extra model call, budget or permission is introduced.
See [reasoning](REASONING.md#stop-nonadjacent-candidate-recurrence). This remains an
exact-content heuristic, not proof of intelligence, quality or impossibility.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**688 passed across 34 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The first full run exposed an old custom-policy continuation assertion
expecting `unverified` after independent rejection; it now requires `rejected`
and an attempted verification, and the final full suite passes. Existing socket,
migration, mock inference, malformed-frame, policy-denial, cancellation and rollback
checks remain green with elevated local socket access. Protocol/schema bindings
are unchanged; `flatc` is unavailable, so fresh generation was not run. Discovery
at 127.0.0.1:1234 refused connections; real-backend quality/performance is untested.

## Configured dependency-chain priority (2026-10-20)

Optional owner `subtask_scheduling: "critical_path"` selects ready planner steps by
longest downstream dependency chain, then canonical step ID. Stable original order
remains the default; explicit selections enter configuration fingerprints. Engine
and direct Session construction share installation, and native Engine overrides
remain available. Validated decisions use existing durable intent/order gates;
continuation retains the original recorded decision after current config changes.
No new inference, resource ceiling or capability is introduced. Tests include
all four-node edge subsets/input permutations and real CLI/local HTTP recovery.
See [scheduling](SCHEDULING.md#configured-critical-path-ordering). This is bounded
sequential graph ordering, not measured runtime/quality or physical GPU scheduling.

Verification: formatting, `cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**695 passed across 35 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. Focused ordering tests and actual CLI/HTTP fresh/resumed execution also pass;
existing socket, migration, mock inference, malformed-frame, denial, cancellation,
recovery and rollback checks remain green under approved local socket access.
Protocol/schema bindings are unchanged. `flatc` is unavailable; fresh generation
was not run, while build-time bundled pairing/integrity checks pass. Discovery at
127.0.0.1:1234 refused connections, so no real-model performance/quality result is
claimed. Broader calibrated scheduling and physical resource/lifecycle verification
remain incomplete; Jev+ and Jev-R remain Partial. The next reasoning milestone is
structured exploration of multiple alternative strategies under original budgets.

## Structured alternative-strategy exploration (2026-10-20)

Explorer-R can now produce a strictly bounded two/three-approach portfolio in its
existing RESEARCH call. Canonical IDs determine reproducible once-per-cycle use;
selected JSON advice precedes other strategy within the existing total cap.
Structured rejection, prose critique/reflection, exact custom escalation prompt
admission and recurrence stopping remain intact. Required acceptance/selection
audits fail closed. Recovery rebuilds the pinned portfolio from the original
validated cycle-zero explorer result, retaining the next unused approach without
another explorer/planner call. Malformed structured output is discarded; bounded
legacy prose remains single unverified advice. See [exploration](EXPLORATION.md).
The explorer prompt fingerprint changes; mismatched old checkpoints/evidence are
not silently migrated. No extra calls, tokens, deadlines, permissions or neural
weights are introduced. Canonical order is not calibrated utility or quality proof.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**705 passed across 36 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The initial seven focused tests, two HTTP continuation cases and first full
suite passed; review then tightened legacy fallback and added wrapped-JSON coverage,
and all final checks were rerun successfully. Existing socket, migration, mock
inference, malformed-frame, policy-denial, cancellation, recovery and rollback
checks remain green with approved local socket access. Protocol/schema bindings
are unchanged; bundled generator/runtime integrity checks pass, but `flatc` is
unavailable and fresh generation was not run. Discovery at 127.0.0.1:1234 refused
connections. No real-model portfolio effectiveness, calibrated ranking or quality/
performance improvement is claimed. Those measurements and broader independent
reasoning remain incomplete; Jev+ and Jev-R remain Partial.

## Bounded hypothesis evaluation diagnostics (2026-10-20)

`bench`/`compare` now report per-task hypothesis audit counters; comparison arm
resources sum executed rows. The fixed-label SQLite reader distinguishes accepted,
legacy, invalid, selected, exhausted and unknown metadata, under event/payload
bounds. It copies no strategy, identifier, hash, prompt or provider-error text.
Unknown sources/versions/policies/authority flags/shapes remain unclassified;
malformed/oversized/excessive audit fails reporting. Selection counts are not
unique executed calls or quality evidence. Both report headers bind compiled
runtime-source bytes, and comparison also binds compiled role prompts. Historical
reports remain unchanged. See [diagnostics](EXPLORATION.md#diagnostic-reporting).

Verification: formatting, locked workspace/all-target check, **708 tests across 36
targets** (none ignored), and Clippy with warnings denied pass. Existing socket,
migration, mock inference, malformed-frame, denial, cancellation, recovery and
rollback tests remain green under approved local socket access. Protocol/bindings
are unchanged; bundled pairing/integrity checks pass, while `flatc` remains absent
and fresh generation was not run. Approved host-level discovery found the existing
LM Studio server ON with Qwen3 8B loaded; earlier sandbox-only refusal observations
must not be interpreted as proof that the host server was unavailable. One live
portfolio smoke and a 24-task public comparison at preceding source 5041478
completed; their exact source/evidence scope is separate from this reporting change.
Independent held-out quality, calibrated utility and physical resource proof remain
incomplete. Jev+ and Jev-R remain Partial.

## Retain and independently check public portfolio evidence — 2026-10-21

**Measured public integration / general quality remains Partial.** Two complete
24-task Qwen3 8B runs now have immutable raw reports, hashed provenance/settings,
public audit projections and a fixed four-case subset. The new regression checks
all 48 scores through independent exhaustive simple-path enumeration and
reconciles calls, latency and diagnostic event sums. Existing suites/labels and
historical reports are unchanged. See [results](../evaluation/results/README.md).

At source 5041478, all six profiles solved 4/4, with 12 accepted portfolios and
14 selection events. At b20d736, alone solved 3/4 and every other profile 4/4;
collective modes required more calls/time than tools/recursive. All 12 planning
and 12 exploration phases timed out, with zero accepted/selected portfolios in
that second run. Diagnostics therefore distinguish attempted phases from usable
strategy output. Original budgets remained enforced and failed usage stayed unknown.

The October 21 run used different manual provider load/offload/parallel settings;
no causal before/after claim is valid. The initially off/unloaded localhost server
was temporarily started with existing weights, then restored and checked. No
weights were downloaded or neural/capability promotion performed. Public/authored
cases, missing independent review/larger baseline, unknown weight/device identity
and unmeasured physical resources prevent general quality/efficiency claims.
Cost-aware optional planning/exploration admission is the next verifiable target;
Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**709 passed across 36 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The focused frozen-report regression also passes. Existing protocol pairing,
malformed frames, socket integration, migrations, mock inference, denials,
cancellation, recovery and candidate rollback remain green with approved local
socket access. Protocol/generated bindings are unchanged; `flatc` is absent, so
fresh generation was not run. Real-provider validation is explicitly separate
from mocks; all outcomes and provider-setting differences are retained. The
previous automatic-review quota failure did not execute the pending comparison;
this date's approved retry is the recorded b20d736 run, not a silent restart.

## Recent optional-advice cost admission (2026-10-21)

Optional owner `advice_guard` now skips planning/exploration after enough recent
same-scope failures or completions too costly for the current phase budget.
Scope binds configuration, prompts/source, module metadata, mode, criterion,
prompt-size bucket and execution profile. The reader uses a fixed 512-event tail,
4-KiB strict records and same-boot elapsed time; stale/other-boot/future samples do
not establish failure. Decision/observation auditing fails closed and expired
phase tickets dispatch no work. Original budgets, independent checks and default
serialized behavior remain intact. Reopened state shares observations; model or
profile changes start separate cohorts. Saved continuation does not replay fresh
optional advice. See [advice guard](ADVICE_GUARD.md).

This is operational cost suppression, not calibrated benefit, semantic equivalence,
physical hardware enforcement or proven real-model efficiency/quality. Cancellation
can leave missing observations and the bounded tail can forget history. Comparison
profiles are separated, but same-profile history remains live and is disclosed by
`adaptive_advice_history_frozen:false` when enabled. General expected-utility and
independent live evaluation remain incomplete; Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**718 passed across 37 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on the final unchanged source. The focused five-test implementation pass and
actual CLI/HTTP regression passed before final profile/cost/expiration coverage.
An earlier full run correctly rejected a compiled-source fingerprint after edits
continued during that run; the check was preserved and the final rebuild/full gate
passes. Existing sockets, migrations, mock inference, malformed frames, denials,
cancellation, recovery and rollback remain green under approved local socket
access. Protocol/generated bindings are unchanged; exact bundled pairing/integrity
checks pass, but `flatc` is absent and fresh generation was not run. No real-model
performance/quality gain is claimed for the new guard; its fixture evidence is
separate from the preceding public Qwen diagnostics.

### Managed operation resolution and receipt archive — work in progress

Schema 8 adds explicit operator resolution of unknown native residency operations
and immutable terminal receipt archives. The CLI requires approval and provider-stop
acknowledgement for GET-only resolution; no retry or inferred provider success is
introduced. Stale receipt authority requires explicit retirement. Archive snapshots
preserve history without restoring release authority. Initial integration tests
cover unknown loads, terminal archives and permission/replay denials. Full current
source verification is still pending. See `docs/MANAGED_RESIDENCY.md`.


### Ongoing implementation before the final verification pass

Task-driven operator-approved native model lifecycle integration now has source
APIs and CLI entry points (`residency-run`, `residency-task-resolve`). A request
provides the exact approved model set and lifecycle deadline. The coordinator
retains runtime/resource leases while loading/reusing active receipt-backed models,
binds each role to a receipt, checks fresh inventory before generation and checks
native response instance IDs. Only newly loaded models may be automatically
released, and uncertain inference defers cleanup. Registered expert models pass
through the same approved adapter factory rather than bypassing native bindings.

Native model keys are the provider's documented request selectors. Instance checks
apply to returned metadata, not an attestation that an input selected immutable
weights. Unknown task generation creates a durable independent mutation gate;
explicit resolution requires operator stop acknowledgement and unchanged receipt
inventory. Ordinary recovery cannot silently replace a native lifecycle backend.

Other integrated source work includes atomic coordinated expert dependency-version
activation, a typed trusted in-process module broker exposed on Engine, and exact
trusted verifier restoration. General FlatBuffers process dispatch and complete
recorded-wave reconciliation are being implemented. This source state has not
received the final build/test/security/performance pass. Earlier passing counts
refer only to their recorded historical source snapshots.

General module IPC source is now integrated: versioned `general_module.fbs`,
strict typed codecs, private authenticated Unix sockets, fixed same-image workers,
parent/child identity checks, deadlines, cancellation and owned child reaping.
`Engine::with_builtin_module_worker` and CLI `--typed-module-worker` enable this
path for typed arithmetic/SQL work; Session retains task tool accounting and the
compiled checker independently verifies results. Expert manifests can optionally
bind exact typed provider identities without creating grants. Task admission pins
the general module catalog so later host version switches do not replace handles.
The exact official FlatBuffers 25.12.19 compiler was downloaded to temporary build
tooling with its release SHA-256 checked; new bindings were generated rather than
handwritten. Source integration remains unverified pending the final pass.

## Client-owned MCP, skills and web workflow — 2026-10-25

**Implemented:** `cogfabric-agent` preserves client function descriptions/schemas,
correlated tool results/errors, client skill text and same-model reasoning effort.
The shared native/FlatBuffers catalog limit is 128, with existing aggregate byte,
frame and output-proposal limits retained. `agent_client_capabilities` audits each
turn's names and description/schema hashes before inference; audit failure prevents
inference. Catalog metadata grants no kernel rights and client results remain
unverified. A removed historical tool may retain its correlated result; new calls
must target the current catalog.

The agentic client owns MCP, skill loading, web connections, credentials,
permissions and execution. OpenCode is one interoperability fixture, not a
required dependency or a special core code path. No native MCP client/server is
introduced. The shared API is documented in `docs/AGENT_CLIENTS.md`. Source: `src/agent/{types,
mod,protocol,local}.rs`; tests: agent validation/IPC/audit tests and opt-in
`crates/cogfabric-server/tests/opencode_mcp_client.rs`. See `docs/OPENCODE.md`.
Verification results are recorded below. **Partial:** broader
client APIs and multimodal results; full Jev-R orchestration through this endpoint
is not implemented. Synthetic MCP fixtures establish transport behavior, not real
REA/web capability or model quality.

Verification for this change:

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo +1.99.0 test --locked -p cogfabric agent:: --lib`: 30 passed.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  11 HTTP and 7 managed-worker tests passed.
- Socket-dependent tests required execution outside the restricted socket sandbox;
  the initial restricted run was not treated as a passing result. Two new fixture
  assertion errors were corrected before the successful rerun.
- The full all-target test suite was not run, preserving the operator's request to
  stop reflection tests. No live-model quality/performance benchmark was run.
- FlatBuffers schemas/generated bindings were unchanged. Typed IPC count,
  malformed-message and socket regressions passed; the exact generator pairing
  was not revalidated because `flatc` is absent in this environment (Rust runtime
  remains pinned to 25.12.19). Migration and rollback suites were not rerun.

Security/performance: catalogs remain bounded; expanding the count can increase
model context use, while audit hashing adds work bounded by existing request
limits. This does not broaden kernel capabilities or establish verified success.

Installed-client verification:
`cargo +1.99.0 test --locked -p cogfabric-server --test opencode_mcp_client -- --ignored --nocapture`
passed (1 test, installed OpenCode v2.0.32, 2.02 seconds). OpenCode loaded the
project skill with its actual `id` parameter, advertised `skill` and `execute`,
discovered the fixture MCP functions through Code Mode `search`, invoked both
synthetic REA/web tools and returned the results to CGF. Earlier fixture/config
probes failed and are not counted as passing evidence; direct MCP function
exposure with `codemode:false` was not established. This verifies a client-owned
Code Mode workflow, not live REA/web access or arbitrary client compatibility.
The total focused successful regression count is 49. Full Jev-R orchestration
through the model endpoint and additional client protocol adapters remain the
next separate engineering milestones.

## Client-conversation Jev-R first slice — 2026-10-25

**Implemented per request; full session orchestration remains Partial.** Canonical
architecture sections 4, 5, 7, 9 and 10: `src/agent/collective.rs` integrates a
bounded client-conversation role graph under the existing AgentRuntime task,
model pin, policy, deadline, resource pool and typed managed workers. Shared
`reasoning::mode_limits` supplies existing executive mode ceilings. The operator
enables `Config.agent_collective` with `configure-agent-collective --enabled true`
(default off); request effort selects FAST/STANDARD/DEEP/RESEARCH on the same
`cogfabric-agent` model. Planning, optional exploration, solving and budget-gated
critique/reflection/revision preserve client history and current function schemas.
Only solver proposals may return; client execution remains external. Structural
Verifier-R checks never claim factual verification. Each call is durably reserved
before dispatch; aggregate request token/input budgets, call/cycle caps, deadlines
and cancellation remain enforced. Hash-only traces exclude private message bodies.

**Not implemented:** persistent session identity, cumulative budgets/checkpoint
continuation across client tool turns, general expert-registry integration and
independent factual criteria for arbitrary conversations. Each request is a fresh
task, and client session limits remain necessary. This supersedes the earlier
one-call-only endpoint description only when the option is enabled, not the
remaining full-session gap. See `docs/CLIENT_COLLECTIVE.md`.

`docs/ROADMAP.md` now explicitly places optional image, audio and tentative video
evidence adapters at the end, after core/client completion; all remain planned,
not implemented. The canonical French architecture is preserved unchanged.

Verification for this slice:

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo +1.99.0 test --locked -p cogfabric agent:: --lib`: 41 passed,
  including role/effort preservation, aggregate budgets, input/call fallback,
  no-actionable-critique stopping, audit refusal, cancellation, privacy, typed IPC
  and deadline-before-dispatch denial.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  11 HTTP tests passed; the initial managed run found a new fixture assertion
  counting audit phases as calls. After correcting it, the exact native-worker
  rerun passed all 8 tests, including three distinct owned/reaped collective role
  children, same model/effort, aggregate usage and no tool execution.
- `cargo +1.99.0 test --locked -p cogfabric config::tests::client_collective_setting_defaults_off_and_roundtrips --lib`:
  1 passed.
- `cargo +1.99.0 test --locked -p cogfabric --test client_collective_config`:
  1 passed; enable/disable persistence, invalid input refusal and no database/task
  bootstrap are checked through the real executable.
- `cargo +1.99.0 test --locked -p cogfabric-server --test opencode_mcp_client installed_opencode_v2_jev_collective_owns_mcp_and_skill_workflow -- --ignored --exact`:
  1 passed (installed OpenCode, 3.47 seconds, synthetic MCP fixture/deterministic
  backend); planner/explorer/solver calls and client skill/MCP results coexist.

Total: 63 focused passing tests. Socket/process checks ran with loopback access.
The full all-target test suite and reflection test campaigns were not run, honoring
operator direction; the successful-reflection/revised-answer branch has no new
runtime test in this slice. No live-model quality or performance benchmark was
run. FlatBuffers schemas/bindings and database schema were unchanged; generator
pairing, migration and rollback suites were not revalidated for this change.
A review found and fixed a late-dispatch deadline race before final verification.

Security/performance: no new tool grants or execution were added. Optional roles
consume extra inference and repeat client context; the option defaults off and
charges all reservations within the existing request limits. Managed role workers
add startup/reaping overhead. This establishes mechanics, not a quality gain,
GPU stop guarantee or complete Jev-R session orchestration.


## Durable client tool-loop collective — 2026-10-25

**Implemented for bounded client tool-loop continuation; focused verification
results are recorded below.** This entry supersedes the immediately preceding first-slice
note where it says that each client request is a fresh task or that persistent
session identity/checkpoint continuation is not implemented. The earlier test
results above apply to that earlier slice and are not evidence for this addition.

`src/agent/sessions.rs` now uses opaque `cgf_<uuid>` function-call IDs to resume a
single root task through standard client history. A transactional SQLite state
machine stores generation, pending call bindings, root model/configuration/
registration/effort pins, original same-boot deadline and cumulative call, cycle,
prompt-byte, output-reservation, proposal and internal-tool counters. Atomic
claims reject concurrent duplicate resumes. The client must resend the committed
history prefix and all correlated tool results; altered arguments, changed or
missing results, replay, unknown IDs, configuration drift and expired sessions
fail closed without a fresh budget. Per-request `max_tokens` only limits that
step's additional work and cannot raise the root configured ceiling.

The session captures bounded, untrusted local document/memory/skill-manifest
references at the root task and pins their identities. Planner/explorer advice and
derived references are stored in a bounded private SQLite checkpoint; advice may
quote conversation content. Checkpoint data is not encrypted, and clearing its
current value does not erase SQLite WAL history. Raw transcript, tool results and
arguments remain outside persisted session state and ordinary audit traces.

Only a completed checkpoint waiting for client tools is resumable after process
restart on the same Linux boot with unchanged pinned configuration. A reboot,
expired or unavailable original deadline, changed model/registration/effort or
other pinned configuration invalidates restore and is refused; it does not create
a replacement task or fresh budget. An in-flight model call interrupted by
crash/cancellation remains charged and cannot be replayed. `recover-tasks` aborts
active/waiting sessions without a refund or retry. Bounded admission-time cleanup
expires waiting sessions when their saved deadline is expired, on another boot or
unavailable. After a final answer, the echoed answer plus a new user turn starts a
new root task. Disabling the collective refuses its prefixed continuation IDs;
restore the original configuration or start a new conversation.

The feature keeps client tool execution external and does not expose every Engine
expert, custom execution, project-profile/model selector or the general
`resume-task` flow through HTTP. Applicable pure compiled criteria can verify a
narrow supported root-goal result; all other output and client tool results remain
unverified. `src/agent/sessions_tests.rs`,
`src/agent/session_context_tests.rs` and
`src/store/agent_sessions_migration_tests.rs` contain new regressions.

Focused verification supplied for this milestone:

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo +1.99.0 test --locked -p cogfabric --lib agent::`: 54 passed.
- `cargo +1.99.0 test --locked -p cogfabric --lib schema_`: 11 passed,
  including session schema migration coverage.
- `cargo +1.99.0 test --locked -p cogfabric --lib storage::`: 10 passed.
- `cargo +1.99.0 test --locked -p cogfabric --lib security::`: 12 passed.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  all 11 HTTP tests passed. The initial combined run found a native fixture
  assertion issue; after correction, `cargo +1.99.0 test --locked -p cogfabric-server
  --test native_worker` passed all 8 tests.
- `cargo +1.99.0 test --locked -p cogfabric-server --test opencode_mcp_client
  installed_opencode_v2_jev_collective_owns_mcp_and_skill_workflow -- --ignored
  --exact`: 1 passed with synthetic MCP results.
- `cargo +1.99.0 test --locked -p cogfabric --lib recovery::`: 145 passed,
  including existing revision/reflection recovery checks.
- `cargo +1.99.0 test --locked -p cogfabric --lib restore::`: 5 passed,
  including restoration invalidation cases.
- `cargo +1.99.0 test --locked --test restore_configuration`: 1 passed; the
  backup/restore configuration proof passed.
- `cargo +1.99.0 test --locked --test client_collective_config`: 1 passed.
- `cargo +1.99.0 test --locked -p cogfabric --lib models::tests::`: 5 passed.

No new successful-reflection branch test, dedicated reflection campaign, live
reasoning-quality benchmark or real REA/web connection was run. This verification
does not prove general reasoning quality, GPU completion after cancellation or all
agentic-client/protocol compatibility. FlatBuffers files were unchanged and the
generator pairing could not be checked because the generator was unavailable; no
FlatBuffers schema change was made. The dedicated restore, model and configuration
checks above passed.

## Engine client context and first-use model activation — 2026-10-25

**Implemented; broader Engine endpoint integration and physical GPU control remain
Partial.** Architecture sections 4–7, 9, 10 and 12: client collective sessions can
capture an operator-selected immutable project profile with
`configure-agent-project --profile id@version` (or `--clear`). The default remains
unset and preserves legacy Config serialization. Selection resolves inside the
opening SQLite transaction; full context/version/hash remain pinned through the
tool loop. Configuration drift refuses continuation. Matching active Expert
Network manifests supply at most three bounded workflow advisories; they never
grant execution rights or replace the current goal/checker. Existing private
checkpoint and aggregate 8-KiB context limits remain in force.

Native lifecycle policy v3 adds first-use activation for approved Engine model
keys. Deterministic work activates no model. Configured roles and selected expert
models share a coordinator and exact receipt checks. Logical resource admission
precedes loading, activated instances are checked before inference and replies
must match their instance. Original task/lifecycle deadlines survive restoration;
unknown load/inference results remain non-replayable and block unsafe mutation.
Only newly loaded instances can be released automatically after safe completion.
V1/v2 policies retain eager loading. Stable `managed_native` targets are policy
identities, never placeholder loaded instances. See `docs/NATIVE_ON_DEMAND.md`.

The optional server crate now forwards `nvml-gpu`, and `setup.sh --nvml-gpu`
builds both installed packages with it. Typed GPU frame tests cover malformed
frames, device/byte/value bounds, duplicate identities, version refusal and missing
metrics. The installer test uses local fake build/fetch tools and makes no remote
installation. CI checks the feature and GPU protocol suite. NVML is read-only
observation; inference stays with the configured provider. This environment
identifies an RTX 3060 but cannot access a functioning NVIDIA driver/device, so
live GPU inference, offload performance and measured VRAM were not established.

Client endpoint native lifecycle control, arbitrary Engine expert execution,
project/model selection by clients, physical GPU quotas and general cross-process
GPU arbitration remain incomplete. No real-model quality or speed claim is made.
Focused verification results for this source state are recorded in the latest
implementation-matrix entry; no complete v1.0 release-readiness claim is made.

Verification: formatting, default/all-feature workspace all-target checks and
strict all-feature Clippy pass. Focused tests pass: 57 client-agent, 33 resource
scheduler, 17 residency, 11 backend, 7 resource-route recovery, 20 native lifecycle,
2 policy-file input, 11 HTTP and 8 managed-worker tests. Optional NVML build checks
pass 12 GPU unit tests, 2 observation tests, 1 installer and 1 project CLI test;
the project Config roundtrip test also passes. Groups overlap where documented;
these are command outcomes, not an independently sampled reasoning benchmark.
Native GPU placement echo/inventory disagreement remains an unknown load rather
than successful GPU execution. Initial native integration failures were fixed and
the final 20-test target is green. No full reflection/all-target test campaign or
live GPU measurement was run. See the matrix for exact commands and limitations.

## Explicit Engine project requests and observed GPU admission — 2026-10-25

**Implemented; physical GPU enforcement and full conversational expert execution
remain Partial.** The existing one-goal HTTP Engine aliases accept optional
`cgf_project: "id@version"`. Strict parsing and registry preflight reject malformed
or unknown profiles before task admission; `cogfabric-agent` refuses this override.
The selection reaches existing `TaskOptions` and records an immutable context
snapshot/version/checksum before inference. Native lifecycle validates project
selection before it can load models, including library and CLI use.

`Configuration.gpu_observation` is an opt-in resource setting with exact device
UUID, selected sensor and `headroom_bytes`. Shared resource admission checks fresh
reported total/used memory against declared working device bytes plus headroom
before provider invocation. It adds no model call, permission or queue retry.
Default configuration and serialization stay unchanged. Guarded histories bind
the declaration and require matching successful observation/dispatch evidence at
continuation. Eager native policies are refused with this setting, so first-use
loads use version 3 behind the guarded call.

Missing drivers, devices or required metrics deny guarded work. Observations do
not independently estimate model-weight/KV requirements or enforce physical VRAM;
external clients can allocate after a sample. The NVIDIA driver and local model
endpoint remain unavailable in this environment, so deterministic fixtures verify
mechanics and no live GPU performance result is claimed. See the implementation
matrix for final checks and the resource scheduling guide for configuration.

Focused verification for this addition passes: 38 scheduler, 3 GPU-history,
21 native lifecycle, 13 HTTP, 8 worker, 1 HTTP shared-capacity, and 9 route/profile
continuation tests. Formatting, default/all-feature all-target compilation and
strict all-feature Clippy pass. Full all-target/reflection tests and live GPU
benchmarks were not run; exact commands and limits are in the latest matrix entry.
