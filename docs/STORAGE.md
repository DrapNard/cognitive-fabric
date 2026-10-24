# Database limits and reviewed retention

`max_database_bytes` configures the logical SQLite page ceiling (default 256 MB,
range 64 MB–2 GB). CLI connections apply the same saved limit using
`Store::open_with_limit`; embedded callers must pass their intended ceiling.
Existing databases larger than the requested limit are preserved and rejected,
not truncated. All processes should restart after changing a shared limit.

`cogfabric storage-status` reports allocated/live page bytes, effective maximum
bytes and current WAL size. SQLite rejects page growth beyond the ceiling. Task
creation is transactional and requires 16 MB of reusable/new page headroom and
fewer than eight running tasks across connections. Abandoned tasks consume slots
until explicit recovery. Headroom reduces exhaustion risk; concurrent tasks and
other writes can still exhaust the database. Failed durable writes are errors,
not successful task completion. Recover interrupted work after restoring space.

## Runtime WAL write boundary

Runtime file connections use a named SQLite VFS adapter that refuses a WAL write
or truncation extending beyond 16,000,000 bytes with `SQLITE_FULL`. It is selected
explicitly for Store creation, existing/read-only connections, migrations, token
admission, backup destinations and restore staging; SQLite's process-wide default
VFS is unchanged. `storage-status` reports `wal_hard_limit` and `wal_limit_bytes`
from a marker on the actual connection. In-memory/unwrapped connections do not
report this guarantee.

For explicit maintenance, run `cogfabric --json storage-checkpoint`. This
human-only operation requests a truncating checkpoint with a 250 ms lock wait,
then restores the connection's prior timeout. It reports busy readers instead of
evicting them or claiming success. `storage-status` and `storage-checkpoint` open
an existing current-schema database without bootstrap/migration writes, so they
can inspect or recover a WAL that blocks normal runtime initialization. Checkpoint
I/O is not a hard real-time operation. No task is replayed or deleted.

The 1,000-page autocheckpoint and 16 MB retained-journal target remain maintenance
settings. A pinned reader can now make writes fail at the WAL boundary instead of
permitting continued growth. Failed transactions remain failures; no task success,
refund or data deletion is inferred. Releasing the reader permits ordinary SQLite
checkpoint/reuse. Transactions that alone need more WAL space also fail, so large
imports or migrations may require smaller operator-reviewed batches.

An already oversized WAL is preserved and remains readable. Its writes are
refused until a successful checkpoint/truncation reduces it below the limit;
opening the database never deletes committed history. The connection marker
reports enforcement on future writes, not that pre-existing bytes are conformant.
Inspect `wal_bytes` alongside `wal_limit_bytes`.

The adapter bounds WAL growth for cooperating runtime connections. It does
not cap the directory's total allocated disk blocks, SHM, backups, artifacts or
other applications' writes. A process opening the same database with another VFS
can bypass it. A strict workspace-wide physical budget still requires an operator
filesystem quota. This runtime does not administer the host filesystem.
Configured main-page limits still apply independently to migration and task writes.

## Task-history cleanup

Preview a bounded set of old terminal tasks, then inspect/save that exact JSON:

```sh
cogfabric --json retention-preview --before-unix EPOCH_SECONDS --limit 100 > plan.json
cogfabric retention-apply plan.json --human-approve
```

The plan lists exact task IDs and event count and has a canonical checksum. The
cutoff cannot be in the future; batches contain at most 1,000 tasks. Apply requires
an explicit operator flag, rechecks eligibility and the checksum in an immediate
transaction, then deletes only those task rows and their events. A changed plan,
new event, new memory/reference or altered eligible task set makes it stale.
A retained summary audit records deleted identities/counts and plan hash, without
query/answer bodies. Failed deletion rolls back the audit and all deletions.

Running tasks, every published-memory source (including revoked memory), tasks
referenced by stored explicit conversation events, and managed residency operation
tasks are protected. Durable managed residency receipts also protect their load and
release task rows through foreign keys. Archived receipts protect their original
load/release tasks and retirement task; append-only resolution records protect the
interrupted operation they resolve. This includes unknown-outcome managed
operations that have no receipt or resolution yet. No skill,
document, dataset, candidate, model registration or memory is deleted. External
benchmark reports and exported references are not database foreign keys: preserve
needed task traces before approving cleanup. Deletion is intentional and permanent;
there is no automatic cleanup or implicit archive. Retention audits cap at 10,000
entries and remain subject to the database ceiling. Managed residency receipts have
a separate 1,024-row live-ledger quota. Schema 8 adds append-only operator-resolution
evidence capped at 1,024 rows and an immutable receipt archive capped at 8,192 rows.
Reviewed task retention never deletes these rows or their provenance, so operators
must preserve capacity for future managed operations and resolutions.

Freed pages become reusable; cleanup does not promise immediate file shrinkage.
No automatic VACUUM or WAL truncation occurs. Existing progress cursors for deleted
tasks return unknown-task errors. This is explicit bounded retention, not full
backup/restore, secure erasure or automatic checkpoint resumption.

## Consistent database backups

`cogfabric --home STATE --json backup` uses SQLite's backup API to copy a pinned
read transaction, including committed WAL data, to `STATE/backups/UUID.db`.
It performs a SQLite quick consistency check, streams SHA-256 over the file,
fsyncs and publishes it read-only (0400) in an owner-only (0700) directory. Reports
include path, checksum, bytes and schema version. Backups require the operator-only
storage.backup capability; model actors cannot request them. No task is replayed,
cancelled or marked complete as a result of backing up.

A stable owner-private `backups.lock` uses nonblocking flock to serialize cooperating
backup processes. Source reads do not take SQLite's write reservation, so WAL
writers on other connections can continue. The source Store connection/mutex is
held during copying: embedded callers using that same connection wait. Backups
pin a read view and can temporarily delay WAL checkpoints. Copy/hash loops check
a ten-second deadline between bounded batches; filesystem/SQLite syscalls and the
consistency check are not hard real-time bounded. This is not a disk quota service.

Limits are eight backup directory entries, individual files no larger than the
configured database ceiling, and total retained backup bytes no larger than twice
that ceiling or 2 GB, whichever is smaller. No existing backup is overwritten or
automatically deleted. Symlink directories, nonregular entries and invalid lease
files fail closed. Temp files are cleaned after ordinary failures; filesystem
failures during publication may leave a valid read-only file despite an error.
Same-user hostile tampering is outside an absolute isolation guarantee.

The snapshot includes SQLite tables only. Save the checksum report separately and
back up reviewed artifacts/configuration through an operator-controlled process.
Candidate artifact paths are not rewritten, weights are not copied, and a database
snapshot alone is not a portable full workspace archive. Running tasks remain
running in the snapshot; explicit recovery can mark abandoned tasks interrupted
without replaying them after restoration.

To inspect restoration safely, verify the file against the saved checksum, copy it
without overwriting into a separate new private directory as `fabric.sqlite3`, make
the copy owner-writable (0600), and open that directory with `--home`. Keep the
original backup read-only. This procedure does not replace a live database and does
not restore missing configuration/artifacts automatically. Validate task traces,
registry state and artifact availability before using a restored workspace.

## Restore into a new directory

`cogfabric restore-database SNAPSHOT --checksum SAVED_SHA256 --destination NEW_HOME`
automates the database-only procedure for current schema-8 standalone backups.
The destination must not exist, even as an empty directory or symlink; its parent
must already exist. No active workspace is replaced. This command runs before
ordinary workspace initialization, so it does not create the configured source
home, open its database or start an engine. It may read that home's configuration
for limits. Only human-cli/storage.restore is authorized.

The command copies through bounded 64 KB buffers, checks the saved SHA-256 and
file identities, and opens the staged database read-only for SQLite quick_check,
schema and journal validation. Live WAL databases and other schema versions are
rejected. Input bytes cannot exceed the configured database ceiling. Copy and
validation check a ten-second deadline; blocking I/O/sync and SQLite callbacks
are not hard real-time guarantees. Publication uses a no-overwrite hard link,
with private 0700 directory/0600 database permissions and durability syncs.

`restore-manifest.json` records a versioned restore UUID, source checksum, byte
count, schema and count of running tasks not replayed. It is provenance metadata,
not an ongoing checksum of a database later modified by normal execution. No
configuration, external artifacts, model weights or candidate paths are restored
or rewritten. Absent configuration uses normal defaults on the next CLI run.
Inspect traces/artifact availability and run explicit recover-tasks for abandoned
work; restoration never infers completion or resumes execution.

Failures can leave the newly reserved private directory or partial metadata;
publication/sync errors can leave a valid database despite an error. Inspect it
before retrying with another new destination. No existing directory is cleaned up
or overwritten. Same-user hostile tampering remains outside an absolute isolation
guarantee. Full artifact/configuration archives and older-schema migration restore
remain separate work.

## Checked configuration recovery

New backups embed version-1 `backup_configuration` metadata in the standalone
SQLite snapshot. It contains the validated effective runtime configuration (including
CLI/environment configuration overrides), not the raw source config file. The
snapshot checksum covers these settings and the task database together. Workspace
paths are excluded by configuration serialization; environment credentials are
never configuration fields. Backup reports now set `includes_configuration: true`.
External artifact/weight files remain excluded. Metadata insertion and final byte
admission apply to the backup only, without changing the source database.

Pass `--restore-configuration` to `restore-database` to restore these settings.
The default remains database-only. Explicit configuration restoration requires the
embedded metadata and supported version, bounds its JSON to 64 KB, validates all
settings before publishing the database, then creates private 0600 `config.json`
without overwriting. The regular loader binds settings to the new destination;
archived source paths cannot redirect workspace access. Old snapshots lacking
metadata still restore database-only, but explicit configuration requests fail.

Restoring settings is an operator action: review endpoints, model registrations,
resource ceilings and optional managed-worker/authentication flags. Auth scope
metadata may be restored; the dedicated credential must still be provided locally.
As before, publication/sync failures can leave valid partial output in the newly
reserved directory. This is not atomic full workspace restoration: artifacts,
model weights, missing external candidate paths and older-schema migration remain
separate work. The earlier database-only instructions describe default behavior;
new snapshots additionally include the configuration metadata described here.

## Optional RAG artifact archives

`backup --include-rag-artifacts` embeds the exact RAG candidate payloads referenced
by the pinned database view in `backup_rag_artifacts`. Each payload must be valid
UTF-8, nonempty, no larger than 256 KB, and match its registered SHA-256. Identical
hashes are deduplicated. Capture permits at most 4,096 candidate rows, 4,096 unique
payloads and 16 MB total (or the smaller configured artifact ceiling). Database and
retained-backup byte limits still apply after insertion. Files are read through
bounded no-follow readers; changed/missing files fail the backup. Neural adapters,
base weights and unrelated directory contents are never copied. Default backups
remove inherited archived payloads so opt-in remains explicit on each backup.

`restore-database ... --restore-rag-artifacts` validates payload bounds/hashes and
exact RAG candidate coverage before creating new owner-private quarantine. It
creates read-only 0400 content-addressed `.md` files and transactionally rebases
only RAG candidate paths to the canonical new workspace. Candidate status, model
permissions, evaluations and promotion decisions are preserved, not upgraded.
Candidate-table triggers are rejected before path updates. Optional configuration
restoration may be combined; artifact admission uses the smaller current/restored
ceiling. Restored payload writes remain bounded with checked deadlines; blocking
filesystem operations are not hard real-time bounded.

Reports expose `rag_artifact_count` on backups and `rag_artifacts_restored` on
restoration; null means the option was not requested. `includes_external_artifacts`
on the backup denotes included RAG payloads only. `external_artifacts_restored`
remains false because this does not restore arbitrary external/neural artifacts.
The restore report's checksum and byte count describe the source archive. Rebasing
candidate paths changes restored database bytes; its live checksum will differ.
The original archive remains unchanged and read-only.

Old snapshots without embedded payloads fail an explicit artifact restore request,
but retain normal database-only restoration. Validation failures publish no final
database; later I/O/sync failures may leave a partial newly reserved directory or
quarantine for inspection. Existing workspaces are never overwritten. Neural files,
older-schema migrations and general portable external artifact bundles remain
separate work. The earlier database-only sections describe default options.

## Explicit older-schema migration

`restore-database ... --migrate-schema` permits recognized schema versions 1–7.
The default still requires current schema 8. Future schemas and unversioned/schema-0
files remain rejected even with opt-in. The source checksum is verified before
migration; only the newly staged private copy is upgraded through the existing
transactional Store migrations. The copy is checked again, checkpointed and
returned to standalone DELETE-journal mode before publication. No tasks are replayed
and no existing workspace is replaced. Migration failure publishes no final database.

Restore records distinguish `source_schema_version` from resulting `schema_version`.
The source checksum/bytes remain archive provenance; migrated destination bytes
will differ. The configured database page ceiling and checked ten-second overall
restore deadline apply; SQLite progress callbacks interrupt long migration SQL.
Filesystem sync and short SQLite operations are not hard real-time bounded.
Configuration/RAG restoration can be combined only when their required metadata
is actually present. Old snapshots do not gain missing configuration or artifacts.

Regression fixtures cover versions 1–7, preserved source bytes/running state,
standalone output and transactional failure; the CLI fixture verifies the flag.
Earlier notes describing older migrations as separate work are superseded by this
explicit recognized-version option. Unknown/custom schemas still need review.

## WAL boundary verification — 2026-10-24

The implementation received an independent source review of the unsafe file
layout, callback forwarding, version gates, registration lifetime and cap bypasses.
No blocking defect was identified; this is not a proof against arbitrary VFS code.

Commands and actual outcomes:

- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo +1.99.0 check --workspace --all-targets --locked`: passed.
- `cargo +1.99.0 check --workspace --all-targets --all-features --locked`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- Focused library filters `storage::tests` (6), `storage::wal_vfs::tests` (3), `backups::tests` (1), `restore::tests` (4), `store::tests` (15 including the same four restore tests), `token_admission::tests` (7), and `security::tests::storage_checkpoint_is_human_only` (1): passed.
- `cargo +1.99.0 test -p cogfabric --test input_token_admission --locked`: seven passed, including managed worker processes.
- `cargo +1.99.0 test -p cogfabric --test restore_configuration --locked`: one passed.
- `cargo +1.99.0 test -p cogfabric --test wal_checkpoint_cli --locked`: one passed. The CLI reports an oversized external WAL, returns busy without evicting its pinned reader, then checkpoints after release and preserves all 36 committed fixture rows.

No full reflection test suite or reasoning benchmark was run. Filesystem-wide
quotas, physical GPU resource enforcement and the remaining scientific/release
requirements are not established by these storage checks.
