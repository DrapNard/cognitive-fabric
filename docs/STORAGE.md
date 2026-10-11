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

WAL autocheckpoint is 1,000 pages and retained-journal target is 16 MB. These are
checkpoint/retention settings, not a hard physical WAL bound: long-lived readers
can prevent checkpoint progress. `wal_hard_limit` is explicitly false. A strict
physical disk budget requires an operator-configured filesystem quota, including
SQLite WAL/SHM files, artifacts and backups. This runtime does not administer the
host filesystem. Configured page limits also apply to migration writes.

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

Running tasks, every published-memory source (including revoked memory), and tasks
referenced by stored explicit conversation events are protected. No skill,
document, dataset, candidate, model registration or memory is deleted. External
benchmark reports and exported references are not database foreign keys: preserve
needed task traces before approving cleanup. Deletion is intentional and permanent;
there is no automatic cleanup or implicit archive. Retention audits cap at 10,000
entries and remain subject to the database ceiling.

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
automates the database-only procedure for current schema-6 standalone backups.
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
