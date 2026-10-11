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
