# Cognitive network implementation

## Let expert steps request bounded typed computations — 2026-10-28

**Implemented a bounded expert-model → typed-core → dependent-step bridge;
runtime/provider behavior unverified and general heterogeneous workflows remain
Partial.** A model-backed expert may return either its ordinary structured advice
or an explicit JSON envelope with `advice` and one to four `computations`.
Each computation names a host-advertised fixed function, an ID and typed argument
data. `src/agent/expert_compute.rs` strictly parses the envelope and delegates
name/argument/call-ID validation to the existing core catalog and graph path.
Advisory `actions` never implicitly request execution.

`collective.rs` places optional computation schemas in expert prompt metadata;
actual model tool definitions remain empty and `tool_choice` remains `none`.
The exact per-wave catalog is captured with the paid prompt and reused when
parsing the response. Small record allocations (under 200 bytes), insufficient
tool/headroom, missing task pins and client tool-choice restrictions keep this
optional protocol unavailable. Schema visibility does not grant a capability.
Advice plus exact host result-reference overhead must fit the existing per-step
allocation; content/summary/checkpoint limits are not increased.

`sessions.rs` validates a unique ordered paid `expert-advisory` dispatch receipt
and matching model/content outcome before admitting cycle-zero core work. Receipt
queries have payload/row/time bounds. Every operation and verifier still receives
its own deterministic task-policy check and reservation, using original frozen
routes and deadlines. Namespaces use the originating model-call index rather than
the changing global counter. Durable pending fingerprints prevent duplicate or
replayed in-flight work and count toward the original 16-record memory bound.
Unresolved pending batches block checkpoint publication; failures are never
automatically replayed, refunded or declared complete.

`collective/experts.rs` stages requests until every simultaneously ready report
has been validated, then checks aggregate tool space and duplicate fingerprints
before new admissions. Original typed verification slots stay held. Core phases
remain owned futures in the rolling scheduler and share task execution slots
with model calls. The step stays in flight until its computations pass compiled
checks and their bounded memory is admitted; only then do dependent steps become
eligible. Newly ready model prompts inherit checked core memory. No extra Qwen
translation or model call is needed solely to submit the computations.

Version-3 `expert_advice.rs` summaries carry operation-ID/result-digest references.
They are syntax-checked and cross-checked against the same-root checked core
memory on publication and restoration. Version-1 text and version-2 advisory
summaries remain readable under their original schemas. Actual solver/presentation
input capacity is checked before publishing core feedback. Sizing hashes are
disposable preflight markers and never enter actual checked-result references.

These checks validate computations against model-selected arguments, not the
broader meaning of the original request. Model advice remains unverified; no
permissions, models, weights, registries or promotion thresholds are selected by
its envelope. Duplicate work currently refuses rather than coalescing requests.
General dynamic neural/typed graphs, unknown-provider recovery and evaluated
project LoRA remain incomplete or unverified.

Validation: formatting, locked workspace/all-target compilation, strict Clippy,
whitespace checks and independent source review only. No tests, benchmarks,
live provider calls, model activation, installation or recovery execution ran.
No throughput or reasoning-quality result is claimed.

## Keep structured planner, explorer and reflector memory — 2026-10-28

**Implemented for client collective role handoffs; runtime/provider compatibility
unverified and the wider framework memory objective remains Partial.**
Planner, explorer and reflector responses now use the existing bounded advisory
record schema instead of concatenated or truncated prose. Each role receives a
400-byte output-record limit and must return a confirmed provider `stop`
response. Pair accounting still processes both completed calls before any
schema/finish error propagates. Critic output keeps its explicit `revise/reason`
object and incomplete critic output cannot trigger a revision.

`src/agent/reasoning_memory.rs` keeps the original structured context plus at most
three named role records in a strict 2,000-byte envelope. It binds the original
request and frozen references by SHA-256, preserves context across replacements,
and records deterministic checker rejection as a bounded typed feedback entry.
Claims and suggested actions remain untrusted: this memory does not verify the
original meaning, authorize tools or train/promote weights. Empty/prose legacy
advice can remain readable; when combined into new memory, prose is represented
by an explicit digest/omission marker rather than copied as new conclusions.

All normal and overlapped planner/explorer admissions project joint memory
space before their model reservations. Single roles and reflection also check
space first; insufficient capacity skips optional work or ends bounded revision.
Projections fill the complete record-byte allocation. Actual output is parsed
again and merged without truncation or eviction. `collective.rs` checks final
memory before language presentation, while `sessions.rs` checks references on
continuation validation and the original root before publishing a waiting
checkpoint. New marked malformed/duplicate-key envelopes, replaced format
markers, mismatched roots/references and oversize canonical encodings fail closed.
The original 2 KiB advice and 32 KiB session-checkpoint ceilings remain unchanged.

This replaces these client-collective role prose handoffs, not every legacy
memory/episode or inference path in the framework. Published project memory,
arbitrary mixed neural/typed graphs and evaluated project LoRA remain separate
or incomplete. No new model, training job or automatic promotion is introduced.

Validation: formatting, locked workspace/all-target compilation, strict Clippy,
whitespace checks and source review only. No tests, benchmarks, live inference,
model activation, installation or recovery execution ran. Reasoning quality
and performance are unmeasured.

## Store structured expert conclusions — 2026-10-28

**Implemented for new Expert Network step summaries; broader reasoning memory
remains Partial and runtime/provider compatibility is unverified.**
`src/agent/advisory_record.rs` validates compact JSON with `v: 1` and bounded
`claims`, `actions`, and `unknowns` arrays. Claims distinguish `supplied` from
`hypothesis` and may carry proposed SHA-256 references; actions use a finite
kind enum. These fields describe untrusted conclusions, not a reasoning trace,
checked facts or executable permissions. References are syntax-validated model
claims, not proof that a matching source exists or supports the claim.

Each array has at most four entries. Claim/uncertainty strings are capped at
256 bytes, action targets at 128 bytes, and references at four lowercase hashes.
Unknown keys, duplicate JSON keys, null/wrong-type fields, empty records and
oversized payloads fail explicitly. The actual per-step canonical byte allocation
is included in the expert prompt and remains inside the existing shared
600-byte content and 1,200-byte summary budgets. No additional model is loaded.

`collective/experts.rs` parses complete responses without prose fallback or
truncation. Only provider `stop` responses may become `partial: false` results;
token-limited output refuses admission even when its prefix is valid JSON.
Failures retain charged calls, stop new workflow admissions and drain existing
owned work. Disposable schema-valid sizing values are used only for cost
projection; actual turns are admitted again before reservation. Insufficient
space skips the optional workflow or refuses publication without raising quotas.

`expert_advice.rs` writes version-2 summaries whose result advice is a validated
object. Root/reference pins, canonical hashes and the 2 KiB context limit are
preserved. Version-1 string summaries remain readable as untrusted legacy data;
new v2 records cannot silently fall back to strings. Client continuations restore
records without replaying inference or promoting claims into facts or weights.

This change affects expert workflow conclusions. Planner, explorer, reflector
and the broader session-advice path still use separate free-form strings;
structured memory across the entire network is not complete. Curated project
memory and optional evaluated project LoRA remain separate, with existing
operator/evaluation gates. No training or promotion occurs here.

Validation: formatting, locked workspace/all-target compilation, strict Clippy,
whitespace checks and source review. No tests, benchmarks, live inference, model
activation, installation or recovery execution ran. No speed or quality result
is claimed.

## Share execution capacity across typed and model experts — 2026-10-28

**Implemented shared mixed-phase task slots; runtime/provider behavior unverified.**
Mixed typed/model phases now use one `network_slots::Budget` instead of reserving
a fixed half of the width for each side. Ready work can use available task slots
while the other side has fewer running operations. The original configured task
ceiling (one to four), lower typed/model group limits, evidence dependencies,
model-call reservations, token quotas and deadlines remain authoritative.
Single-kind phases keep their existing scheduling behavior.

`src/agent/network_slots.rs` owns a task-local Tokio semaphore. Single operations
acquire one slot; planner/explorer pairs acquire two atomically to avoid holding
one slot while waiting for another. Waiting is deadline-bound and cancellation
drops permits through owned futures. The FIFO semaphore may leave capacity idle
while a paired reservation waits; this is not a throughput or fairness benchmark.

`sessions.rs` acquires slots inside owned typed futures before module evaluation
and retains them through compiled response verification. `collective/experts.rs`
does the same before model route preparation and inference; advisory pairs hold
their shared reservation through both results. `collective.rs` also moves
preparation of the explicit overlapping reasoning pair behind slot acquisition.
No outer scheduler waits for a permit while neglecting already-admitted futures.
Both joined branches still finish/account for admitted owned work after sibling
failure; no automatic retry, charge refund or detached task is introduced.

Version-2 overlap audit events describe shared execution capacity and retained
group limits rather than a fixed partition. These slots bound local owned
operations. An ambiguous provider timeout/disconnect can leave physical remote
work running after its local future ends; existing residency/resource policies
remain necessary, and this does not prove a physical GPU/provider concurrency
bound. No model weights, permissions or routing identities are changed.

Validation: formatting, locked workspace/all-target compilation, strict Clippy,
whitespace checks and source review only. No tests, benchmarks, live inference,
model activation, installation or recovery execution ran. Actual speed/quality,
general mixed neural/typed dataflow and evaluated project LoRA remain incomplete
or unverified.

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

The target remains the seven-layer architecture in `ARCHITECTURE.md`. The network
is **Partial**. Existing models and deterministic Rust services implement roles;
these changes do not create newly trained Jev+ or Jev-R weights or establish speed.

## Explicit language-only model

A reserved `language` role profile is an operator routing declaration, not a new
inference actor. It selects one model for `interface` (structured interpretation)
and `synthesizer` (presentation). The default model must be a different reasoning
model. Other profiles cannot select the language model, and conflicting explicit
interface/synthesizer profiles are refused. Provider-reported model identities must
match the selected model when the boundary is enabled. Names and endpoint replies
do not attest immutable weights or detect two aliases serving identical weights.

New Engine tasks save a version-2 presentation reservation and a bounded,
hash-bound evidence-reference snapshot before reasoning starts. Explicit
`resume-task` continuation can restore a fully recorded reasoning phase at a
validated reasoning boundary, preserving interpretation, context, charges and
the original deadline. It can also restore validated pre-solver advisory graphs,
including pending parallel waves whose recorded results can be reconciled.
Parallel-wave preflight subtracts the held language capacity before admitting
remaining work. Final presentation consumes the original reservation;
it does not receive a fresh budget. Native tasks still require their original
approved native restoration path. Existing continuation restrictions still
apply: Full/Collective execution, no explanation option and no conversation
reference. Legacy reservations without the evidence
snapshot, unknown call outcomes and interruptions outside supported boundaries
remain refused. Terminal presentation recovery uses its separate existing path.
This continuation is source implemented and statically checked; no runtime
recovery, tests or benchmarks were performed for this change.

Example configuration commands, using model keys chosen by the operator:

```sh
cogfabric configure-network --language-model YOUR_LANGUAGE_MODEL \
  --reasoning-model YOUR_REASONING_MODEL --parallelism 2 --expert-parallelism 2
```

Do not copy the placeholder names into a live provider configuration. No model is
downloaded, trained, loaded or promoted by these commands. Default configurations
without a language profile retain their prior shared-model behavior.

Direct role inference, token-target selection, native role targets and first-use
native selection preserve the boundary. Private one-instance native adapters are
constructed only after the outer immutable role map selects and validates their
model. Selecting a registered reasoning model preserves the explicit boundary
instead of silently clearing it. Token admission currently disallows role profiles;
combining it with this network configuration remains unsupported.

Typed worker bootstrap version 4 carries the explicit boundary in the existing
FlatBuffers role-profile vector. It requires the `language` profile and older
versions refuse that marker. No JSON internal transport or generated schema change
is introduced. Older workers reject version 4.

The conversation endpoint has its own durable collective path with language-only
interpretation/presentation, independent role routes and client-tool continuation.
It does not delegate every conversation to the Engine or inherit every Engine
recovery scope. Native activation remains explicit and experimental. These
implemented paths do not establish a finished, runtime-verified cognitive network.

## Client language request scope

The explicit language profile receives the original validated conversation for
interpretation and final wording. Its appended host context contains the supplied
advice/result and the pinned-reference digest; expert workflow plans and retrieved
reference bodies are retained for reasoning roles. A digest is an identity link,
not evidence of correctness. This projection does not summarize, truncate or
remove original client messages, including user constraints and correlated tool
history; those messages can still contain large or untrusted text.

Only the solver receives client tool definitions. Language and advisory roles
omit the definitions and keep `tool_choice: none` and parallel tool proposals
disabled. Exact serialized role requests still pass the same preflight and durable
reservation before dispatch. This reduces repeated schema/reference payloads; it
does not prove shorter wall time or backend concurrency. Legacy shared-model
configurations retain their reference bodies. Client deterministic typed-provider
execution remains incomplete; current client expert workflows still use configured
reasoning backends, distinct from the explicit language model.

## First-turn typed client experts

With an explicit language profile, a trusted embedding host can attach its typed
module broker through `AgentRuntime::with_typed_modules`. The broker must belong
to the same Store. The HTTP executable shares the Engine broker under its existing
`--typed-module-worker` option. An approved native policy's
`typed_module_worker: true` instead installs a separate client registry, preserving
the empty Engine template required for native task setup. The explicit worker flag
remains forbidden alongside a native policy. This configuration registers reviewed
worker code; it does not load a model.

The opening transaction freezes active task routes and retains exact module
implementations in process memory. A bounded metadata digest, broker ID and count
are audited; the full catalog is not persisted or sent to Qwen. These trusted
host routes can be narrowed by the frozen manifest bindings described below.
Metadata cannot register code, select executable paths or grant capabilities.

Supported exact original inputs are:

| Input | Typed operation | Independent result check |
| --- | --- | --- |
| `calc:`, `calculate:`, `math:` | Bounded expression evaluation | Compiled arithmetic parser |
| `sql:` | Bounded SELECT over inline task data | Separate execution of the original inline task |
| `sort:` | Rust builds a sorted candidate; typed module diagnoses it | Original multiset/direction criterion |
| `order:` | Rust builds a deterministic topological candidate; typed module diagnoses it | Original dependency criterion |
| `path:` | Rust builds a shortest path; typed module diagnoses it | Original directed shortest-path criterion |
| `csp:` | Typed bounded finite-domain search supplies an assignment | Original finite-domain criterion |

Recognized structured inputs reject duplicate JSON keys and retain their existing
domain, byte and complexity bounds. SQL never opens a host database; its existing
authorizer, progress limits, cancellation and capacity controls remain. SQL checking
uses a separate execution of the same SQLite adapter used by the bundled module,
not an independently implemented SQL engine. CSP failures
do not prove unsatisfiability: no assignment may mean exhausted search, provider
failure or an unsatisfiable goal. The built-in worker uses checked i128 linear sums
consistent with the original criterion, and unreachable paths accept `[]`.

Validated exact client typed inputs skip model interpretation: Rust already has
the complete typed program. Qwen runs once for final presentation, after typed
execution; model-backed expert workflows and planner/solver/critic/reflector calls
are skipped. Natural-language inputs still need interpretation before their
translated typed work. Independent typed operations use the configured bounded
slots without reserving a slot for an unnecessary language call. This supersedes
the former exact-input interpretation/expert overlap. Cancellation drops owned
futures; it does not establish that an already-started external operation
physically stopped. No unknown outcome is replayed or refunded.

Eligibility requires an available opening route and two remaining internal tool
slots: expert work and final presentation checking. Required/named client tools
retain the ordinary client-tool path. Without eligibility, the previous bounded
reasoning path remains. Single-operation natural-language translation uses the separate, semantically
unverified route below. Manifest bindings and typed dependency graphs follow the
separate rules below; interrupted catalog restoration remains incomplete.

The typed result stays in memory through presentation. Qwen receives a strict
numeric/JSON output contract; the actual returned answer is checked against the
retained value or original criterion under an additional charged internal check.
Changed values, wrapped prose or invalid structured results fail without a fresh
model budget or fallback. Internal correctness cannot be inherited by presentation.
The original model/token/input/tool and same-boot wall ceilings remain. The entire
typed dispatch and result verification, including SQL re-execution, share one
bounded deadline. Final presentation/checking remains under the root deadline.

Native client turns bind module forks to their existing exclusive workspace owner;
process workers avoid a conflicting runtime lease. Trusted Rust modules are not
sandboxed, and the reviewed worker process is a process boundary, not a sandbox.
Tool continuations do not refreeze the typed catalog. New requests are new tasks,
not global duplicate suppression. Built-in worker concurrency still defaults to
one; this change does not prove physical resource enforcement, general reasoning
quality, a speedup or a finished cognitive network. No tests, benchmarks, live
provider calls or recovery operations were run.

## Natural-language typed translation

When an explicit language profile has a host-attached typed broker and two spare
internal tool slots, the client interface prompt can advertise an optional
`typed_goal` alongside its existing interpretation fields. The host supplies the
finite list of capabilities available in the opening snapshot. For example:

```json
{
  "objective": "Compute twice the sum of three and four",
  "constraints": [],
  "required_capabilities": ["math.evaluate"],
  "clarification": null,
  "typed_goal": {
    "capability": "math.evaluate",
    "arguments": {"expression": "2*(3+4)"}
  }
}
```

This is a client-only extension. The public `language::Interpretation` and Engine
formats remain unchanged; legacy client interpretations without `typed_goal`
remain valid. The envelope rejects unknown fields, duplicate keys, surrounding
prose, unsupported capabilities, non-object arguments, unresolved clarification,
invalid single-operation capability declarations and a requested semantic capability version that
the typed host protocol cannot attest. Existing domain parsers validate math,
inline SQL, sorting, ordering, paths and finite-domain arguments before dispatch.
Missing data must not be invented; ambiguous requests should omit the extension.
Schema validation cannot prove that the model followed that instruction.

Original exact typed prefixes cannot be replaced by a translation. Required or
named external tool choices keep their ordinary client path. A translated request
is admitted only after the interface reply, only against its frozen available
route, and only within the original tool/call/token/input/deadline limits. There
is no model-selected executable, module, provider, model key or permission grant.
Natural-language work cannot overlap its own translation because it depends on
that output; exact-input expert/language overlap remains available separately.
No extra interpretation model call, automatic activation or fresh budget is added.

The Work object retains separate hashes for the original natural request and
canonical translated criterion, and marks its origin as a language translation.
Result, dispatch and final-presentation audits preserve this distinction. Typed
computation and strict final numeric/JSON formatting are checked, but a translated
task still finishes `unverified`: these checks do not prove that the translation
preserved the original meaning, constraints or all requested work. The standard
completion content is not a verification certificate; operators can inspect the
task status and audit. No newly trained model or neural safety claim follows.
Raw translated criteria remain in one-use process memory rather than persistent
project memory. Tool continuations do not refreeze or restore this typed work.

Single-operation translation and independent batches use the same origin rules.
Measured translation quality, dependent task execution and interrupted typed
recovery remain incomplete; independent manifest binding follows the rules below. No tests, benchmarks or live model/provider calls were
run for this addition.

## Independent composite typed requests

`typed-batch:` groups one to four independent exact typed queries. The batch body
is strict JSON capped at 32,000 bytes; each query keeps its original domain bounds.
Operation IDs are distinct, at most 32 ASCII characters, start with a letter and
contain only letters, digits or underscores. Nested batches, dependency fields and
unknown fields are refused. For example:

```text
typed-batch:{"operations":[{"id":"sum","query":"calc:2+2"},{"id":"sorted","query":"sort:{\"values\":[3,1,3]}"}]}
```

The response is one JSON object, such as `{"sorted":[1,3,3],"sum":4}`. The
final presentation must contain exactly the expected IDs and each value must pass
its retained per-operation check. Strictly validated raw JSON values retain their
numeric spelling during assembly and checking, so arithmetic cannot accept a
different decimal merely because it rounds to the same floating-point value.
Legacy single-operation text remains unchanged.
The aggregate response is capped at 16,384 bytes. Larger outputs or exhausted
language/token/input budgets fail instead of receiving another model allowance.

The client-only interpretation extension also accepts an independent translation:

```json
{
  "objective": "Compute two plus two and sort three, one, three",
  "constraints": [],
  "required_capabilities": ["math.evaluate", "sorting.verify"],
  "clarification": null,
  "typed_batch": {
    "operations": [
      {"id":"sum","capability":"math.evaluate","arguments":{"expression":"2+2"}},
      {"id":"sorted","capability":"sorting.verify","arguments":{"values":[3,1,3]}}
    ]
  }
}
```

`typed_goal` and `typed_batch` are mutually exclusive. Batch declarations must
match the interpretation's complete capability set, without unresolved
clarification or unsupported version requirements. The prompt advertises the
current operation allowance from remaining tool slots and the opening pinned
capabilities. Translated canonical queries together are capped at 32,000 bytes.
A translated batch remains semantically `unverified`, even if every computation
and final value is valid. No missing/extra declared operation is accepted as a
partial answer, but schema checks cannot prove that Qwen captured the user's intent.

The host validates the entire plan, requires every route to be present in the
opening snapshot, and requires two internal tool slots per operation (invocation
and final check). Every invocation is charged through the original session CAS
before polling any expert. One immutable snapshot is shared across bounded owned
waves. Concurrent operations respect both `subtask_parallelism` and
`expert_parallelism`, each capped at four. Exact inputs skip language interpretation and use the configured typed slots;
natural-language operations wait for translation.
All waves use one existing typed deadline and the original same-boot task clock.
No later wave starts after a failed/unknown result; all started siblings settle,
and reservations are not refunded or replayed. Cancellation still cannot prove
physical termination of arbitrary trusted modules.

Ordinary audits record `root_query_sha256` for the complete batch and separate
per-operation source/criterion hashes and IDs. Raw inputs/results remain in
one-use process objects. Final checks are charged per operation before any final
value is approved. Exact batches retain original-criterion verification;
translated batches retain their original-meaning limitation. The built-in worker
still has one execution slot, so parallel futures can queue behind that worker.

These remain independent batches; the separate `typed-graph:` syntax below
handles declared dependencies and checked data bindings. Cross-operation iterative
reasoning and interrupted typed-plan restoration remain incomplete. There
is no new permission, protocol/schema change, model activation or speed claim.
No tests, benchmarks or live provider calls were run for this addition.

## Manifest-bound typed providers

The typed opening snapshot now captures active, enabled Expert Network provider
manifests in the same SQLite transaction as the client session. It retains compact
metadata and exact host module pins: at most 256 manifests and 1 MiB of encoded
provider metadata. Workflow bodies, descriptions, credentials and weights are not
provider audit payloads. The canonical full-manifest hash is distinct from an
original source-file hash. Module metadata keeps its separate existing 64 KiB
bound; the task snapshot digest links both metadata digests.

A usable binding requires the exact provider module ID/version and equality of
its entire declared capability set with the frozen module descriptor. This shares
the Engine's pure binding validator without claiming that client tasks have the
Engine's recovery audit format. Capability versions, expert manifest versions and
module implementation versions are separate identities. Each operation's compiled
checker remains the acceptance authority; declarations do not prove competence or
attest executable behavior/weights.

For automatic selection, matching usable manifests are ordered by expert ID and
version. This is deterministic selection, not calibrated usefulness or performance
ranking. If no provider declaration covers the capability, legacy host-registry
routing remains. If a covering declaration exists but its binding is unavailable
or unsupported, the typed route fails instead of silently bypassing it. Selected
bound routes reserve the exact pinned module and never substitute another provider
because of queue capacity, failure or timeout. Generic host-only routes preserve
their prior bounded candidate selection before dispatch intent.

Exact `typed-batch:` and `typed-graph:` operations may additionally name an expert
and/or capability version requirement. For example, with an already published and active expert:

```text
typed-batch:{"operations":[{"id":"total","query":"calc:2+2","expert":{"id":"math.reviewed","version":"1.0.0"},"capability_requirement":"^1.2"}]}
```

The example expert identity is a placeholder, not an installed provider. The
manifest must declare a compatible `math.evaluate` version and bind an exact
opening host module. A version requirement cannot be satisfied by an unversioned
generic host route. Unknown experts, inactive/different versions, incompatible
capabilities or mismatched descriptors fail before dispatch. Model-translated
`typed_goal`/`typed_batch` forms cannot include expert/module/provider selectors;
the host alone chooses from its frozen catalog.

Dispatch audits link task, operation ID, whole-request hash, typed criterion hash,
snapshot digest, capability/version, canonical manifest hash and exact module
identity before broker intent. They store no raw request/result bodies. Activation
changes after task opening cannot replace those pins. Each call still needs its
own narrowed task grant, original budget and deadline. These client-only metadata
anchors do not implement interrupted typed-plan recovery or an immutable-code
attestation.

Manifests that require dependency experts or also bind an inference model are
currently refused by this typed route. Advisory workflow text is not an executable
provider program and grants no extra steps. Typed provider dependency orchestration,
mixed provider/model execution, calibrated ranking and interrupted restoration
remain incomplete. No model is loaded, trained or promoted by provider selection.
No tests, benchmarks or live provider operations were run for this addition.

## Typed dependency graphs and checked data bindings

The exact `typed-graph:` input describes up to four typed operations as a bounded
DAG. Each operation declares an ID, one fixed capability, concrete arguments,
optional `depends_on`, optional `bindings`, and the same optional expert/version
selectors available to exact batches. The entire graph is strict JSON capped at
32,000 bytes. Unknown IDs, self-dependencies, duplicate edges/IDs, cycles, unknown
fields and unsupported capabilities fail before provider execution.

```text
typed-graph:{
  "operations":[
    {"id":"descending","capability":"sorting.verify",
     "arguments":{"values":[0],"descending":true},
     "depends_on":["ascending"],
     "bindings":[{"target":"/values","source":"ascending","pointer":""}]},
    {"id":"ascending","capability":"sorting.verify",
     "arguments":{"values":[3,1,2]}},
    {"id":"total","capability":"math.evaluate",
     "arguments":{"expression":"2+2"}}
  ]
}
```

`ascending` and `total` can start independently, while `descending` waits for
`ascending` to pass its compiled check. Declared order need not be topological;
result checking restores declared order before matching IDs. The final result is
one object such as `{"ascending":[1,2,3],"descending":[3,2,1],"total":4}`.

Arguments must initially pass their existing capability schema/domain checks;
use concrete valid placeholders for fields that will later be bound. Each binding
replaces an entire existing argument value. Its `source` must also occur in that
operation's dependency list. `pointer` selects a JSON value in a previously checked
source result; the empty pointer means the complete result. `target` selects a
field within the destination arguments, never the operation/capability/provider
metadata. Pointers use strict RFC 6901 escaping, at most 256 bytes and eight path
segments. Array indices are canonical nonnegative integers; append syntax,
missing targets, root replacement and overlapping/ancestor targets are refused.
There are at most three dependencies and eight bindings per operation.

Default `value` bindings transfer structured data. SQL query text cannot be bound
from a result; the SELECT remains literal and the existing inline-data sandbox
still applies. A math expression target requires `mode: "numeric_expression"`,
which accepts only a whole checked JSON number and wraps its exact numeric token
as a parenthesized expression. It does not interpolate arbitrary strings or accept
exponent tokens unsupported by the math parser. Other data/type mismatches and
all original domain/size limits are checked again after binding. Copied values,
source expansion and resolved arguments have bounded 32,000-byte ceilings; the
capability may impose a stricter bound.

The host validates all routes and charges all declared invocations before polling
any. Only operations whose prerequisites have checked results enter a wave; ready
unrelated nodes share the existing bounded concurrency. No new deadline or budget
is minted between waves. Binding audit precedes queue dispatch and links the root,
operation, graph-template hash, prerequisite result **and criterion** hashes, and
the resolved criterion. All result data stays in one-use host memory and cannot
select code, models or permissions. A binding or provider failure stops further
waves without replay/refund; started siblings still settle.

Any output consumed by a later binding is frozen in final presentation. Qwen must
copy that source's verified JSON representation exactly, in addition to the usual
per-operation checks. This prevents substituting a different valid CSP/path witness
after a dependent operation used the original witness. Whitespace changes inside
such a source value, or equivalent numeric spellings, can therefore be refused.
Leaf values retain their existing criterion checks. Aggregate output remains
capped at 16,384 bytes and final checks remain charged per operation.

This adds exact structured dataflow. The client-only natural-language graph
extension below uses the same validated dataflow; `typed_batch` remains independent.
Graph edges can satisfy manifest-declared expert prerequisites under the frozen
provider routing rules below. Simultaneous model/provider bindings remain refused. Interrupted graph restoration, mixed neural work,
physical termination guarantees and measured speed/quality remain incomplete.
No model is activated by binding resolution. No tests, benchmarks or live provider
operations were run for this addition.

## Natural-language graph translation and language-call scope

The client-only interpretation envelope additionally accepts `typed_graph`,
mutually exclusive with `typed_goal` and `typed_batch`. It contains one to four
operations with IDs, fixed capabilities, domain arguments, dependencies and data
bindings. Separate strict DTOs exclude expert/module/provider/model selectors and
capability-version requirements. The entire translated graph and its canonical
form are bounded at 16,000 bytes; the existing graph parser rechecks DAG, pointer,
placeholder, binding-safety and domain rules before admission.

For example, a language interface may translate an unambiguous sorting pipeline:

```json
{
  "objective":"Sort the supplied numbers ascending, then reverse-sort that result",
  "constraints":[],
  "required_capabilities":["sorting.verify"],
  "clarification":null,
  "typed_graph":{
    "operations":[
      {"id":"ascending","capability":"sorting.verify","arguments":{"values":[3,1,2]}},
      {"id":"descending","capability":"sorting.verify","arguments":{"values":[0],"descending":true},
       "depends_on":["ascending"],
       "bindings":[{"target":"/values","source":"ascending","pointer":""}]}
    ]
  }
}
```

The declared capability set must equal the graph's complete set, clarification
must be absent, and unsupported version requirements are refused. A concrete valid
placeholder is a structural slot that will be overwritten, not evidence of user
input. The prompt forbids inventing missing real data; schema checks cannot prove
that the model complied. Ambiguous tasks should request clarification instead.
Original exact typed programs cannot be replaced by a translated graph.

Every translated Work and Template retains only the original request digest for
provenance. Resolving a dependent node creates a new concrete criterion and then
reapplies the translation marker and source digest. Thus neither a successful
provider result nor a successful final check can turn translated meaning into
original-goal verification. Such tasks still finish `unverified`. Consumed source
values remain frozen in final presentation, and all existing provider pins,
per-operation charges, deadlines and concurrency bounds apply.

The exact client path now admits work before any language call, executes its
checked typed program, then reserves one synthesizer call. Its final checker
expects one charged model call. The natural-language path reserves interpretation
first, executes translated work only after validation, then reserves presentation;
its final checker expects two charged model calls. No new allowance, retry,
checkpoint restoration path or model activation is added. This is a reduction in
configured call count for eligible exact inputs, not a measured latency claim.
The full configured typed concurrency is available during either typed execution
phase; language parsing or presentation does not run concurrently with that phase.

Public Interpretation/Engine request formats and older client replies remain
unchanged. This optimization is specific to the client typed route, not every
Engine/CLI path. Manifest-declared expert dependency/model combinations, mixed
neural/provider orchestration, interrupted restore and measured language accuracy
or speed remain incomplete. No tests, benchmarks, live models or providers were
run for this addition.

## Bounded parallel work

In initial Research-mode client conversations, the first expert-workflow wave
may run alongside planner/explorer advice. All three branches use the original
objective and prior interpretation; planning does not consume expert results
until they are collected for the solver. This requires a configured concurrency
limit of three or four, enough spare slots for the complete first expert wave,
and budget for the whole workflow, both advisory calls, solver and presentation.
All calls are reserved before the overlap starts. Required native routes prepare
their models before generation.
Later expert dependency waves wait for that first combined wave to finish. The
planner/explorer results keep their reserved call IDs and are reused once rather
than requested again. Unsupported or insufficient-capacity cases retain the
existing sequential phase ordering. This is client orchestration support,
without a runtime speed claim.

The Engine can also overlap its planner with the first wave of at least two
selected independent experts when planning is admitted outside Fast mode and
configured concurrency and shared budgets permit it. One slot is reserved for
the planner; dependency barriers remain in force for later expert waves. Exact
role routes are prepared after durable call reservations. Resource scheduling
checks the combined configured allocations before admission; this snapshot is
not a reservation or proof of simultaneous provider execution.

Completed overlap results and their expert-advice snapshot are published in one
database transaction. Explicit human-approved continuation validates the saved
calls, original configuration, selected manifest snapshots and registry digest,
and reuses the completed planner result. The digest links trusted task records;
it is not an independently verifiable registry membership proof. Read-only
checkpoint inspection does not reconstruct or modify a saved plan. Reconstruction
occurs inside the explicit resume transaction and rolls back on admission failure.
Incomplete or uncertain overlap histories are refused rather than replayed.
These paths have static checks only; runtime behavior and latency remain unverified.

When inference resource scheduling is configured, advance preparation acquires
a separate resource lease through the original target/profile, queue/allocation
and optional GPU-observation checks. Its `resource_preparation_intent` identifies
lifecycle work independently of the later generation lease. Unknown preparation
outcomes retain their allocation and require the existing explicit resource
reconciliation; completion does not complete a model inference reservation.
Recovery validates both chains. Already-bound models and adapters with no
preparation work skip that admission. Loading remains serialized and a provider
may still queue generation; this does not establish a latency improvement.

Client planner/explorer waves select and prepare both exact role routes after
the wave is authorized and durably reserved, before polling either generation.
For approved native on-demand adapters, this loads only the models required by
that wave. Expert-step waves similarly prepare their exact pinned solver route
once. The selected adapters are retained for dispatch; request deadlines,
uncertainty blocking and fresh per-generation receipt checks still apply.
Activation remains serialized under the native lifecycle policy. Preparing a
wave does not prove simultaneous provider execution or improved elapsed time.

Engine independent expert batches, parallel expert-workflow steps and advisory
graph waves use the same preparation ordering through an in-process backend
hook. Supported route adapters are pinned before preparation and reused for
generation. Preparation retains lifecycle/GPU admission, task permissions and
the original clock. With resource scheduling it temporarily reserves the
configured working allocation, then releases it after confirmed preparation.
Generation acquires its own lease through the typed broker and resource pool.
The expert phase
deadline includes both preparation and generation. Preparation failure stops
the wave without refunding or replaying its durable reservations.

`--parallelism 1..4` configures the existing `subtask_parallelism` setting. In
addition to Jev-R advisory graph waves, dependency-ready steps in one selected
structured expert workflow now run in bounded waves. The pinned manifest order is
validated before schedule publication. Each complete wave's call/token/input
limits are checked before reservation, and permission decisions and durable
reservations precede polling. Resource admission and the original wall deadline
remain active. Dropping the parent future drops locally owned pending futures;
it does not prove a remote model stopped. Unknown effects retain existing gates.

Parallel replies remain unverified advice, never permissions or independent
proof. Ranked experts are still attempted sequentially. Coordinating independent
experts with Jev-R roles, cross-task scheduling, model capacity-aware activation,
continuous service recovery and client streaming remain broader network work.
Concurrent calls on one backbone are not a measured speed improvement.

## Schema-based project memory

SQLite schema 11 adds immutable `project_memory_records`. Each canonical record
contains bounded facts, entity relations, typed constraints and evidence pointers.
Values are scalar strings (up to 256 bytes), numbers or booleans, not conversation
bodies or arbitrary nested documents. Every claim references declared evidence;
links and hashes are provenance declarations, not independent truth verification.

Records bind one exact immutable `project@version`, its checksum, an immutable
record UUID and a sequential revision. The table is outside document FTS and
cannot be updated/deleted by ordinary writes. Existing database limits still
apply; records have count/byte caps and persist through ordinary task retention.
Database backup/restore includes the table. Canonical JSON is the schema encoding:
it is structured text, not encryption, neural memory or protection from a hostile
workspace owner. Existing legacy prose memory paths have not yet been replaced.

Operator commands:

```sh
cogfabric init
cogfabric project-memory-publish memory-draft.json
cogfabric project-memory-show PROJECT_ID@VERSION
```

`init` performs the schema migration. A draft contains `project_ref`, `revision`,
`facts`, `relations`, `constraints`, and `evidence`; the CLI creates the record ID
and checksum. The referenced profile must already be published. The Rust API
exposes the same typed records and Store operations. Models cannot publish records
through these operator-only commands. Session opening now captures a bounded typed projection of the latest exact-profile
revision. Record, profile and projection hashes pin it across continuations;
checkpoint validation rejects inconsistent projections. Semantic conflict resolution and
conversion of legacy prose remain to implement. Memory constraints are data, not task permission
or acceptance-rule overrides.

New client sessions capture project memory before ordinary document/skill
retrieval consumes the context. The projection may use up to half the bytes left
after the project profile; the other half remains available for expert groups
and retrieval. Whole
claims and their evidence are retained, and oversized claims are omitted rather
than cut into invalid fragments. The existing total context and pin limits apply.

With the explicit language-only network, Engine and client retrieval skip the
legacy query/answer memory search itself, rather than fetching those episodes and
discarding them afterward. Legacy configurations keep their prior retrieval.
Historical storage, audit transcripts and corpus backups are not converted or
deleted by this policy. Projection selection validates/hashes its immutable record
once and computes candidate array size without cloning the surrounding context.
These changes reduce repeated source-level work; no measured latency claim is made.

### Explicit memory contradictions

`cogfabric project-memory-conflicts memory-draft.json` inspects a bounded draft
without opening the database, publishing a revision or calling a model. Its report
names conflicting claim IDs for matching subject/predicate pairs. Detection covers
facts violating explicit constraints, incompatible equality constraints, inverted
numeric lower/upper bounds, and required substrings forbidden by another constraint.
Reports stop with an error beyond 256 conflicts instead of silently truncating.

New publication and runtime projection construction refuse detected contradictions.
The operator resolves them by publishing the next complete corrected revision;
prior immutable records remain available for inspection. An intentional change
between revisions is not itself a conflict: revisions are complete snapshots.
Multiple facts are not assumed to make an attribute single-valued. Relations,
multi-constraint implications, ontology inference and external truth are not proved.
Integer comparisons retain exact signed/unsigned values; mixed integer/float
comparisons are unknown rather than rounded. Unsupported comparisons are skipped.
An empty report therefore does not certify consistent or correct project knowledge.
Existing checkpoint snapshots keep their original evidence; this does not rewrite
in-flight tasks or authorize any tool. No runtime execution or benchmark was run.

## Optional project LoRA

A project-specific LoRA is a future optional candidate, not a replacement for the
structured source-of-truth memory. Existing offline adaptation/provenance and
operator promotion gates remain authoritative. No adapter is trained or activated
by this change. With testing/evaluation disabled by the operator, no independent
quality/safety evidence is available to justify promotion. Dataset licensing,
train/held-out separation, candidate versioning, resource bounds and rollback must
be retained when that path is implemented.

## Evidence for this implementation run

Compilation, formatting and static review only. No tests, benchmarks, live model
requests, database migration execution or installation are performed for this
run. The migration and concurrent execution have not been runtime-verified.
The full network, continuous operation and performance objective remain open.

## Client routing and independent expert batches

With an explicit language profile, client conversations use the durable collective
path automatically. Interpretation and final presentation select that profile;
reasoning calls select their assigned role backend. Language calls consume the
same root call, token and wall budgets. Tool proposals return directly to the
client; presentation cannot inherit the solver's verification result. Custom
backends must support role selection before accepting this configuration.

`configure-local --expert-parallelism 2..4` opts into concurrent independent
workflow experts, including structured multi-step workflows; `1` restores
sequential selection. Batches require
distinct explicit requested-capability bindings, pinned compatible manifests and
complete version-compatible workflow dependency closures. Version requirements from the
language interpretation must match declared capability versions. With an explicit language profile, the opening transaction freezes the active
catalog (at most 256 entries and 16 MiB of encoded metadata), including registered
model snapshots, in one-use host memory. Only its digest is added to durable pins.
After interpretation, exact capability/version matching selects complete workflow
groups from this frozen catalog without rereading the registry or calling a model.
Configured language, native endpoint and resource declarations filter candidates;
normal dispatch still performs independent admission. Unsupported or oversized
candidates can yield to another candidate. The selected context remains bounded
to three entries, 8,000 reference bytes and 4,096 pin bytes. Schema memory is
preserved and optional retrieval pairs are restored only when they fit.
The selected subset and audit metadata are saved atomically under the original
claim, budgets and deadline. Tool continuations reuse that subset. The full
catalog is not persisted or recoverable after process loss; client mid-reasoning
crash recovery remains unsupported. Legacy sessions or catalog size refusal retain
query-ranked opening context. The digest links trusted host metadata and does not
attest immutable model weights or expert competence. Legacy snapshots without
routing-declaration pins retain the single-workflow path. All calls are admitted and durably reserved against
the same task ledger before their futures run. Each expert receives a first dependency-ready step before spare slots are filled
with additional ready steps, within the per-expert subtask cap.
Heuristic-only routes retain sequential expert selection; dependency-ready steps
inside a selected structured workflow retain their separate bounded wave support.

These changes do not implement a full client-to-Engine bridge. Native client
activation uses the explicit experimental mode described below. They do not create trained expert weights, prove inference
speedups or replace existing legacy prose memory. No live inference, tests,
benchmarks, migration execution or installation was performed for this slice.

Client batch scheduling lives in `agent/collective/experts.rs`. A fair ready-wave
selection gives each selected expert one ready step before filling remaining
slots, while enforcing `subtask_parallelism` per expert and an overall maximum of
four calls. The global cap is the larger of configured expert and subtask caps.
Shared model backbones are permitted; each registered route remains exact and
the explicit language model is excluded from expert reasoning.
An optional workflow that selects that language model, or whose adapter cannot
honor its exact registration, is recorded as skipped before call admission;
remaining experts and the ordinary reasoning solver retain their own routes.

Research-mode first waves may reserve two slots for planner/explorer. The exact
same expert wave is used for capacity preflight and dispatch, so advisory overlap
cannot exceed the global cap. Whole-workflow prompt admission includes bounded,
maximally escaped outputs for every step, covering any completed-results context.
Fallback to one workflow happens before model-call reservations for independent
work; a dependency graph is never flattened into a lone dependent. All sibling
outcomes are awaited; failures retain charges and do not trigger another expert
or provider retry. Dependencies are never satisfied by a sibling in the same wave.
One bounded internal workflow-advice reservation covers the batch; every model
call retains its own authorization and durable inference reservation.

New snapshots pin a digest of capability and dependency declarations alongside
workflow/model pins. Continuation rejects modified declarations. This links
operator metadata, not trained weights or proof that an expert is competent.
Configured resource preflight is an instantaneous advisory check; generation
still acquires individual leases and may queue. Client mid-reasoning crash
recovery, larger dependency graphs and measured performance remain open.

### Pinned cross-expert workflow dependencies

Client selection expands each requested root into its complete dependency closure
within the opening snapshot's maximum of three expert entries. Prerequisite IDs
and semantic-version requirements come from the hash-pinned manifest declarations.
Only executable workflow prerequisites with complete metadata/model pins are
supported. Missing, incompatible, cyclic or unsupported closures are skipped and
reported. Selection stays within pinned candidates, without a registry reread
or automatic download. No dispatched call is retried.
Shared prerequisites execute once in the selected graph.

An expert becomes ready only after every step of every direct prerequisite has
completed in earlier waves. Its request contains bounded prerequisite results,
exact expert IDs/versions and explicit untrusted-data labels alongside its own
step history. Whole-graph prompt admission uses complete worst-case result maps
with nested JSON escaping. The final structured advice preserves dependency IDs.
`expert_parallelism` caps active experts independently of each expert's
`subtask_parallelism`; the existing total call cap still applies.

The graph retains the original session budget, deadline, permissions and per-call
reservation rules. If a prerequisite route is unavailable or the whole graph
cannot fit the budget, the graph is skipped before any expert inference. Model
preparation remains serialized under native lifecycle rules, while admitted ready
inference can overlap. Legacy snapshots lacking dependency pins retain their
historical single-workflow behavior and cannot authorize new graph execution.
Static checks only; runtime dependency execution and performance remain unverified.

Opening capture (`agent/session_context/experts.rs`) resolves each ranked root's
prerequisites against the same transaction's full active registry snapshot.
Prerequisites need not match query words. Capability IDs participate in root
ranking alongside IDs, descriptions and tags. Runnable complete groups take
precedence over metadata-only advice, with stable relevance/ID ordering within
each class. Shared prerequisite entries are pinned once.

Each group commits its context and immutable pins together only if the complete
union fits the three-entry, 8,000-byte reference and 4,096-byte pin bounds.
Incomplete, unsupported or oversized runnable groups are omitted in full.
Snapshot order preserves ranked roots for capability-free fallback; execution
order is independently derived from the pinned dependency graph. Graph capture
precedes optional document/legacy-memory/skill retrieval, while retaining the
structured project-memory reservation. Optional retrieval entries and their pins
are removed in pairs if metadata fills up; they cannot displace a captured graph
or structured memory. This changes advisory selection priority, not tool grants,
model loading authority or provider parallel capacity.

## Client advisory concurrency

Research client sessions with `--parallelism 2..4` can admit planner and explorer
as one independent wave against the same initial advice. The complete wave plus
a minimum solver allocation and language presentation reserve must fit the root
budget. Both calls receive durable reservations before either starts; the solver
waits for both results. No sibling is detached, failures retain reservations, and
provider cancellation is not claimed on timeout. Other modes and insufficient
budgets keep sequential admission. Actual provider execution may still queue.
Compilation/static checks only; no runtime or performance evidence is claimed.

## Engine project-memory snapshot

Engine tasks with an explicit immutable project reference now capture the latest
structured project-memory revision before workers/inference. Engine and client
paths share the same bounded complete-claim projection builder; linked evidence,
partial-projection indication and record/profile/projection digests are retained.
The Engine records its selected projection and pin in the audit log and never
refreshes that revision during the task. Constraints remain untrusted data.

When the explicit language-only profile is configured, both Engine and new client
sessions omit legacy query/answer memory episodes from inference context. Existing
legacy data and audit traces are preserved; this is not encryption or removal of
all text from storage. Documents and current conversation remain language inputs.
Previously captured continuation contexts remain pinned rather than silently
rewritten. Optional independently evaluated project LoRA remains unfinished;
native client activation uses the explicit experimental mode described below.

## Independent structured expert graphs

Explicitly bound independent experts may now include up to four validated DAG
steps each. The complete graphs must fit shared call/output/input budgets while
reserving whole-goal planner and solver capacity. A stable round-robin ready-step schedule runs
each wave, bounded by both the selected expert cap and per-expert subtask cap; an expert's dependent step sees
only its own completed dependency results. Each result is unverified advice, not
evidence of success or a new permission.

Wave permissions and durable model charges precede every dispatch. All waves
share one expert-phase deadline bounded by the root deadline. Failure or timeout
discards the batch advice with no sequential retry. Completed structured outputs
remain complete JSON objects when added to context. Stable manifest identity and
explicit dependency events describe causality; finish order does not.

This is cooperative concurrency, not proof that a provider executes models
simultaneously or faster. No tests, benchmarks or live inference were run.
Managed client activation and the complete client-to-Engine bridge remain open.

## Atomic network configuration

`configure-network` replaces the previous role map in one validated configuration
save. Repeated `--role planner=MODEL`, `--role solver=MODEL`,
`--role explorer=MODEL`, `--role critic=MODEL` and `--role reflector=MODEL`
bind optional role models. Omitted reasoning roles use the default reasoning
model. Interpretation/presentation resolve through the language marker; no model
may serve both the language and reasoning groups under the same configured name.
The verifier remains the independent kernel, not a configurable reasoning model.

The command enables goal interpretation and client collective orchestration.
Parallelism defaults to two in this command; explicit caps accept 1–4. It keeps
existing provider, credential scope, budgets, project reference and residency
settings. It clears the prior default model registration because the default
model identity changes. No currently running service or session is reconfigured,
and no model is loaded or trained. Restart the service with the saved configuration
to use it for new sessions. Exact installed model keys remain operator inputs.

With an explicit language profile, Engine network execution requires the language
phases described below. Managed client activation and the full client-to-Engine
bridge remain incomplete.

## Required Engine language phases

With an explicit language profile, Engine execution profiles other than the
`Alone` comparison baseline require an interpretation call before inference/tool
work and a presentation call for completed, unverified or rejected results.
Interpretation is advisory: the original request and deterministic permissions
remain authoritative. Its parsed result is reused for capability refinement,
without a second interpretation call. Errors, cancellation and exhausted tasks
return operator diagnostics rather than asking a model to rewrite them.

The Engine reserves one presentation call, `interface_tokens` output tokens and
8,192 prompt bytes inside the original root ceilings. All reasoning/expert
admission sees the remaining capacity. The mode's reasoning-call allowance is
augmented by at most two language calls, still capped by `max_calls`. Actual
model dispatch charges remain durable. A complete result that exceeds the
presentation reserve fails explicitly rather than truncating JSON or returning
raw reasoning as a successful language response.

For completed results, the language output must equal the independently verified
answer byte-for-byte. A changed result fails; it cannot inherit verification.
Unverified/rejected results retain their status, and the language call supplies
no independent evidence. Cancellation and the original wall deadline cover both
phases. Existing configurations without the language marker retain prior behavior.

**Experimental recovery limitation:** tasks recording a language-boundary
reservation cannot use Engine checkpoint continuation until phase-aware
restoration exists. Continuation refuses these tasks before dispatch; no replay
or fresh budget is substituted. This restriction does not change the existing
durable client tool-continuation path. No tests, benchmarks or live inference
were run for this implementation.

## Language effort isolation

An explicit language profile forces `reasoning_effort=none` for interface and
synthesizer calls, even when a request or global configuration selects high
effort. OpenAI-compatible adapters send `none`; the native LM Studio adapter
maps that to its existing `off` value. Managed one-call workers and private
on-demand native adapters pin this setting before dropping the outer role map.
Reasoning roles retain request/config effort precedence. Configurations without
the explicit language marker retain their previous behavior.

This requests the provider's non-reasoning mode; it does not attest internal
model execution or measure latency. A provider that rejects the setting fails
explicitly rather than silently rerouting language work to a reasoning role.
Formatting/static checks only; no tests, benchmarks or provider calls run.

## Filling available expert slots

`expert_parallelism` bounds total active calls in an independent expert batch;
`subtask_parallelism` bounds ready steps from any one expert. Each wave considers
one ready step from each active expert first, then additional ready steps in
stable expert/topological order until the total cap is reached. A dependency
requires a result from a previous completed wave; merely scheduling its producer
in the same wave is insufficient. Registered expert brokers use the validated
step cap so the host does not serialize these admitted calls unnecessarily.

`expert_ready_wave` records the exact expert versions/steps and both limits before
per-call authorization and durable reservations. Existing root budgets, deadline,
resource admission, failure handling and no-replay behavior remain authoritative.
No runtime or speed evidence is claimed; tests and benchmarks were not run.

## Phase-aware inference audit

Inference audit now validates the Engine's language reservation against the
original configuration and mode, using the same reservation encoder as execution.
The reservation must occur once, before model/tool/cycle work. Interpretation
must be the first call with its declared allocation; its recorded result must
precede reasoning or presentation. Presentation consumes its held slot/token/input
capacity, ends model dispatch, and must correlate with a recorded synthesizer
result before publication is recognized. Conflicting model roles, altered
reservation fields, overdrawn phase budgets and work after presentation are refused.

Audit reports the current phase and keeps held presentation capacity out of
reasoning admission. Once presentation is dispatched, remaining inference
admission is zero. Unrecorded results remain charged and unknown; a result without
a publication marker is not reported as published. Reports contain identities,
hashes and metrics rather than model text. These observations grant no replay
or resume authority. Engine continuation is still refused until full phase-aware
restoration is implemented. No tests, benchmarks or live provider calls run.

## Published language-result recovery

`cogfabric recover-language-result TASK_UUID` restores the terminal task state
from an already published final language response. It requires a persistent
workspace, an exclusive runtime lease, an abandoned/interrupted task, a valid
phase audit, and no unresolved model dispatch. It performs no inference, model
load or tool replay and grants no new task budget. The saved response identity
must match the audited publication hash.

Unverified and rejected publications retain their status. A completed publication
requires a fresh supported compiled criterion check against the original query,
charged to the original remaining verification allowance. Failure records rejection
and retains the charge. Tasks without such a criterion cannot recover a completed
claim through this path. The audit allows only that single specific compiled
recovery check after presentation, not further reasoning or arbitrary tools.
Native lifecycle ownership and verifier callbacks retain their existing restoration
requirements; this command does not bypass them.

Raw solver-result and revision-result recovery refuse Engine language-phase tasks
so they cannot publish an answer that bypasses language presentation. Mid-reasoning
continuation and pending/unpublished language phases remain refused. This is
partial phase-aware recovery, not uninterrupted operation. No tests, benchmarks
or live workspace recovery operations were run.

## Exact pre-dispatch presentation checkpoint

Before the Engine dispatches its terminal language call, it records a strict
`language_presentation_ready` checkpoint containing the complete bounded request,
original configuration/objective/answer/prompt hashes, allocation and next call
identity. The saved request is at most 8,192 bytes, and is task state rather than
project memory. Like existing task traces it may contain sensitive task content;
it is not encrypted. It cannot restore permissions.

Inference audit validates the checkpoint against original settings, requires all
earlier model results to be recorded, and checks the next synthesizer dispatch's
identity, prompt digest, byte count and allocation. It refuses further reasoning
or resource work once the terminal checkpoint exists. Publication must match the
saved answer/status. The public audit reports `presentation_ready` and a checkpoint
flag, not the saved prompt. Old histories without this checkpoint retain their
previous phase interpretation.

The checkpoint distinguishes work ready to dispatch from an uncertain dispatch;
it does not itself authorize resumption. Dispatching a pending checkpoint under
the original clock/resource/worker scope is still unfinished. No tests, benchmarks
or live inference were run.

## Client Expert Network workflow execution

New durable client sessions pin complete bounded workflow plans, exact versions,
manifest hashes and plan hashes in their opening snapshot. Continuation validates
those pins without rereading live registry data. Old metadata-only snapshots
cannot authorize execution. Plans retain full legacy advice (up to 2,000 bytes)
or validated structured dependencies (up to four steps, 1,000 bytes per step).

The client collective executes a matching pinned workflow before its main
reasoning graph, or an admitted batch of independent workflows as described above.
Steps use the configured solver role or their exact registered expert route, with client tools disabled
and no provider/model selection from workflow text. All steps share the existing
Lease call/token/input/deadline ledger. A deterministic workflow-advice capability
reservation consumes one existing internal tool slot. Required client tool
capacity is preserved. No nested Engine task or fresh task budget is created.

Preflight uses the exact expert request construction, bounds nested JSON escaping
and reserves final solver/presentation capacity. Optional workflows that cannot
fit are skipped before dispatch. Scalar advice is bounded by JSON-encoded bytes
and marked partial; complete structured records are serialized whole. The summary
enters the same pinned continuation advice and never grants permissions.

This is a partial client bridge: expert steps use dependency-ready waves through
the shared solver model, bounded by `subtask_parallelism` (default one). Manifests binding a separate registered local model use their pinned model
snapshot through an adapter that explicitly supports that route. Provider-bound
manifests remain unsupported. Managed client activation requires the explicit
experimental mode described below. Client tool
execution/approval remains with the external application. Static compilation and
Clippy only; no tests, benchmarks or live provider calls run.

## Parallel client expert steps

Pinned client workflow DAGs now run independent ready steps in waves of at most
`subtask_parallelism` calls (1–4, default one). Dependencies must have completed in
a prior wave; scheduling a producer in the same wave does not satisfy them. The
whole-workflow preflight constructs exact tool-free expert requests and preserves
main solver/presentation capacity. Every wave rechecks capacity, authorizes all
calls and persists all Lease reservations before polling its owned futures.

Calls use the pinned solver route and the same ResourcePool/worker limits as
other client inference. All sibling outcomes are awaited and successful results
are recorded by their reserved call IDs. A failed or unknown result terminates
workflow execution with no retry and no dependent later wave. Model/provider
cancellation is not asserted by a timeout. Complete JSON advice and original
session budgets/clock remain unchanged across client tool continuations.

This enables host concurrency; provider queues and resource limits may serialize
actual inference. Speed, reasoning quality and runtime reliability remain
unmeasured. Workspace/all-target compilation and Clippy passed; no tests,
benchmarks or provider calls were run. Separate registered client expert backends
and optional native activation are described in the following sections; they
remain runtime-unverified.

## Registered client expert models

A new client session captures an enabled registered model's immutable manifest,
version and checksum in its opening transaction. Its workflow pins a digest of
that complete registration. Continuation uses that snapshot without a live
registry reread; disabling a registration affects new session capture. The
registration checksum and exact local endpoint/model are validated before route
selection. The language-only model cannot serve this reasoning role.

`LocalChat` explicitly supports the route. Custom adapters must opt in and attest
the exact local target; unsupported adapters skip the workflow before reservation,
with no default-model fallback. Expert calls retain the root session ledger,
resource pool, process policy, worker limits and credential scope. A changed
endpoint does not gain a wider credential scope. Registered output ceilings are
enforced, and configured exact token admission is clamped to declared context.
Ordinary OpenAI-compatible routes report `declared_context_tokens` and
`context_enforcement=unverified_provider`; no byte/token equivalence is assumed.
Provider context rejection fails explicitly. Provider-bound manifests remain
unsupported, and model names/checksums do not attest loaded weight identity.

## Resume a never-dispatched terminal presentation

`cogfabric resume-language-presentation TASK_UUID --human-approve` can dispatch
the exact saved presentation request when its phase is `presentation_ready` and
all earlier model results are recorded. It requires an exclusive runtime lease,
the original matching workspace/configuration/prompts, the original unexpired
same-boot clock, and sufficient original held call/token/input capacity. It
creates no task or fresh budget. It never replays a previously dispatched call.

This experimental path supports the configured local adapter, including its
same-image managed inference worker mode. Add `--native-policy PATH` to restore
the exact original approved native lifecycle policy, ownership and instance
receipts after bounded provider inventory validation. Eager and lazy restoration
resolve language roles through the explicit language profile. Original activation,
cleanup and deadline limits apply; unknown outcomes remain blocked. Custom
inference modules, registered default-model scopes and exact token-admission
scopes remain refused. Configured
resource pools use the restoration rules described below.
It does not silently bypass those settings. Completed candidates need an exact
prior independent proof hash binding, and their returned answer bytes must not
change. Engine arithmetic/SQL proofs now include objective and answer hashes.
Unverified/rejected candidates retain that status. A failed/uncertain dispatch
remains charged, leaves the task interrupted and is not retried.

General mid-reasoning restoration and managed client activation remain open.
Static checks only: no tests, benchmarks, provider calls, recovery execution,
configuration changes or installation were performed in this run.

## Resource-managed terminal presentation restoration

Pending presentation restoration now validates the original complete per-call
resource chains and immutable route map before rebuilding the configured pool.
It refuses unresolved calls, closed pools or a route map that did not admit the
synthesizer. It restores original catalogs/allocations and applies the pinned role
route restriction to the reconstructed backend. The terminal Session uses that
same pool for its sole new call, including profile selection and fresh GPU
observation when configured. No physical enforcement claim is added.

Admission is checked twice under the exclusive runtime lease, around pool setup,
within one bounded setup deadline. The second transaction compares the complete
configuration and exact presentation checkpoint before claiming the task. The
original task clock/call/output/input allowances remain authoritative. Histories
that ended in independently checked inline tools can restore their presentation
without inventing a whole-goal reasoning context; all resource chains must still
be complete. Existing general continuation checks retain their context requirement.

Native lifecycle ownership and other unsupported reconstruction scopes remain
refused. Resource restoration alone does not load/unload models or establish
GPU correctness, neural quality or a speedup. Compilation/static checks only;
no tests, benchmarks, recovery execution or provider calls run.

Native terminal recovery persists the response and task status before lifecycle
cleanup can release a model. A separate `recovery_native_cleanup` event records
cleanup outcome; cleanup failure does not erase a durable response or authorize
replay. An interrupted cleanup remains subject to original residency reconciliation.

## Shared inference connection pool

Engine role calls, native instance-bound calls, client conversation calls and
exact-token preparation share a process-local HTTP connection pool. Connections
are not held behind a global request mutex: independent admitted waves may use
the pool concurrently. Idle reuse is limited to four connections per host for
30 seconds. This does not change model activation or provider scheduling.

The client has no default authorization header, model, task or request body.
Credentials remain resolved and applied per request; proxy use and redirects
remain disabled. Engine, native and client calls retain their configured
request timeout. Exact-token calls retain their shared absolute preparation and
completion deadline. Existing response bounds and instance/model validation
remain in their adapters. Pool reuse grants no authority to load models, retry
uncertain generation or bypass a reservation. Connection setup overhead can be
avoided when the provider permits reuse; no latency improvement is measured.

Static checks only; no tests, benchmarks or provider calls were run for this
change. Optional native activation is described below; general client
mid-reasoning restoration remains incomplete.

## Provider prediction slots versus CGF parallelism

Native inventory retains optional `loaded_instances[].config.parallel` as the
provider-reported prediction-slot limit for that instance. `residency-status`
includes a `prediction_concurrency` observation with instance IDs, nullable slot
counts and unknown queue/active-prediction counts. Missing slots remain unknown;
CGF does not interpret omission as unlimited capacity or sum slots across models.
First-use native activation also records the observed instance's slot count in
its hash-bound model-binding event, without changing the receipt or granting
additional inference capacity.

A CGF ready wave can be concurrent while its requests wait in a provider queue.
One configured prediction slot may serialize calls to that instance. Reported
slots do not prove simultaneous GPU execution, free slots or a faster response.
These observations neither load/reload a model nor change an operator policy.
Legacy inventory fingerprints and KV-placement fingerprints keep their existing
projections; prediction-slot changes do not reinterpret saved approval digests.
Provider inventory documentation:
[LM Studio model inventory](https://lmstudio.ai/docs/developer/rest/list).

Static checks only; no tests, benchmarks or live inventory requests were run.
Automatic managed client activation still needs a conversation lifecycle and
explicit treatment of OpenAI tool proposals versus native instance evidence.

## Native HTTP host resource ownership

When an HTTP Engine has an approved native lifecycle policy, server setup leaves
client resource pools request-scoped. Otherwise an idle client pool's shared
workspace resource lease would prevent the coordinator from acquiring the
exclusive lease required for model loading. Ordinary servers retain their shared
Engine/client pool behavior.

Each client request constructs or joins the exact configured pool before task
execution, then uses ordinary resource admission. Concurrent requests share a
live pool through the existing weak registry. The pool is released after its
last active owner disappears; waiting client-tool checkpoints do not retain an
idle process-owned pool. Durable session call/token/tool ceilings remain in the
session ledger, and unresolved resource history still refuses a new pool.
Native mutation remains excluded while a client request actually owns resources.
This resolves an idle ownership conflict; it does not yet install a native
activation adapter for client conversation calls. No model or tool permissions
are added. Static compilation/lint only; no tests or live lifecycle operations.

Configured pool reuse now requires matching ownership as well as matching
workspace/configuration: ordinary pools share only with ordinary callers; native
pools share only with the identical private lifecycle owner. A weak-registry hit
cannot lend exclusive lifecycle authority to an ordinary client request.

## Client lifecycle scope admission (partial bridge)

`agent::native_scope::NativeClientScope::prepare_session` is a library admission
API for an existing durable client session. It requires explicit operator approval,
retains exclusive ownership of the configured persistent workspace, and validates
the exact saved session configuration and original same-boot clock. Initial
attachment is limited to an active generation-zero session with no dispatched
model calls. Restoration reuses the single original policy and cutoff; the cutoff
is the earlier of the original session deadline and original session start plus
the approved lifecycle duration. Attaching late cannot restart that duration.

Preparation and subsequent receipt refreshes inspect native inventory and admit
only existing active managed receipts matching the approved model/context/KV
settings. Missing, changed, ambiguous or unknown outcomes fail admission.
Cumulative receipt pins cannot be replaced or removed. At most eight bounded
hash-chained checkpoints are stored; unchanged refreshes need no new checkpoint.
SQLite scans have cooperative deadlines and clear their progress callbacks.
Scope evidence excludes prompts and credentials and explicitly reports
`scope_only: true`, `runtime_bridge_connected: false`, and no response-instance
attestation. Hashes preserve local integrity; they do not attest model weights.

Standalone scope preparation does not load models, dispatch inference or execute
tools. The optional existing-model client bridge described below now connects
scope ownership to conversation inference. Optional client model activation is
provided by the separately approved on-demand mode below. OpenAI model-key replies are never represented as
native response-instance evidence. Format-1 admission scopes are refused by the
new evidence-pinned format-2 bridge; they are not silently upgraded.

The private cached native language adapter also preserves the language profile's
temperature precedence when it prunes its outer role map; explicit interface and
synthesizer profiles cannot silently replace that temperature. Language effort
remains off. Static checks only; no tests, benchmarks or live provider calls.


## Existing-model client inference bridge (experimental)

`AgentRuntime::with_existing_native_models(policy, human_approve,
allow_model_key_evidence)` opts a direct collective client runtime into a
request-owned native scope. Both approval arguments must be true. The HTTP host
exposes the equivalent explicit options alongside the approved native policy:

```sh
cogfabric-server --native-lifecycle-policy POLICY.json --approve-native-lifecycle \
  --bind-client-existing-native-models --allow-client-model-key-evidence
```

The client endpoint keeps ordinary OpenAI-compatible messages, tool definitions
and tool-call proposals. Client tools remain client-owned. Every scoped role and
registered model route must match its selected configuration, the approved
provider origin and an existing exact managed receipt. The bridge validates
inventory/receipt evidence before and after inference and requires the response
model key to match. It cannot identify the exact generating instance from an
OpenAI response: the weaker `model_key_and_inventory` evidence class and explicit
operator approval are immutably pinned in scope format 2 and its checkpoints.
A native-instance-required scope cannot be reused as weak-evidence approval.

A request acquires one private owner before reconstructing its configured pool;
the same owner is reused by the client session and scope. It is released when the
request ends. Waiting tool continuations restore the original policy, evidence
class, receipt pins, budgets and deadline. Disabling/changing the scope is refused
before claiming a waiting continuation. No additional solver call or fresh wall
budget is granted. Registered expert routes share this owner and session ledger.

Independent admitted calls share immutable scope bindings and a local uncertainty
flag, without holding a scope mutex across inference. Live sibling calls do not
trip the global abandoned-generation scanner. Errors/cancellation after intent
record an unknown outcome and block later dispatch; the durable generation ledger
also blocks new scoped requests until existing reconciliation resolves it.
A response is published only through the existing bounded collective checkpoint.

In this existing-only mode, models must already have active managed receipts.
It does not load, unload or retry models. Optional on-demand activation is a
separate mode below; general mid-reasoning recovery remains unfinished. Ordinary servers preserve their existing routing unless
these options are supplied. No local policy, installation or running provider was
changed. Static checks only; no tests, benchmarks or live inference were run.


## On-demand client model activation (experimental)

`AgentRuntime::with_on_demand_native_models` and the HTTP host's
`--activate-client-native-models` opt into loading the exact model selected by an
already admitted role or registered expert call. The mode additionally requires
operator `model.residency.change` authority. The existing-model binding option
and activation option are mutually exclusive. Both require the pinned v3 native
policy and explicit acceptance of model-key/inventory response evidence:

```sh
cogfabric-server --native-lifecycle-policy POLICY.json --approve-native-lifecycle \
  --activate-client-native-models --allow-client-model-key-evidence
```

Scope format 3 pins `on_demand: true` to the original session. Format 2 preserves
existing-only behavior. Changing modes on a waiting continuation is rejected
before claim. Scope setup observes existing managed receipts without loading
unused models. A missing selected model can be loaded only from the approved
installed model set, with the exact context and KV-cache placement settings.
No weights are downloaded or trained.

Activation commits intent before requesting a managed load, validates the fresh
receipt/inventory, checkpoints its `newly_loaded` origin, and then records
confirmation. An endpoint-wide bounded journal gate prevents another task from
adopting a load that succeeded without a confirmed scope checkpoint. Cancellation,
missing terminal evidence or errors remain unresolved; no retry or guessed
adoption is attempted. A configured working profile/GPU observation still gates
the enclosing admitted call; provider-reported memory is not physical isolation.

A shared generation lease allows independent ready calls to run together once
models are available. Existing bindings use a fast path and do not take an
exclusive activation lease. Actual loads use the exclusive side and wait for
active generation before mutating residency; the provider may also queue calls.
All waits and provider work remain subject to the original scope/task ceilings.
No throughput or response-time improvement has been measured.

Models remain available while the client executes proposed tools. After the
final response is durably checkpointed, `release_after` may release only receipts
marked `newly_loaded` by this scope, under the original owner and deadline, with
no pending activation/generation. Reused instances are retained. Errors,
cancellation, uncertainty or an exhausted cleanup allowance leave release deferred.
Managed cleanup has its own durable ledger; cleanup audit failure cannot erase a
committed response or grant permission to replay it.

An interrupted activation can be retired only after explicit operator approval
and acknowledgement that the provider operation has stopped:

```sh
cogfabric resolve-client-activation TASK_UUID --model MODEL_KEY \
  --human-approve --provider-stop-acknowledged
```

Use the original saved workspace/configuration; configuration drift is refused.
For a crashed process whose task is still running, use the existing
`recover-tasks` operation first after the host is stopped. Retirement preserves
unknown evidence, does not resume the old session, and grants no retry authority.
It does not resolve a separate managed-load receipt or generation ledger; those
keep their existing independent resolution gates. The stop record pins any
remaining nonterminal receipt IDs for that endpoint/model. New scopes cannot
adopt those receipts until independent managed handling marks them released or
retired; missing or malformed receipt evidence remains blocked. Operator acknowledgement is
not proof of provider stop or model identity. Scope and activation roots are
protected from ordinary history retention so unresolved intent cannot disappear.

Static formatting, compilation and lint inspection only. No tests, benchmarks,
provider calls, model loads/unloads, configuration changes or installation were
performed during implementation. End-to-end runtime behavior and reasoning speed
remain unverified; general intermediate reasoning recovery remains incomplete.

## Frozen typed provider prerequisites

Typed provider selection now considers the entire bounded graph before dispatch.
Each manifest dependency must match the expert ID and expert-version requirement
of a selected direct predecessor operation. Capability versions and provider-module
versions remain separate constraints. The runtime never invents prerequisite
arguments, operations or edges; missing prerequisites fail admission.

Topological joint selection can backtrack earlier provider choices, with at most
4,096 candidate probes across at most four operations. Reaching this bound refuses
the plan even if a later feasible assignment might exist. Host fallback cannot
satisfy a named expert dependency. All route choices are frozen in host-only proofs
bound to the opening catalog, task, root request and operation routing metadata.
Checked dataflow may resolve arguments without selecting another provider.

Dependent operations start only after direct predecessors produce checked results.
Independent ready operations retain configured bounded parallelism; worker limits
can still serialize them. Audits record matched prerequisite identities and hashes,
actual checked result/criterion hashes, and bounded selection effort. Permissions,
original deadlines, invocation charges and no-replay behavior remain enforced.
These proofs are internal routing guards, not independent security attestations.

Mixed neural/provider execution and interrupted graph restoration remain Partial.
Natural-language translation remains semantically unverified. Static compilation
and lint checks passed; no tests, benchmarks or live provider calls were run.
