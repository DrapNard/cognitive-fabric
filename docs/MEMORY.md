# Reusable task memory

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
