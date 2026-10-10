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
