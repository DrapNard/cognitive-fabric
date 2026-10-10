# Evolution lifecycle

Level A uses persistent task traces and operator-ingested documentation.
Level B supports RAG candidates with source, license, content hash and version.
Level C remains an optional offline Python training experiment; neural promotion
is disabled. Level D architecture reconstruction remains future research.

## RAG lifecycle

Register a skill using `skill-add`, then create a candidate with
`candidate create --skill ID --kind rag --artifact FILE --source SOURCE --license LICENSE`.
The reviewed input must be a bounded UTF-8 Markdown/text file. New imports copy the artifact into content-addressed quarantine, as described below.

Run `candidate evaluate ID --suite integrity` and
`candidate evaluate ID --suite quality --dataset FILE --dataset-source SOURCE --dataset-license LICENSE --held-out`. The latter evaluates
retrieval against a candidate-only corpus, with at least three supplied cases
and a 0.9 acceptance threshold. An operator must provide genuinely held-out
cases: the runtime does not establish dataset independence automatically.
These tests establish neither prompt-injection resistance nor neural safety.

Use `candidate promote ID --human-approve` after reviewing the document and
results. Promotion verifies the content hash and latest passing gates inside a
SQLite transaction, stores a durable document snapshot, links the candidate to
that snapshot, switches the skill's active version and records a lifecycle audit.
Candidate snapshots outside the active version are excluded from retrieval.
Directly ingested documents remain available independently of candidate versions.

Use `candidate rollback OLD_CANDIDATE_ID --human-approve` to restore an approved
older version of the same skill. Rollback verifies the stored snapshot hash;
it needs no external artifact and cannot activate an unevaluated candidate,
restore a newer version, or silently succeed on a corrupted/missing snapshot.
The switch and audit record are transactional. Models have no rollback permission.

A task currently copies retrieved documents into its context before model calls.
Full atomic pinning of all model/skill configuration versions for task graphs is
still outstanding. Neither rollback nor promotion hot-swaps neural weights.

## Database compatibility

Schema version 1 migrates the unversioned Rust alpha using an immediate
transaction. Existing tasks, skills, candidates and documents are preserved;
approved candidates are linked to matching stored document hashes and skills.
SQLite user_version tracks compatibility; newer schemas are rejected.
WAL and a five-second busy timeout support bounded lock contention. Lifecycle
audit records include prior and selected versions. Back up state before upgrades.

Schema version 2 adds reference-manifest lifecycle and skill enable state. Tasks
now pin declarative registry versions and copied retrieved content in one SQLite
snapshot. See `EXPERTS.md`; external backend weight identity is still unverified.

## Content-addressed quarantine

New CLI candidate imports use `src/evolution.rs`: operator-reviewed content is
copied into owner-only `artifacts/` and stored as a read-only hash-named file.
Changing the original source no longer changes a candidate. Repeated imports reuse
verified content while creating separate immutable candidate versions. Imports
serialize through an immediate SQLite transaction to enforce a shared quarantine
ceiling (max_artifact_bytes, default 256 MB, at most 2 GB) and 4,096 entries.
Candidate metadata is capped at 10,000 rows and version allocation is transactional.
Individual limits remain 256 KB for RAG and 64 MB for inactive neural candidates.

Existing alpha candidates with external paths are preserved and still hash-checked;
operators can re-import them into quarantine. Read-only permissions are not a
security guarantee against a compromised same-user process: modification is detected
by content hashes at evaluation, reuse and promotion. Neural evaluation/promotion
remain disabled. Schema version 3 stores immutable held-out dataset snapshots with source, license,
and checksum (up to 1,024 datasets / 64 MB). Quality evaluation rejects duplicate
normalized queries and requires operator attestation of held-out independence.
Promotion additionally requires retaining every passing case in the active
baseline candidate’s latest reviewed suite. It rejects a changed baseline or
mismatched evaluation identities. This is a retrieval regression gate, not a
neural safety assessment or cumulative historical regression bank. Legacy approved
artifacts require re-review with dataset provenance before serving as baselines.

## Historical retrieval regressions

New RAG candidates retain passing assertions from every previously approved
version, including versions inactive after rollback. Evaluation is bounded to
128 approved versions and 8,192 historical cases; exceeding either limit fails
closed. Approved reviewed quality suites cannot be replaced. Legacy approved
artifacts without reviewed provenance can receive a one-time reviewed suite.
Promotion compares the evaluated approved-version list and active version within
its transaction, rejecting intervening promotions even after rollback. Quality
and regression records are currently separate writes; atomic paired review
records remain a hardening task. These retrieval checks do not establish neural safety.
