# Reusable task memory


Exact eligible expert computation batches can reuse checked records within the
same root task, or wait for an in-flight source. Each caller and policy remains
validated; reused call namespaces are durably consumed without adding records.
Expert summary v4 permits the same operation ID and digest in different steps;
older summary versions retain their original validation rules. Reuse does not
prove that model-selected arguments capture the original goal. See
[cognitive network](COGNITIVE_NETWORK.md) for scope and verification limits.

Concurrent core batches hold projected metadata bytes within the same
12,000-byte task memory cap. Completion checks actual output size while
preserving other batches' holds, then releases the matching byte and fingerprint
reservations atomically. Unknown result growth can still refuse publication;
pending work is never represented as completed evidence or automatically replayed.

Expert summary schema v3 can reference checked core results by operation ID and
result SHA-256. Publication and restoration require those references to match
the same root task's validated core memory. Pending computations are separate
durable intents; they are not completed evidence and prevent checkpoint
publication until resolved by the owned execution path. Older v1/v2 summaries
remain readable. Model-selected arguments and advice do not become proof of the
original goal, tool permissions or training data.

Client planner, explorer and reflector handoffs also use structured advisory
records, with 400 bytes per role and a 2,000-byte task envelope. New envelopes
bind the original request and reference snapshot, retain structured context and
store typed checker feedback. Joint size checks precede optional role calls;
records are never truncated into invalid JSON. Legacy prose is explicitly
represented by a digest/omission marker when merged into this format. Claims
remain unverified data and actions do not grant execution authority. This does
not migrate all legacy episodes or other framework memory paths.

New client Expert Network step summaries use bounded structured conclusions,
separate from published episodes below. A minimal model record is
`{"v":1,"unknowns":["Need evidence"]}`. Optional `claims` distinguish supplied
statements from hypotheses; optional `actions` describe suggestions that grant
no execution authority. Canonical source/reference hashes bind the surrounding
task record, while any model-proposed reference remains unverified. New summaries
use schema v2; legacy v1 text is read-only untrusted context. No model output is
automatically published, indexed as fact or used to train a project adapter.
See [cognitive network](COGNITIVE_NETWORK.md) for caps and incomplete role paths.

Task/event history remains private to explicit conversation references by default.
New reusable-memory publications are limited to deterministic arithmetic records;
free-text episode publication is rejected. Existing episode rows remain in the
database for archival inspection and are never included in new reasoning prompts:

```sh
cogfabric memory-publish TASK_UUID --kind verified_arithmetic --note "Reviewed arithmetic result"
cogfabric memory-search "Rust lifetimes"
cogfabric memory-revoke MEMORY_UUID
```

`verified_arithmetic` independently recomputes the original bounded `calc:`,
`math:` or `calculate:` expression and requires the completed answer to match
exactly. It is an operator-searchable record, not prompt recall or generic
model-output fact promotion. Model actors cannot publish or revoke memories.

Schema 4 stores immutable task snapshots, hashes, source-task references, review
notes, an FTS5 index and a revocation flag. Publication/revocation and source-task
audit records commit together. Entries remain after source-task changes; hash
checks and arithmetic recomputation run again when inspected. Tampering fails
closed. Revocation excludes an entry from operator search; it cannot erase copies
already pinned by old tasks. Revoked rows retain audit history and consume quota.
No automatic task-history indexing occurs.

Limits: 10,000 entries, 16 MB of query/answer/note payload, 12 KB query, 16 KB answer,
500-byte operator note, eight search terms and three retrieved entries. Physical
SQLite/FTS overhead and task/event history are outside this payload quota and still
use the separate database page ceiling and reviewed retention policy (`STORAGE.md`). Search terms are quoted after bounded
ASCII tokenization; external input cannot inject FTS expressions or SQL.

No production inference profile retrieves published memory rows. Historical
episode content is only available through the explicit operator `memory-search`
command. Structured project knowledge belongs in immutable project-memory
records, which use bounded facts, relations, constraints and evidence pointers.
This prevents old or hostile prose episodes from influencing future model prompts;
it does not erase historical rows or prove model safety.
