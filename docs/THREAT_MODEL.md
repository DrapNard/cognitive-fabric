# Threat Model (v1.0)

## Optional managed-worker kernel bounds

Linux managed workers can opt into independent CPU-time and virtual-address-space
ceilings before exec; core dumps are disabled for these bounded children. Limits
never raise tighter inherited values and do not affect the parent. Only human
configuration enables or clears them. Task settings and typed implementation
identities retain original limits during continuation. See [worker limits](WORKER_LIMITS.md).
These are per-process bounds on reviewed Cognitive Fabric workers, not limits on
the external inference server, GPU or task aggregate, and not RSS/filesystem/network
isolation. Existing task grants, deadlines and owned child cleanup remain required.


## Optional GPU telemetry and placement boundary

The NVML feature dynamically loads the driver library in a fixed same-image,
read-only child. Driver/library code remains trusted host code; this process
boundary supplies cancellation and bounded output, not a complete sandbox.
FlatBuffers validation, device/value limits and tracked kill/reap constrain results.
Whole-device measurements do not authorize capabilities or prove task energy.
Explicit KV-cache placement requires existing human-approved lifecycle operations,
applied configuration readback and pinned v2 receipts/plans. It does not establish
exclusive ownership, physical GPU limits or model-weight identity. New GPU paths
are implemented but not runtime-tested for this milestone.


## Explicit project profile boundary

Only operator publication creates immutable bounded profile versions. No model
actor has project read/publish management capabilities. An explicit task selector
permits a trusted runtime lookup, then binds its exact canonical snapshot and
context hash before inference. Project goals remain quoted untrusted background;
closed presentation preferences cannot express tools, budgets, model settings or
acceptance criteria. Default tasks load no profile. Resume validates the original
snapshot, not a later registry lookup; unknown outcomes retain existing refusal
rules. SQLite immutability/checksums provide local integrity, not authenticity
against a hostile database owner or proof of model adherence. Publication stores
text and is not suitable for secrets. All original resource limits remain active.

## Explicit Wasm registry reconstruction

Only operator-supplied, already pinned manifests supply executable bytes. New v4
task snapshots bind queue/concurrency limits alongside original identities. A
persistent unfinished nonnative task, explicit approval, exclusive lease and
bounded transaction are required. Missing/extra/substituted modules, altered
limits, uncertain dispatches and audit insertion failure refuse reconstruction.
No grants, calls or checkpoint acceptance are created by reconstruction; normal
continuation validates its own original caps/deadline/history. Native and mixed
registries, pre-v4 snapshots, publisher authentication and hostile database-owner
attestation are outside this reconstruction route. Hashes remain integrity
comparisons, not signatures or universal executable trust.

## Optional Wasm expert execution boundary

Explicit operator manifests bind immutable guest bytes, not publisher identity.
No imports/start functions or host callbacks expose files/network/model/Store
handles. Host task grants remain necessary. Memory/fuel/stack/table and shared
worker permits bound guest work; blocking jobs retain permits after caller
cancellation until interpreter exit. Typed output and aggregate limits precede
object unpacking, and host criteria remain answer authority. Implementation hashes
in v3 task snapshots prevent silent guest replacement during retained recovery.
There is no hard compilation deadline, process isolation against interpreter bugs,
or generic untrusted native executable sandbox. See `WASM_EXPERTS.md`.

## Prepared candidate provenance boundary

Pinned report bytes bind artifact and held-out hashes plus normalized source byte
lengths. Fixed no-follow bundle reads and snapshot import prevent a later pathname
replacement from changing the quarantined bytes. Byte framing treats embedded
headings as data. Strict split/duplicate checks cannot establish semantic
independence, licensing or poisoning resistance. Metadata-only provenance commits
with the inactive candidate and never counts as an evaluation. Failure rolls back
database authority; an orphan immutable quarantine blob may remain within quotas.
Retention protects association evidence, but hashes do not authenticate a hostile
same-owner database edit. No held-out bodies, executable, new capability, automatic
promotion or neural acceptance are stored through this provenance path.

## Optional benchmark device sampling

Only explicit operator comparison options enable GPU observation; models receive
no telemetry capability or command selection. One run-wide cap charges each fixed
query irreversibly. The observer drains existing bounded child cleanup and rejects
late samples from completed windows. Missing data and identity changes retain
unknown energy instead of becoming zeros or authority. Sampling is shared-device
observation with timing overhead, not physical task attribution, GPU isolation,
weight attestation, verifier evidence or permission. No result can promote a model
or increase the original inference/tool/deadline/resource ceilings.

## Local source discovery drafts

Discovery enumerates a selected directory descriptor through trusted Linux procfs,
not a mutable user pathname. Each child opens relative to its retained parent
with no-follow/nonblocking flags; symlink/special-file or bound failures prevent a
successful partial inventory. Hidden entries are excluded by an explicit counted
policy. Drafts publish hashes/paths and asserted provenance, not source bodies,
independent review or held-out authority. Preparation refuses the unreviewed
reference-only draft. This introduces no model filesystem/network capability,
candidate promotion, crawler, or sandbox/physical-I/O preemption guarantee.

## Reviewed-source preparation boundary

Only explicitly selected local manifest/root inputs reach preparation. Hash pins,
strict byte/count/path limits and directory-relative no-follow file descriptors
prevent silent substitution and traversal through symlink components. Nonblocking
opens reject special files before reads. Held-out text is excluded from reference
artifacts; exact overlap checks are mechanical and cannot establish semantic
independence, licensing, factual correctness or poisoning resistance. Output is a
new private bundle, never candidate promotion or model authority. Same-owner
filesystem changes, hard-link provenance and blocking kernel I/O remain outside
an independent sandbox/attestation guarantee. No model filesystem/network grant,
generated executable, crawler or neural training is introduced.

## Native generation-history scan deadline

The deny-before-mutation history gate uses a five-second SQLite progress handler
plus elapsed checks during bounded record validation. Expiry is refusal, never
evidence of absence; incomplete or duplicate admissions retain their prior deny
behavior. Handler cleanup occurs on every result path so inspection cannot leave
a stale interrupt hook on the shared connection. Cooperative SQLite callbacks do
not provide hard preemption of blocking filesystem operations or trusted host code.

## Explicit native continuation ownership

Only private validated native runtime objects bypass the default missing-owner
guard. Exact Store/task/policy/configuration/clock/binding checks precede ordinary
checkpoint admission, and the original exclusive owner is shared with resource
pools and process reapers. No model gains a residency capability or alternate
backend. Each new inference has a durable separate lifecycle admission; uncertain
or dropped work blocks endpoint mutation until explicit operator resolution.
Resolution cannot reconcile unknown inference or refund reservations. Fixed
builtin reconstruction still requires matching source and completed history;
custom callbacks remain trusted host code. These local receipts and source hashes
do not attest physical provider ownership, termination, weights, GPU limits or
authenticity against a same-owner database/source edit.

## Read-only native restoration inspection

The operator supplies the original policy; model text cannot choose new receipt
authority. Bounded unique source records, role mappings, same-boot remaining time,
completed generation identity and fresh full inventory are checked under exclusive
local leases. Inspection changes no model/task state and is not checkpoint
admission or provider-stop acknowledgement. The default native continuation guard
remains in force. Reported receipt consistency does not prove physical instance
continuity, weight identity, safe callbacks or authenticity against the same owner.

## Target-specific working admission

Only operator configuration declares model/role working profiles. Canonical
targets, nonoverlapping roles, bounded tables and resident-aware allocation checks
prevent ambiguous selection or expanded pool capacity. A pinned backend supplies
trusted local target metadata; the same backend is retained across queueing and
dispatch. Required profile audits precede provider generation. Unknown/unlisted
targets under an active table are refused without a uniform fallback.

Original task budgets, authorization and quarantine remain authoritative. Recovery
must reconcile complete per-reservation chains against prior same-role dispatches
and saved profile/target/allocation bindings, including overlapping waves. Neither
a new config file nor a selected profile can reset charges or certify missing
outcomes. Metadata is operator/adapter-declared, not measured loaded-weight or
physical GPU/RSS identity. No model residency-mutation grant, automatic unloading,
alternate route selection or new executable loader is added.

## Trusted host verifier extension

Optional native verifier callbacks are installed by the host, never by models or
checker manifests. Their captured declared identity and original objective/answer
hashes are audited before invocation. Existing task authorization and charges
remain required. Acceptance also requires the original compiled checker; a custom
claim cannot replace criteria or certify an invalid candidate. Matching memories
must be rechecked under the selected gate. Provider failure cannot silently remove
it, and recovery refuses provider histories until exact restoration is supported.
Interrupted callbacks may retain only an intent, never an inferred acceptance.
The recovery guard refuses oversized or malformed proof payloads before JSON
inspection. A compiled finite witness vetoed by the provider consumes its cycle
without entering ordinary failed-answer progress scoring or gaining authority.

Effective decision/history configuration hashes include the captured descriptor;
default and differently declared provider scopes cannot silently share empirical
evidence. The implementation digest is an operator assertion, not attestation.
Native callbacks are trusted process code, not sandboxed experts: they must yield
and be cancellation-safe. Dropping a future or reaching a deadline does not prove
external work stopped. No new model grant, executable loader, neural evaluation
or general factual-verification claim is introduced. See [verification](VERIFICATION.md).

## Trust boundaries

- **User interface:** human inputs are untrusted data, not system permissions.
- **Socket interface:** one local owner on a `0700` state directory with `0600` socket.
  A malicious process executing as the same OS user remains a threat.
- **Inference server:** localhost only; backend text is always untrusted.
- **Skill ingestion:** explicitly operator-selected `.txt` or `.md` with source and license.
- **Cognitive engine:** no shell access, internet crawler, arbitrary Python execution,
  autonomous privilege escalation or direct production credential access.
- **Model evolution:** new artifacts remain inactive until explicit operator approval.

## Explicit non-goals

Connected inference workers authenticate the host UID/PID and require immutable
trusted task grants before dispatch. Bounded cumulative reservations and sessions
limit repeated requests. Disconnect/shutdown drops active inference, but cannot
prove an external GPU server stopped. These controls do not attest executable
contents, isolate same-user compromise or provision grants securely by themselves.
Managed same-binary workers now receive authenticated task grants and are reaped
on task termination. Persistent workers hold shared runtime leases and operator
reconciliation requires exclusive ownership; it never kills recorded PIDs or
assumes external GPU work stopped. General executable isolation and stale-resource
cleanup remain release integration work; see `WORKERS.md` and `RECOVERY.md`.

Worker audit records use a trusted existing task anchor and bounded metadata.
Authorization decisions must persist before inference; atomic anchor checks prevent
orphan event insertion. Prompt/answer/error bodies are excluded. Abrupt termination
can leave a session open, and post-inference audit failures cannot undo backend work.

- Model alignment is **not** proven by deterministic policy checks.
- LoRA model training cannot be made safe by a checksum check alone.
- Source code forks can remove local safeguards.
- The runtime does not safely execute arbitrary third-party programs.
- The initial runtime is single-user; not a secure multi-tenant service.

## Release gates

A RAG candidate requires a held-out retrieval suite, an artifact-integrity suite,
manual review of training data and model behavior, and explicit operator
promotion. The automated integrity suite does **not** constitute a behavioral
security evaluation. Sensitive capabilities require independent human review. Neural LoRA promotion
is deliberately disabled in v1 because no candidate behavioral evaluator is available.

## Red-team test scenarios

1. Malicious text in retrieved documents is framed as reference data and receives
   no privileged execution tool.
2. A model requesting `candidate.promote` is denied by the capability kernel.
3. Oversized or malformed envelopes are rejected before dispatch.
4. No code paths execute generated shell commands or Python code.
5. Modified candidate artifacts fail integrity evaluation.
6. Recursion and model calls are hard bounded in the orchestration runtime.
7. An unavailable LLM results in a visible error, not a fabricated answer.

## Remaining gaps

- A stronger sandbox is required before adding code execution or web crawling.
- Cryptographic signatures and attestations are not implemented.
- Automated behavior/security evaluations for neural weights are not implemented.
- No isolation against an already-compromised same-user process.
- No guarantees that model collaboration improves correctness.

## Inference HTTP boundary

The adapter accepts literal loopback HTTP addresses; localhost is pinned to
127.0.0.1 without DNS lookup. Proxies and redirects are disabled. Credentials,
query strings and fragments in endpoint URLs are rejected. Responses are
limited to 256 KiB even without Content-Length, under the request timeout.
A local HTTP fixture verifies these boundaries. A malicious same-user process
can still impersonate an inference endpoint; its content remains untrusted.

RAG promotion now binds quality/regression records to artifact and dataset hashes, validates stored dataset integrity, and rejects stale baseline versions transactionally. Held-out independence is operator-attested; same-user database modification remains outside the local trust boundary. Passing retrieval tests does not establish neural safety.

### Task-local authorization
Every runtime task receives a non-deserializable immutable TaskPolicy. Grants narrow the global actor policy and bind to one task UUID. Jev+ can search references/evaluate arithmetic; Jev-R can generate, and interface roles can generate only. Session dispatches check the task and actor scope and persist policy decisions, including budget denials. Neither planner graphs, interpreted capabilities nor reference manifests construct grants. This is an in-process deterministic boundary, not isolation from compromised runtime code or the same OS user.

Published task memories remain untrusted data. Only the operator can publish or
revoke them; generic episodes retain source status, while arithmetic verification
requires independent recomputation. Snapshots have hashes and source-task links;
tampering fails retrieval. Revocation affects future snapshots, not already pinned
contexts. Poisoned text can influence model answers but cannot acquire capabilities.
The memory payload quota excludes SQLite overhead and task/event history; see
`MEMORY.md` for bounds and remaining retention work.

Interrupted-task recovery requires an exclusive OS execution lease, not a trusted
PID or elapsed-time heuristic. Live Engine executions hold shared leases; task
updates and recovery audits are transactional. Lease replacement, hard-linked
database aliases and malicious same-owner processes are outside this cooperative
coordination guarantee. Recovery never replays tools or assumes external inference
stopped; see `RECOVERY.md`.

Model registration is immutable operator-reviewed metadata, not executable code,
a permission grant or loaded-weight attestation. Only local endpoints are allowed.
Disable blocks new task selections but cannot revoke already pinned execution.
Context/output capacities and optional weight hashes are declared; actual backend
weight identity and tokenization remain unproven. See `MODELS.md`.

Progress observers receive typed, bounded metadata without prompt/answer bodies.
Subscriptions are task-scoped and runtime-owned; slow consumers receive a gap
error and cannot stall producers. Known stages/sources/states, UUIDs and optional
counters are validated from FlatBuffers. In-process code remains trusted; the
hub does not authenticate independent plugins or remote subscribers. Durable
catch-up is owner-local and does not grant model observation or tool permissions.

Socket task controls use the existing OS owner-UID authentication. Task UUIDs
identify execution; request correlation IDs cannot assign or overwrite task IDs.
Targeted cancellation compares against the current execution under one mutex, so
stale targets cannot cancel new tasks. Progress pages validate nested FlatBuffers,
task identity and monotonic cursors; wrong-target and malformed requests fail.
This adds observation/cancellation for the authorized owner, not model tool grants.

Logical database page growth is capped per configured connection, with transactional
task admission/headroom checks. Retention is operator-only, requires a reviewed
checksum-bound plan and explicit approval, and preserves running/memory/referenced
tasks. Changes and deletions share a transaction with a summary audit. SQLite WAL
retention settings are not hard disk quotas; long readers can grow the WAL. Strict
physical limits need host filesystem quotas. See `STORAGE.md`.

Evaluation retrieval uses one bounded read-only database snapshot per comparison,
preventing concurrent source updates from giving different arms different evidence.
The image remains in process memory; reports expose only its hash/size and capture
time. It retains the original reviewed corpus for that run, including subsequently
revoked entries; cancel the run if that evidence must no longer be used. No model
permissions or expected test outputs are added to inference contexts.

Operator-only GPU diagnostics may run one fixed `/usr/bin/nvidia-smi` read-only
query with cleared environment, bounded output/time and child cleanup. No runtime
model/tool API selects executable paths or arguments. The installed utility/driver
is trusted host software; this is not sandboxing or task-attributed GPU accounting.
No GPU setters, weight loading or new model permissions are introduced.

### Opt-in inference credentials

The operator can bind a dedicated environment bearer credential to one validated
local API scope. Registered model endpoint changes do not inherit authorization
for another endpoint. Workers receive only this credential after clearing their
environment; bootstrap contains scope metadata only. Exact raw/decoded echoes are
rejected before task persistence. This does not defeat transformed exfiltration by
a malicious provider, same-user process inspection or administrator access. See
`AUTHENTICATION.md`. Authentication configuration remains human-only.


Host-approved event-module streams now authenticate expected UID/PID credentials
on both ends and bind every FlatBuffers frame to one task, module version and
stage grant. Incoming command bytes, wrong grants and malformed packets are denied;
overflow/timeouts end observation without cancelling task execution. Trusted host
code still provisions streams, grants and child lifecycles. No executable integrity,
same-owner isolation, general module command authorization or automatic remote
registration is established by this observer transport.


Arithmetic process commands (`src/fabric/tools/unix.rs`) require host-provided,
non-deserializable task/version/call/session grants and approved UID/PID peers.
Requests cannot create grants. Authorization audit failure prevents execution;
malformed/uncorrelated exchanges and duplicate correlations fail closed. Cancelling
an exchange discards its stream, and disconnect drops pending backend futures.
The host controls socket permissions, child creation and grant provisioning.
This transport does not isolate executable code, attest backend identity beyond
OS credentials, stop arbitrary external side effects, or protect against a
compromised same-user host. Tool-worker lifecycle automation remains unfinished.


Managed arithmetic provisioning is now opt-in through the human-only
`tool.worker.configure` capability. The launcher accepts only its current Rust
executable, clears inherited environment, requires a matching private persistent
workspace and gives exactly one arithmetic call through authenticated COTB setup.
Original host tool allowances and deadlines remain authoritative. Ownership survives
cancelled/failed startup and normal cleanup confirms child reaping. Runtime crashes
can still leave stale resources; same-user code and database access are not sandboxed.


SQL capability modules (`src/fabric/tools/sql.rs`) receive typed COGQ tables/rows/
cells with frame, verifier traversal and decoded-byte limits, including shared
FlatBuffers reference amplification checks. Registration is a trusted Rust API;
metadata cannot grant capabilities. The default SQL backend retains inline-only
SQLite authorization and resource limits. The host reruns that compiled checker
before accepting replacement output as evidence. A replacement Rust backend is
trusted code, not automatically isolated; verifying its answer does not prove
absence of side effects. SQL Unix/process provisioning is not implemented yet.

SQL Unix transport now authenticates exact host-approved UID/PID and enforces
non-deserializable task/version/call/session grants. Authorization audit failure
prevents backend execution; replay, malformed responses and cross-capability
grants fail closed. Cancellation closes streams and drops pending futures, with
cooperative SQLite cancellation for the built-in backend. Metadata excludes SQL
and result/error bodies. This supersedes the SQL transport limitation above;
automatic SQL worker provisioning, executable sandboxing and crash cleanup remain
incomplete. Trusted hosts still supply sockets, grants, backends and child ownership.

Managed SQL setup is now human-only through `tool.worker.configure`; opt-in
`sql_process` defaults false. The shared launcher accepts only its current Rust
executable, clears inherited environment, uses private authenticated sockets and
provides one task/version/call grant via SQL-only COTB v2. Arithmetic v1 grants
remain arithmetic-only. Original permission/tool budgets precede launch; host
SQLite checking verifies outputs. Child ownership survives failed startup and
normal cleanup confirms reaping. This supersedes SQL provisioning limitations;
trusted worker code still has workspace access, so no sandbox or crash-cleanup
guarantee is established.

Inference worker ownership now persists through interrupted connection/bootstrap
and audit failure. Launch intent precedes spawn; owned PID precedes handoff; failed
initialization cannot silently retry. Normal shutdown confirms reaping of retained
startup children with existing bounds. This improves live-runtime failure
containment; host death, stale directories and provider GPU work remain outside
that guarantee. No additional model capability, credential or executable is granted.

Operator worker recovery now repairs abandoned arithmetic/SQL audit sessions
under the same exclusive lease as inference. Live workers block reconciliation;
all metadata validation and audit insertion share one bounded transaction. Tool
format/capability/version and initial charges are validated, with bounded legacy
format compatibility. Results and charged events are preserved. Interruption
records explicitly leave resource completion and external-effect stopping unknown;
recovery neither replays commands nor kills PIDs or removes files.

Human-only `worker.cleanup` permits reviewed removal of journaled same-binary
socket identities under exclusive runtime ownership. Plans bind workspace and
exact directory/socket identities; symlink/type/mode/extra-file checks precede
nonrecursive descriptor-relative deletion. Intent audit failure prevents removal;
post-removal audit failure cannot undo filesystem effects. Linux procfs and a
root-owned sticky `/tmp` are required. This is cooperative cleanup, not isolation
from malicious same-owner races, a general filesystem tool or proof that external
GPU computation stopped. Unjournaled resources are never guessed or swept.

Inference transport registration now validates the adapter's fixed module
ID/version/role descriptor before accepting it, and initial discovery preserves
that descriptor. A connected peer cannot be accidentally advertised under another
module's roles/version by the standard broker API. This is a consistency check
inside trusted host code; implementations can lie and same-user/executable or
loaded-weight attestation remains outside the guarantee.

## Optional agent HTTP boundary

`cogfabric-server` is a separate loopback-only front end using a dedicated bearer
key, rejecting browser origins, bounding accepted connections, request bodies,
concurrent parsing and active completions. It does not provide remote or tenant
isolation. Native conversation roles and client tool results are preserved as
explicit typed data; they cannot construct permissions. Only model generation is
granted to native solver turns. Declared client tools produce proposals, not
host execution; the connected client must enforce its own schema and action
permissions. Fabric does not sandbox OpenCode or control tools it executes.
Native logs retain hashes/usage rather than raw source conversations.

Streaming and non-streaming disconnects drop pending local inference; provider
GPU work is not proven stopped. A client may initiate further independent turns,
so session-wide client step ceilings remain a separate control. The native
adapter refuses role overrides it cannot preserve. Native
turns snapshot an enabled immutable model registration before admission and
recheck model/endpoint/output-ceiling consistency. Disabling a registration
blocks later snapshots while admitted turns preserve theirs; this is metadata
pinning, not provider weight attestation. Existing collective FlatBuffers worker security is unchanged.
See HTTP_API.md for exact limits and unsupported routes.

Native managed conversation workers now use a separately verified FlatBuffers
format and trusted one-task/one-generation grants over UID/PID-authenticated Unix
connections. Proposals confer no client-tool execution authority. Authorization
must persist before backend construction, including audit-failure and pipelined
input denials. Scoped inference credentials alone are forwarded into cleared
worker environments; server API keys and raw conversation bodies are excluded
from audit metadata. Shared execution leases block recovery while workers live.

Parent ownership includes a native process-capacity slot until reaping (eight per
host process). Normal cancellation waits for the owned child; dropping an entire
runtime future schedules bounded cleanup while retaining ownership. Runtime/host
death or unavailable audit storage may leave completion unknown; recovery never
claims GPU work stopped or kills a recorded PID. The same-image process boundary
is coordination, not executable attestation or a hostile-code sandbox.

### Dropped collective task owners

Closing a client connection can drop the Engine future before its explicit
shutdown code executes. Collective process owners now close their adapters and
retain child/socket/execution-lease ownership in bounded reapers. HTTP shutdown
waits for collective ownership as well as native ownership. Stop-audit failure
is sticky, blocks subsequent collective worker construction and fails shutdown.
Actual subprocess regressions cover disconnect, immediate shutdown and injected
audit failure. Forced runtime/host termination and upstream GPU completion remain
unproven; the process boundary still does not isolate arbitrary untrusted code.

### Residency observation is not ownership recovery

`residency-inspect` uses a current-schema read-only database handle and exactly
one operator-scoped inventory GET after validating bounded recorded intent and
its explicit endpoint. Provider state cannot prove request causality or exclusive
ownership. Inspection never changes task state or grants retry/unload authority.
Stored events remain local consistency evidence, not signatures against same-user
database compromise; external inventory can race other clients.

### Trusted executive routing extensions

A host-installed `ExpertRouter` may select only distinct enabled IDs from a
pinned task snapshot, within the existing two-route ceiling. It cannot return
permissions, budget overrides or executable commands. Policy audit precedes
invocation; actual expert/tool checks remain independent. Native callbacks are
trusted host code and cannot be preempted or sandboxed by this interface; direct
external I/O/model calls, blocking and arbitrary allocations are outside its supported
metadata-only contract. No model or remote API can install a callback. Untrusted
policy loading needs separate isolation and review. Logical versions are declared
metadata, not cryptographic implementation identity.

### Skill metadata allocation and admission

Skill inventory reads now cap row count and SQL field bytes before Rust copies,
then reject invalid tags, IDs, enable state and versions. Malformed tag JSON is
not downgraded into an empty list. Shared transactional admission prevents legacy
registration from bypassing the manifest inventory's retained-ID bound, including
competing connections. Invalid stored records remain unchanged and fail closed.
These are resource/consistency checks, not authenticity against an owner able to
rewrite the database; no executable or model permission is derived from metadata.

### Manifest allocation and active-version ambiguity

Registry reads now cap stored bodies, checksums and identity fields before Rust
copies, then validate hashes and embedded identity. Streaming active-graph reads
reject excessive or duplicate identities even if a corrupted/restored schema
lacks its expected uniqueness index. Idempotent registration verifies the stored
body instead of trusting a retained checksum alone. Errors leave corruption
unchanged and deny activation; no automatic repair or neural promotion follows.
An owner who can rewrite content and hashes still defeats local consistency
checks, so these bounds are not signatures or executable attestation.

Operator-directed disable excludes only its explicit target from graph validation
so body/projection corruption cannot prevent containment. The remaining graph
and reverse dependencies must still validate. Enable change and audit commit
atomically; audit failure rolls back. No corrupt body is trusted or rewritten,
no dependent cascade occurs and normal activation stays strict. Models receive
no disable capability through this change.


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


Declared expert capability aliases are immutable versioned metadata, not grants.
Lookup is human-only and disclosure flags deny health/permission attestation.
Advisory intent resolves only pinned enabled providers within existing route
ceilings, preferring reviewed providers over legacy category placeholders.
Sensitive names cannot modify compiled task policy. Invalid aliases fail registry
validation; larger optional interface prompts are audited and skipped without
truncating the catalog or increasing solver allowances. Untrusted executable
provisioning/isolation remains outside this metadata feature.


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

### Experimental Core-backed scalar module audits

The optional Wasm bridge admits only trusted operator Rust embeddings. The actor
label is checked by deterministic policy; this is not remote user authentication.
Core owns fresh task IDs, immutable approved artifact identities, original deadlines
and a live recovery lease. SQLite immediate transactions reject duplicate allocation
slots and invalid dispatch/outcome sequences, including attempts by independent
executor wrappers sharing the context. Ledger reads are bounded to 25 rows and
4096 bytes per record and fail closed on malformed/overflow evidence. Scalar values
and guest diagnostic bodies are excluded. In-memory acknowledgements are not disk
durability. Cancellation denies further acknowledgements; already executing guests
are fuel bounded rather than forcibly terminated. Trusted host Store access can
alter state and is outside the guest threat boundary. No production execution
endpoint, artifact signature trust, domain correctness or independent sandbox audit
is established by these tests.

### Advisory crash-result reconciliation

The operator-only advisory recovery operation writes no new inference or tool
reservation. It requires an exclusive runtime lease, one running advisory node,
a latest fully recorded cycle-zero solver call after that node started, and a
valid original continuation checkpoint. The prospective unverified graph result
and metadata audit commit together or roll back together. Missing/uncertain
outcomes, changed hashes/schedules/context, expired deadlines and unsupported
phases deny reconciliation. Advice remains untrusted and continuation needs a
separate approved operation. No new actor capability or provider-stop claim is
introduced. Recovery preserves typed kernel lease errors; tests distinguish
transient lock contention from malformed history and live-owner denial.

### Expert admission fallthrough

Only a durable refusal before workflow admission permits trying another already
routed, pinned candidate. Admission limits and task authorization remain enforced
by Rust. An admitted timeout/partial answer does not grant retries; generic
provider failures and failed refusal audits remain fatal. No new actors, shell,
network or filesystem capabilities are granted. Budget-phase metadata is not
proof of provider execution or model utility.

Expert workflow admission is now durably recorded before constructing a bound
backend/worker handle or allocating its phase. Refused candidates cannot emit
an admitted model snapshot or enter backend setup. Immutable prepared prompts and
DAG orders are internal Rust values; no deserialization grants execution. Registry
identity validation and per-call policy enforcement remain separate. Admission
is intent, not attestation of provider entry, output correctness or loaded weights.

Local broker load observations are bounded metadata, not authorization. A module
has at most four buffered requests and one processed request. The processing
observation uses scoped cleanup, including cancelled/shutdown work. New routing
uses sampled local load only after operator rank; existing pins and failure
handling are unchanged. IPC accepts absent legacy observations as unknown and
rejects counts above four or partial observation pairs. Remote inventory values
do not overwrite the broker's internal counters or grant model/tool permissions.

A failed local inference worker may leave its underlying backend object reporting
healthy. Closed queue state now dominates that label for routing, inventory,
activation and dispatch. No old-pin retry or resource refund follows. Availability
callbacks execute outside registry locks, and activation rechecks retained identity
and current constraints afterward; retirement cannot be undone by a stale health
sample. Tests deliberately panic a trusted callback inside its Tokio worker and
exercise metadata reentry/retirement races. This is not OS isolation: panic-abort
builds, arbitrary native code, external provider execution and blocked callbacks
remain outside that limited containment claim.

The closed-worker lifecycle protection also applies to arithmetic and SQL tool
queues. A failed old handle is never redirected to another implementation, and
no budget is refunded. Only future approved handles can select a healthy registered
alternative. Tool observation callbacks run outside registry locks; final activation
rechecks retained identity and constraints. Deliberate trusted callback panic tests
exercise this local Tokio-worker behavior without claiming process isolation,
provider cancellation or permission expansion.

### Reviewed empirical retry evidence

The optional Jev+ retry policy accepts bounded operator-reviewed observations and
immutable thresholds, not model confidence. It applies only to a matched compiled
criterion, mode, cycle, configuration, role prompts and runtime-source digest.
Failed attempts remain in cost and success-rate denominators. Unknown scope and
unverified prose cannot use this evidence to buy inference. The original kernel
still checks every call/tool reservation, deadline and actor grant; an empirical
allow result cannot enlarge them or certify an answer. Only the existing
independent checker may mark a revised answer verified.

Held-out review, outcome independence, study completeness and model identity
remain operator responsibilities. Source/content hashes detect drift, not forgery
by a database/source owner or executable/provider substitution. Observed duration
and success rates are not statistical predictions or future resource guarantees.
The host-installed policy is immutable and custom-policy recovery still refuses
unavailable implementations. `retry-study-evaluate` is human-only bounded file
inspection before state initialization, not policy installation or promotion.
No new model actor capabilities, executable loader or external endpoint is added.

### Explicit restoration of pinned executive policy

Custom-policy continuation accepts only an operator-supplied native implementation
whose cached descriptor matches every original snapshot. Empirical CLI restoration
constructs only the existing compiled policy from bounded reviewed data. It never
loads code or infers authority from a policy name. Legacy compatibility is a
runtime-owned handle property, not a serialized ID. Revised custom checkpoints
require original matching intent/allow records with unchanged candidate, resource
chain, configuration, prompts and runtime-source fingerprints. Denial, ambiguity,
missing records and source drift prevent new dispatch; committed choices are not
reinvoked. This is owner-local consistency, not native executable attestation or
proof that operator review or provider identity is correct.

Store locks are released before callbacks; approval and execution leases remain
mandatory. Audit failure rolls admission back, and ready cancellation makes no
new inference reservations. CLI resume opens only a current existing database;
no schema migration, bootstrap, model/tool capability or new budget is inferred.
General custom-policy crash reconciliation and automatic policy catalogs remain
outside this restoration path.


### Shared inference allocation and uncertain provider work

Native operator settings establish one process-local FIFO resource pool. Models
cannot configure capacities or resolve retained allocations. Wrapped calls reserve
all declared resource dimensions together under bounded queues and deadlines;
original per-task authorization/budgets still precede actual inference. Mandatory
metadata audits precede dispatch, and successful completion must be recorded before
release. Audit/provider errors, timeouts and dropped dispatched futures retain
capacity in quarantine; cancellation is not provider-stop evidence. Operator
resolution requires explicit acknowledgement plus a committed audit outside pool
locks. Pending/undispatched cancellation releases capacity and preserves fairness.

These are declared allocations, not CUDA/cgroup/RSS limits or weight-residency
attestation. Pools neither control other processes nor reconstruct ownership after
a crash. All competing backends must be wrapped by the trusted host. Normal task
continuation refuses original pool usage rather than bypassing unavailable limits.
Explicit restoration, durable shared allocation and actual GPU scheduling remain
incomplete. No default CLI/HTTP installation, automatic provider stop, new model
capability or untrusted executable policy is introduced.


### Configured resource ownership and restart reconciliation

Operator resource config now covers collective roles, bound experts, managed
inference and native HTTP turns through one shared configured pool. Engine clones
retain pool identity; settings cannot silently reset on a live Engine. A separate
private resource lease excludes a second configured owner in the same workspace.
Strict bounded lifecycle validation rejects unresolved or inconsistent prior
dispatches at startup. Read-only status gives sampled identities, not health.
Offline release requires explicit operator stop acknowledgement, exclusive resource
and runtime leases, exact audit matching and a committed release. It kills no PID
and provides no measured provider-stop proof. Retention validates task ledgers
and preserves unresolved resource evidence; deletion excludes live configured hosts.

Configuration changes require service restart. Different workspaces, explicit
native/unwrapped backends and operator removal of config remain outside this
workspace admission guarantee. Resident weights, actual GPU allocation and host
resource isolation are not enforced. An operator can rewrite local records or
source; hashes and audit checks do not authenticate against that owner. Original
configured task continuation remains denied without explicit pool restoration.


### Declared resident-backbone charges

Operator catalogs reserve static host/device memory in the pool before working
inference admission. Each declared immutable backbone identity is counted once
across its listed consumers. Canonical, bounded metadata validation rejects
duplicates, malformed identities/digests/endpoints, impossible totals and live
catalog changes. Request completion, cancellation and operator release cannot
refund the resident base. Catalog hashes and totals accompany resource intents;
configuration snapshots retain the declared catalog. Empty catalogs retain prior
configuration serialization. No model actor gains configuration authority.

These are operator declarations, not provider observations or proof of actual
sharing/completeness. Consumer names do not constrain actual backend selection,
load weights, grant inference or contact endpoints. A local owner can omit memory
or misstate a digest; `residency_attested` and `physical_limits_enforced` remain
false. Static accounting is not a substitute for measured instance ownership,
physical limits or safe model load/unload. Existing cancellation/quarantine and
restart gates remain unchanged.


### Resident consumer coverage at inference dispatch

Nonempty catalogs now require a canonical selected-adapter endpoint/model match
or a trusted deterministic-backend classification. Defaults are unknown, not an
inferred target from task configuration or provider text. Local/managed adapters
report their actual immutable role/native configuration; pinned brokers retain
the same module through observation and dispatch. Target audit failures and
unknown/unlisted/invalid identities stop before provider entry or child launch.
Session charges are retained, and raw invalid metadata/error bodies are not written
to coverage events. Callback reentry tests exercise Store/pool/registry access
without held locks. Empty catalogs retain their prior behavior.

This narrows accidental memory-declaration omissions; it does not authenticate
provider execution, weight hashes, physical usage or native host honesty. Raw Unix
adapters remain unknown until a host supplies a proper identity contract. Metadata
callbacks are trusted nonblocking code, not an isolated model or protocol grant.
No model output may classify its own backend or expand resident coverage. Dynamic
provider ownership, measured residency and physical enforcement remain incomplete.


### Restoring configured resource pools at ready checkpoints

Continuation now validates a bounded strict resource-call ledger against original
execution settings and canonical resident coverage. Charged calls require ordered
matching admission/dispatch/successful completion; denial, missing outcomes,
identity/source drift, ambiguous keys and unresolved workspace dispatches prevent
restoration. Post-context role targets bind to the saved default/role configuration.
Read-only preview probes ownership; approved resume prepares the compiled pool
outside Store locks and revalidates all settings/history in the write transaction.
The original task clock and budgets remain in force, with one bounded admission
window across both validation phases. Audit failure prevents backend/worker entry.

Only the configured Rust pool is reconstructed; arbitrary native policy code and
unconfigured pools are never inferred from stored names. A release acknowledgement
is not proof of a completed model answer. Catalog/ledger consistency remains an
owner-local claim, not provider-stop, weight identity or physical resource proof.
Current CLI configuration cannot enlarge a saved task's allocation. Dynamic load/
unload, general within-call recovery and cross-workspace physical control remain
incomplete.


### Reviewed residency-plan application

The deterministic planner may reuse known exact contexts and propose only explicit
release IDs and missing installed model loads. Application is human-only, requires
explicit approval, verifies the explicit native endpoint and fresh projected
inventory, and atomically records a one-attempt receipt before effects. Each bounded
step checks operator policy, audits its child operation and verifies the expected
projected inventory change. Failures/drift/timeouts stop later steps without retry
or rollback claims. Receipt and linked task retention prevent deletion from silently
resetting the gate; receipt capacity/SQL work and mutation duration are bounded.

All native residency mutations now hold both resource ownership and runtime leases;
an idle configured pool also prevents mutation. Other applications/providers need
not respect these workspace leases. Inventory hashes cover known key/type/instance/
context fields, not weights or unknown metadata. A cancelled process may leave an
uncertain POST and requested/running audits; inspection is not resolution or provider
stop proof. Native Rust callers are trusted, and model actors gain no API/grant.
Receipt retirement, autonomous owned residency, physical control and independent
model quality/safety evaluation remain incomplete.

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
The HTTP server also accepts the paired host startup options, validates them before
state/backend initialization and pins the policy for collective aliases. Native
conversation turns do not use it, HTTP requests cannot replace it, and worker entry
points reject the host flags. Pre-routing checkpoint continuation remains incomplete.
See [workflow-policy deployment](EMPIRICAL_WORKFLOW_ROUTING.md#direct-cli-and-local-service-deployment).

### Bounded finite-domain evidence

`finite_domain/v1` checks explicit user-supplied domains and linear/all-different
constraints using deterministic Rust and bounded i128 arithmetic. It executes no
expression language, solver executable or generated program, and accepts no model
claim of impossibility. Strict JSON, byte/structure ceilings and exact declared-key
matching prevent ambiguous criteria and assignments. `constraints.verify` is a
reasoning-task Jev+ grant; generation-only agents and model roles cannot invoke it.
Per-check audit and budget rules, original-query memory rechecks and immutable
criterion identity remain required. Passing means only satisfaction of the supplied
finite specification, not external truth, optimality or general neural safety.

### Recovery errors and inherited lease ownership

Recovery preserves OS error types and unsafe-file validation causes beneath
operation-specific context. Failure to acquire ownership still prevents all
recovery mutation; an error alone is not proof of a live engine. Forked children
can retain close-on-exec descriptors until exec or exit. Runtime acquisition remains
nonblocking and never forcibly unlocks a surviving description or removes the lease
file. Controlled inheritance/no-write tests cover the boundary; a test-only process
launch gate isolates unrelated fixture descriptor inheritance. This provides no
external provider-stop or malicious-owner guarantee.

### Joint capability metadata resolution

The bounded complete-cover resolver uses only pinned enabled declaration metadata and
existing model-registration availability. It grants no tools, loads no executable
and performs no provider contact. Recorded availability is a prerequisite check,
not endpoint health or weight attestation. Selection cannot exceed four routes or
override profile exclusions; original workflow/call/tool/resource gates still apply.
Audit failure stops before refinement and expert admission. Later registry changes
affect future snapshots. Missing, unavailable and unselected requests remain
separately observable without automatically creating a candidate or new permission.

### Capability-directed pinned references

A detached read-only SQLite snapshot is created in the initial task-metadata
transaction and never handed to a model. Source locks end before inference. Scoped
search preserves enabled/approved-version filters and content integrity; later live
changes cannot replace its documents. One task-authorized search charge, bounded
providers/snippets, main-solver/verifier reserves and pre-context audits govern
refinement. Added data stays untrusted and extends empirical workflow evidence
scope. No capability/weight promotion occurs. The 64 MB snapshot ceiling and prompt
bounds are logical admission limits, not physical memory or hard I/O timing proofs;
large stores skip optional refinement. Pre-interpretation snapshots are not crash
restoration authority. No external fact, relevance or neural-safety claim follows.

### Optional interpretation cannot consume the last solver reservation

Independent Rust preflight requires interface-plus-solver calls, outputs and prompt
bytes. The original deadline is retained under an audited optional phase ceiling.
Only a typed inference timeout permits fallback with charges retained and unknown
usage; policy/provider/audit failures and cancellation remain fatal. Additional
interpretation text is admitted before changing solver context. No model assertion
can create a timeout allowance, refund resources or expand capabilities. Provider
termination and physical resource release remain unproven; configured quarantine
still governs subsequent dispatch. See [interpretation budgets](INTERPRETATION_BUDGETS.md).

### Operator role priority under existing resource ownership

Only reviewed native/operator resource configuration selects role priorities and
bypass limits. Runtime roles determine priority; model text does not create urgency
or permissions. Queue selection changes no allocation or resident budget, never
preempts dispatched work and never releases quarantine. Bypass protection limits
later reservations overtaking older work, not wall latency or provider execution.
Original policy and bounded recorded queue metadata are required for continuation;
FIFO rejects injected priority fields. Existing workspace ownership prevents a new
policy host from silently resetting live counters. No cross-workspace or physical
GPU/RSS enforcement, retrospective global fairness proof or neural safety is added.

### Advisory capability version filtering

The optional version map can only narrow already requested capability IDs. Strict
bounded parsing rejects unknown keys, invalid ranges and ambiguous JSON. Version
matches use immutable pinned declarations, not model confidence or live activation;
wholly incompatible routes cannot regain optional workflow admission through tag
selection. Reference refinement checks compatible bindings. Compatibility labels
remain declaration-only and confer no executable/API/weight attestation, activation
or permission. The original task criteria and budgets retain authority, and the
changed interface prompt remains subject to existing checkpoint fingerprint gates.

## Bounded candidate recurrence (2026-10-20)

Jev-R requires a durable progress audit before escalating a rejected/unverified
whole-goal candidate. First-per-cycle hashes detect nonadjacent recurrence without
trusting critic assertions or counting recursive-ablation advice as new answers.
Independent verification precedes this heuristic. Continuation rebuilds history
from validated original nodes and refuses recurrence; revision reconciliation
rolls back its prospective checkpoint on refusal. No budgets or capabilities are
granted. Hash equality is only an exact-content stopping heuristic, not proof of
unsatisfiability, semantic progress, answer quality or external provider stopping.

## Configured graph priority (2026-10-20)

The optional critical-path scheduler operates only on the validated bounded
planner DAG. Unit chain heights are advice-derived priorities, not authority or
verified cost/utility. Ties use canonical IDs; models can suggest identifiers and
edges but cannot add budgets, permissions or executions. Existing intent/decision
audit failures prevent subtask dispatch. Saved orders remain authoritative on
continuation; a current config change cannot reorder admitted work. Default
serialization remains compatible and explicit policies bind configuration scope.
This does not expand arbitrary callback loading or imply optimal/physical scheduling.

## Bounded alternative strategies (2026-10-20)

Explorer portfolios are strictly parsed untrusted data, not instructions or model
permissions. Unique bounded IDs and normalized distinct text only establish
mechanical shape, not semantic diversity, utility or safety. Malformed structured
responses cannot downgrade to legacy plain-text advice. JSON-framed selection
stays inside the original total strategy cap; acceptance/selection audit failure
prevents subsequent solver work. Independent checks and original budgets retain
authority. Restoration uses one validated original cycle-zero explorer result,
not a model-selected new policy or advisory audit status. It preserves original
custom escalation prompt-byte coverage and rejects ambiguous explorer histories.
The changed compiled role prompt is subject to existing checkpoint/evidence
fingerprint gates. No model call, grant, neural promotion, external action or
provider-stop proof is introduced. Native callbacks are still trusted host code.

## Hypothesis diagnostic boundary (2026-10-20)

Evaluation copies fixed labels from bounded SQLite projections rather than
portfolio/selection payloads. Recognition requires expected local source/version,
non-authority flags, policy and bounded metadata shape; unknown observations are
unclassified and excess/malformed records fail the report. These counters do not
validate replay, grant authority or prove execution/quality. Repeated recovery
selection audits remain repeated observations. Compiled source/prompt fingerprints
identify scope without attesting an executable, callback or provider weights.
No task budgets, permissions, data retrieval or execution policy changes.

## Recent-advice guard boundary (2026-10-21)

The opt-in guard reads only bounded native observation metadata; external documents
and model assertions cannot set outcome/cost or grant authority. Decision and
post-attempt observation audits must persist. Unknown/malformed/oversized inspected
observations fail before subsequent inference; valid unrelated/stale/other-boot/
future samples are not borrowed. Default absence does not activate history reads.
Config, compiled prompt/source scope, declared modules, profile, criterion and size
bucket are bound, without claiming semantic or physical model identity. Tickets
retain elapsed admission/audit time and never reset task/phase budgets. Skipping
optional work never adds permissions, calls, tokens, cycles or provider-stop proof.

This is a forgetful, concurrent operational heuristic, not a security quota,
signature, calibrated probability or independently proven expected utility. Missing
cancelled/crashed outcomes are not successes. Retention/tail displacement can permit
new probes; same-user tampering and blocking trusted callbacks remain threats.
Comparison arms have separate profiles, but their own adaptive history is not
frozen by corpus capture and is explicitly disclosed. No schema/IPC replacement,
provider lifecycle action or neural promotion is added.

## Finite-domain progress stopping

The optional policy derives bounded violation counts from the same compiled finite
specification, never from model confidence. Complete in-domain assignments alone
receive comparable scores; invalid output cannot fabricate progress or verified
success. Full task-authorized independent checking precedes stopping, and progress
audit failures prevent later inference. Checkpoint counters are recomputed from
recorded solver outputs and matched failed checks under the original configuration,
leases, deadlines and charges. Models cannot change the threshold or reset history.
This search heuristic can stop potentially useful future work; it establishes
neither impossibility, general reasoning quality nor external provider stopping.
No new capability, generated execution or neural promotion is introduced.

### Explicit progress-policy evaluation

The ablation runner toggles only the optional stopping setting between two
collective arms under the same original ceilings. Its output grants no permission,
changes no persistent policy and promotes no model. Expected assignments and
satisfiability labels remain outside model contexts; success requires an actual
checked witness. An enabled adaptive advice guard is refused rather than borrowing
unfrozen history. The shared resource pool retains uncertain reservations; the
experiment cannot release them or claim provider termination. Provider weights,
physical resource use, cache state and independent curation remain unproven.

## Opt-in finite-domain search

A separate task-scoped Jev+ `constraints.solve` grant admits deterministic bounded
search only over the original strictly parsed CSP. At most 4096 visited nodes,
bounded depth/domain/constraint structures and deadline polls limit computation;
this is a logical ceiling, not hard process isolation or physical CPU accounting.
Model roles cannot invoke the search or change its node limit. An actual witness
requires a separately authorized satisfaction check. Only exhaustive traversal with sound impossible-branch pruning
permits a contradiction outcome, which remains rejected under witness scoring.
Limit exhaustion cannot manufacture impossibility or refund tools. Required
audit failure prevents subsequent dispatch. Enabled saved finite-domain tasks cannot continue
until traversal receipts have a validated recovery format. No external solver,
generated code, new network access, active-weight change or neural promotion is
introduced. See [bounded search](FINITE_SEARCH.md).

### Restoring completed search-limit receipts

Continuation may inspect a bounded strict original search authorization/charge/
receipt chain for a `limit_reached` result only. Metadata must match the original
finite objective/configuration, unique charge, exhausted cap and event chronology
before the checkpoint. Receipt flags cannot confer verification or permissions.
The resumed Rust session uses a saved strategy and retained accounting; no search
code is replayed and no prior charge is refunded. Audit/admission failure prevents
new dispatch. Enabled-but-skipped, incomplete and terminal search phases remain
refused, as does saved-result reconciliation. This is local consistency checking
under cooperative ownership, not a signature or proof against same-owner edits.

### Saved model candidates following search limits

The operator must supply a task identity, never a replacement answer or criterion.
Receipt/ledger validation precedes a fresh original-budget verifier reservation.
Only the compiled original criterion can establish a witness, including a candidate
produced during advisory work. A failed advisory check has no whole-goal progress
authority and must match an earlier recorded solver/rejection. Rechecks retain
their charges and cannot duplicate assessment authority. Original-clock and normal
checkpoint gates govern any subsequent inference; proof reconciliation cannot
reset those gates. Transaction failure rolls back the new proof, charge and
prospective boundary. No search/model work is repeated and incomplete/terminal
search outcomes remain unsupported. Local audit consistency is not authenticity
against a hostile owner or evidence of provider stopping.

### Direct witness receipt recovery

A version-2 direct-search receipt contains a bounded assignment/hash in one
private task event. It remains unverified data: recovery validates its original
settings/context, unique authorized charge lineage, bounds and later phase before
reserving another independent checker call. Original reservations are never reused
or refunded. Only a fresh complete-criterion check permits terminal completion.
Malformed data fails before writes; proof/audit failures roll back the new charge
and status. The recovery report omits the body, and typed progress frames retain
their existing metadata-only schema. No search/model replay or new authority is
introduced. Body-less legacy witnesses, contradictions and pre-receipt crashes
remain unsupported; consistency checks do not authenticate against a hostile owner.

### Exact complete declared capability coverage

Jev+ may use a bounded mask search to find the fewest additional compatible
providers covering all requested declarations under its retained four-route
ceiling. Selected initial routes, disabled/version/unavailable filters and task
permissions remain authoritative. A complete declared cover grants no tool, loads
no executable and establishes no endpoint health or answer quality. If no full
cover fits, partial greedy fallback remains heuristic. Fixed audit metadata is
required before route refinement or expert admission.

At most 4096 eligible providers, 16 requirement bits, 8,386,560 pair visits and
two compact 65,536-entry tables bound logical work. Yields before/after synchronous
resolution let cancellation/deadline win before publication, not during the loop;
this is not hard CPU isolation or a physical latency guarantee. Original task
budgets cannot be increased. No model confidence, retrieved instruction or mutable
live registry can change the pinned declarations, limits or permissions.

### Partial finite-domain pruning

Only deterministic necessary conditions from the original strictly parsed
criterion prune search branches. Signed interval bounds over declared domains
and duplicate assigned values carry no model authority. Sparse-domain interval
feasibility is not a witness; complete candidates retain the separate original
checker. Bounds, cancellation polls, reservations and receipt formats are unchanged.
Earlier logical contradiction detection does not establish physical resource
limits, general impossibility outside the criterion or neural safety.

### Bounded advisory concurrency

Operator-selected parallelism groups only validated pinned advisory nodes with
completed dependencies. Whole-plan preflight and per-call durable authorization/
reservation precede backend polling; parallel scheduling cannot expand budgets
or capabilities. Shared pool limits and original deadlines still apply. Locally
owned futures are dropped on cancellation, without claiming remote execution
stopped or refunding unknown work. Recorded results remain unverified advice.
Strict wave/chronology validation governs graph observation. Operator-approved
continuation may restore complete recorded wave prefixes after unique charged
call/result reconciliation; unknown or partial waves remain denied. Owner-local
metadata is neither a code signature nor independent model quality/safety proof.

## Initial route eligibility and evidence scope

Operator-declared profiles/residents constrain initial Broker selection. Required
routing intent commits before trusted metadata callbacks; bounded decisions use
fixed reason codes without copying callback errors. Registry identities are
revalidated after observation. Selected routes stay pinned and no failure grants
a retry, refund, new permission or residency mutation. Final pool admission remains
independent. Metadata is still a trusted adapter declaration, not weight attestation.

Policy fingerprints bind the actual selected role/module/target/profile map.
Legacy empirical studies with a different scope fail closed. Versioned recovery
requires the saved audit pair and exact supported builtin map; unsupported custom
routes cannot be replaced silently. These are local consistency checks, not
cryptographic proof against an operator who edits the database or source.

## Shortest-path stopping evidence

Optional path progress is deterministic bounded assessment of operator-declared
criteria. It does not execute candidate code, grant tools or accept a model's
success claim. Malformed/unknown/oversized candidates cannot accumulate a plateau;
accepted witnesses are checked first. A provider veto cannot turn a compiled
witness into an ordinary failed zero score. Immutable policy markers prevent
legacy continuation from acquiring a new stopping rule. Recomputed histories,
strict audit identities and deduplicated result proofs retain original budgets.
These local consistency rules are not signatures against same-owner database
editing and provide no neural quality or provider-termination guarantee.

## Managed native residency receipts

The operator-only managed API binds a local receipt UUID to a confirmed requested
load and exact full-inventory transition. Intent is durable before POST; confirmed
receipt/task/audit/cohort updates are atomic. Release requires an active receipt,
matching endpoint/type/model/context/instance and inventory digest. Unknown effects
and dropped futures retain blocking intent or quarantine, never success or replay
authority. Existing manual operations quarantine active endpoint receipts before
POST and remain explicit operator administration.

Schema 7 does not adopt legacy operations or inventory. Retention protects receipt
references and managed operations without a confirmed receipt. The indexed history
scan has an independent SQLite deadline. No model obtains a residency capability.
This is local evidence, not exclusive ownership or native-instance inference
attestation. External applications and provider ID reuse can defeat continuity
assumptions between observations. Explicit schema-8 resolution/retirement is described below; task-driven
lifecycle and physical resource enforcement remain incomplete. See
[managed residency](MANAGED_RESIDENCY.md).

### Explicit unknown-operation resolution and terminal evidence archives

Operator testimony plus a fresh inventory GET may resolve the local pending gate;
it does not prove provider termination or original success. Exact original intents,
endpoint identities and immutable resolution/member audits are checked. Stale cohort
release authority requires explicit abandonment; it is never silently rebased.
Resolution can interrupt a dangling local task, but cannot grant model permissions,
refund inference or repeat provider mutations. Terminal archive rows remain evidence
only. Archive and resolution transactions retain task references and fail atomically
on audit errors. Fixed row/history/payload/deadline bounds limit local work; they
provide neither physical resource enforcement nor protection against malicious
same-owner database/source edits. Schema-8 negative/migration/loopback tests now
pass in the combined automated suite; this proves neither provider termination nor
physical GPU isolation.

### Typed module lifetime and continuation identity

Explicit builtin reconstruction accepts only a version-2 source-bound fixed
descriptor, original registry UUID and complete known dispatch history. Exclusive
workspace ownership and a bounded transaction govern the required operator audit.
The reconstructed UUID records provenance rather than original process identity.
No worker executes until independently authorized continuation. Source hashes do
not attest executable/compiler bytes or defeat same-owner database/source edits.
Legacy/custom/mixed registries and native lifecycle owners cannot use this path.

Native task setup forks the trusted catalog and binds process adapters to that
task's owner before provider mutation. Binding cannot change immutable descriptors
or grant capabilities. Idle process adapters retain weak ownership only; an
expired adapter fails before launch rather than acquiring unrelated authority.
Active children retain strong ownership until reaping. Service templates therefore
do not retain previous native-task leases through idle module registrations.
Host binding callbacks remain trusted Rust code, not sandboxed plugins.

Broker teardown signals its initialized workers; CLI exits await bounded process
reapers even on cancellation/error paths. The integration test requires a cancelled
task, durable reaped-child evidence and an absent child PID. Abrupt process death
still cannot guarantee audit completion or provider stop.

Default continuation refuses pinned custom module registries and orphan dispatch
history. Explicit host restoration requires the retained original Broker and Store,
exact immutable versions, known returned outcomes and prior task-scoped tool
reservations. It revalidates before transactional admission and preserves original
budgets. Process history requires complete dispatch/result links and reaped ownership;
recorded PIDs are validation data and never become new process handles.

Database descriptors cannot install callbacks. Retained runtime identity is local
provenance, not binary attestation. Arbitrary callback reconstruction across process
restarts, uncertain-call replay and native lifecycle-owner restoration remain
unsupported; approval cannot convert unknown effects into success.

## Exact input-token reservations

Optional llama.cpp preparation uses the same validated loopback host with redirects
and proxies disabled. Native routes receive only the separately authorized dedicated
credential. Bounded complete-history rendering/tokenization precedes an immediate
SQLite transaction; required count/hash audit commit precedes generation. Unknown
outcomes retain charges. Corruption and concurrent exhaustion cannot grant a refund
or dispatch. Capability gates reject unsupported adapters and requests. Logical
wire-token limits do not prove tokenizer correctness, provider termination, immutable
weights or physical GPU isolation. Same-owner database edits are outside audit
integrity guarantees. See [input-token admission](INPUT_TOKEN_ADMISSION.md).

Exact-token preparation now checks task liveness before sending prompt history,
then independently rechecks it under the reservation transaction. A stopped task
cannot obtain a new completion reservation after that recheck. This cannot revoke
requests already sent to an external provider or eliminate the interval between
local admission and network dispatch. Missing or terminal tasks produce no native
preparation requests in regression fixtures.

## WAL growth enforcement

Runtime on-disk connections explicitly select a wrapper around SQLite's platform
VFS. WAL write/truncate lengths are checked before forwarding; size/chunk hints
cannot preallocate beyond the check, and WAL fetch does not expose mmap writes.
The wrapper preserves platform locking/sync behavior and advertises its marker
only on wrapped file connections. Existing oversized WAL bytes are not silently
deleted; further writes fail until checkpoint/truncate recovery. A read-only
connection, backup or long-running reader may therefore cause other writes to
fail at the cap. All durable-write errors must propagate as failures.

This introduces unsafe FFI forwarding code with process-lifetime VFS registration;
focused callback and recovery tests cover supported SQLite behavior, not arbitrary
third-party VFS implementations. SQLite's default VFS is unchanged. A same-owner
external process can bypass this named adapter. Total workspace/physical storage,
SHM and artifact limits still need a host quota; no claim of hostile-owner disk
isolation follows. See [storage limits](STORAGE.md).

## Operator-only remote text acquisition

Only `candidate.acquire` on the human CLI can fetch approved source plans. Entire
strict manifest/URL preflight precedes DNS, all bounded answers must be public,
and HTTP is pinned to those addresses with TLS hostname verification. Redirects,
proxies, credentials, query/fragment inputs and compressed/ambiguous responses are
refused. Hash pins and byte/deadline limits precede a private unpublished bundle.
Four resolver permits remain owned until libc resolution ends; timeout cannot
dispatch HTTP or block Tokio shutdown through its blocking pool. Host filesystem
stalls and OS resolver termination remain outside the logical timer; separate
processes require host quotas. External content retains data status; no source
instruction can confer capabilities, review assertions, evaluation or promotion.
License/reviewer labels are operator claims, not independent attestation. See
[source acquisition](SOURCE_ACQUISITION.md).
