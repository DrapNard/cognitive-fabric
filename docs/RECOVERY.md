# Interrupted task recovery

Run `cogfabric recover-tasks` against the original state directory after an
interrupted runtime. It marks abandoned `running` tasks as `interrupted`, records
a per-task recovery audit, and leaves completed results unchanged. Repeating it
returns an empty list. It never retries a tool, resends inference, claims unknown
usage totals, or treats unfinished subtask results as verified. Start a new task
explicitly when another attempt is appropriate.

Every persistent Engine execution holds a shared operating-system flock from
before task creation until its execution future finishes or is dropped. Recovery
requires an exclusive nonblocking flock, then one immediate SQLite transaction.
Other local processes cannot start Engine execution during recovery. Recovery
fails while any engine lease is held, even within the same process. Kernel locks
release on process death; neither a PID file nor task age establishes abandonment.
A crash during recovery rolls back task and audit writes together. Batches above
10,000 unfinished tasks fail closed for offline inspection.

The persistent lease file is `fabric.sqlite3.runtime.lock` beside the database.
It must be an owner-only regular file, with one hard link and no final symlink.
Do not delete, replace or rename it while any process uses the state directory:
locks belong to an inode, and replacing it would split ownership. Use one canonical
local database path; hard-link aliases, copied live state and network filesystems
are unsupported. The owner can modify local files, so this is cooperative runtime
coordination, not isolation from a malicious owner or modified fork. Custom callers
that write `running` rows directly must obtain equivalent coordination; raw Store
mutation is not a task execution API.

Recovery is explicit, operator-only and independent of model reasoning. An idle
service holds no task execution lease, so recovery may inspect its database when
no task executes. The command does not remove stale sockets, resume subtask graphs,
repair corrupted databases or prove that an external inference server stopped
computing. Those cases need separate handling. Recovered task snapshots can enter
episodic memory only through explicit operator publication; they cannot become
verified arithmetic memories.
