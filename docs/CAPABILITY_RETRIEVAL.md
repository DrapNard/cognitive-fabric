# Pinned capability-directed reference retrieval

Full and Experts tasks with retrieval and non-FAST interpretation can retrieve
references for the providers selected by joint capability resolution. A capability
binding remains declaration metadata, not a permission or proof of usefulness.

## Consistent source

The initial task-context read transaction also creates a detached, read-only
SQLite snapshot, capped at 64,000,000 bytes. Registry metadata, document approval
state and the snapshot share that read view. The transaction and live Store mutex
are released before any interpretation/backend callback. Later disable, activation
or insertion cannot substitute different references into the task. Evaluation
arms reuse their already-frozen corpus instead of making another copy.

The snapshot stays in host memory for the task; it is not sent to a model or
persisted as a restorable executable artifact. Copying and hashing a whole database
has CPU and memory cost, including a temporary serialized image. This bounds data
size rather than providing hard physical memory or real-time guarantees. Oversized
stores skip this optional snapshot and retain ordinary initial retrieval and the
main reasoning path. Integrity/backup errors fail rather than masquerade as a
successful snapshot. No long-lived live WAL read transaction spans inference.

## Charged bounded search

After interpretation and clarification handling, selected available providers form
one bounded `knowledge.search` invocation. There are at most four providers and
one result per provider. Capability IDs precede the original query in the scoped
FTS search; normal token limits, enabled-skill checks, approved active RAG-version
filters and body integrity checks apply inside the frozen view. Duplicate document
IDs already in context are excluded. Empty/duplicate results still retain the
charge for the search performed; no relevant result is guaranteed.

Admission preserves one remaining main-solver call and its 384-token output
reservation. Explicit structured goals also retain one verifier tool invocation.
The candidate extended context must fit the existing 32,000-byte prompt bound and
remaining cumulative prompt budget. A context refusal retains the search charge
but adds no document evidence or solver input. General task budgets, call/phase
admission, wall deadlines and cancellation remain in force.

Accepted snippets contain at most 256 title and 1,200 body Unicode characters
per document and are labelled untrusted reference data. The exact original goal
remains authoritative. The resolver grants no tool, loads no model, changes no
active candidate and performs no network search.

## Audit, policy scope and recovery

`capability_reference_snapshot` records availability and bounded snapshot metadata.
A per-search intent follows task authorization/reservation and precedes frozen
queries. `capability_retrieval` records admission/refusal and document identities,
provider IDs and content hashes before any snippets enter context. Failed audit
writes stop before further inference; committed search charges are retained.
Original initial `knowledge` records remain intact, with additional references
recorded separately.

Additional admitted references extend the optional workflow policy's evidence
fingerprint using canonical document IDs, skill IDs, body hashes, title, source
and license. This prevents a prior cohort's reviewed study from silently covering
new knowledge. The database snapshot checksum is an audit identity, not a stable
utility cohort: task history changes it. No policy is automatically retrained or
approved. Existing post-routing ready checkpoints retain their recorded context;
pre-interpretation snapshots are not reconstructed after a crash.

Tests cover both runtime profiles, original-query misses resolved through aliases,
live disable/insertion during interpretation, verifier-reserve refusal, oversized
snapshot refusal, corrupt documents, failed intent/outcome audits and canonical
workflow-evidence changes. They establish behavior, not held-out relevance,
reasoning quality, VRAM savings or latency improvements. Broader retrieval/reranking,
large-corpus snapshot efficiency and independent effectiveness remain incomplete.
