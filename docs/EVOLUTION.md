# Evolution lifecycle

Level A uses persistent task traces and operator-ingested documentation.
Level B supports RAG candidates with source, license, content hash and version.
Level C remains an optional offline Python training experiment; neural promotion
is disabled. Level D architecture reconstruction remains future research.

## RAG lifecycle

Register a skill using `skill-add`, then create a candidate with
`candidate create --skill ID --kind rag --artifact FILE --source SOURCE --license LICENSE`.
The reviewed input must be a bounded UTF-8 Markdown/text file. The external
artifact is hash-pinned but not copied into quarantine; mutation causes evaluation
or promotion to fail. Immutable external artifact quarantine remains outstanding.

Run `candidate evaluate ID --suite integrity` and
`candidate evaluate ID --suite quality --dataset FILE`. The latter evaluates
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
