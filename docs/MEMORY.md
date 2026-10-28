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
An operator can publish selected terminal task snapshots for later retrieval:

```sh
cogfabric memory-publish TASK_UUID --kind episode --note "Reviewed historical attempt"
cogfabric memory-search "Rust lifetimes"
cogfabric memory-revoke MEMORY_UUID
```

Episodes retain their original task status, including failure or unverified output.
Publication is not a truth judgment. `--kind verified_arithmetic` independently
recomputes the original bounded `calc:`, `math:` or `calculate:` expression and
requires the completed answer to match exactly. There is no generic model-output
fact promotion. Model actors cannot publish or revoke memories.

Schema 4 adds immutable task snapshots, hashes, source-task references, review
notes, an FTS5 index and a revocation flag. Publication/revocation and source-task
audit records commit together. Entries remain after source-task changes; hash
checks and arithmetic recomputation run again when retrieved. Tampering fails
closed. Revocation excludes an entry from future retrieval; it cannot erase copies
already pinned by in-flight tasks. Revoked rows retain audit history and consume
quota. No automatic task-history indexing occurs.

Limits: 10,000 entries, 16 MB of query/answer/note payload, 12 KB query, 16 KB answer,
500-byte operator note, eight search terms and three retrieved entries. Physical
SQLite/FTS overhead and task/event history are outside this payload quota and still
use the separate database page ceiling and reviewed retention policy (`STORAGE.md`). Search terms are quoted after bounded
ASCII tokenization; external input cannot inject FTS expressions or SQL.

Full tasks and the memory comparison profile pin published entries alongside
reference snapshots. Other comparison profiles do not include episodes in model
context. Prompts label entries as data with their source status; verification
applies only to the recorded arithmetic query, never a later goal. Retrieval does
not grant capabilities or change the later task's verification status. LLMs can
still be influenced by hostile data, so deterministic permission enforcement remains
independent of prompt instructions. No live-model poisoning resistance is claimed.

## Fresh checks before structured answer reuse

For an exact `sort:` or `order:` query, Full and Memory execution profiles may
inspect their retrieved operator-published episodes for a reusable answer. Each
attempt consumes the corresponding deterministic verifier capability and records
fresh objective/answer hashes, memory identity and check outcome. The prior
memory status and review note are never proof. Correct answers finish with
`verified_memory` and no inference; incorrect answers leave ordinary bounded
reasoning available. Revoked entries and nonmatching queries cannot be reused.

Explicit conversation references disable this shortcut, as do other ablation
profiles. Retrieval remains bounded to three memories; finding no exact match does
not trigger an unrestricted history search. This establishes a concrete reusable
solution path for bounded structured constraints, not general fact validation,
weight learning, or a measured advantage on live-model benchmarks.
