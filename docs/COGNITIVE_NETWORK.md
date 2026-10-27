# Cognitive network implementation

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
Graph edges
do not satisfy an expert manifest's declared dependency/model requirements: those
mixed bindings remain refused. Interrupted graph restoration, mixed neural work,
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
