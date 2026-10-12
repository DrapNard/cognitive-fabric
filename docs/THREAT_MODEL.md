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
