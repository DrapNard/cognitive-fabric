# Threat Model (v1.0)

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
