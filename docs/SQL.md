# Bounded inline SQL tool

Pass an explicit structured objective to `ask`:

```sh
cogfabric ask 'sql:{"tables":[{"name":"items","columns":["amount"],"rows":[[2],[3]]}],"query":"SELECT sum(amount) AS total FROM items"}' --mode fast
```

The result is `{"columns":["total"],"rows":[[5]]}`. The Rust tool creates a fresh
in-memory SQLite connection containing only supplied tables and discards it after
execution. It never opens the application database, host files or network. Table
identifiers are validated before generated SQL; values use bound parameters.
JSON cells accept null, booleans (SQLite integers), signed integers, finite reals
and strings of at most 2,048 bytes. Unknown fields and duplicate identities fail.

The engine requires task-scoped `jev-plus/sql.query` permission and charges one
tool invocation. Models/interface actors cannot invoke this capability or replace
the original objective. All profiles except the alone-model baseline can execute
explicit SQL tasks. No generative model is needed. Registration of a reference
skill does not authorize SQL execution; this is a built-in bounded tool rather
than an external dynamically loaded plugin.

SQLite authorizer callbacks allow SELECT, recursive CTEs, reads of the supplied
main tables and a fixed small function allowlist: count, sum, avg, min, max, abs,
round, lower, upper, length, coalesce, ifnull, nullif, typeof, substr and trim.
Everything else is denied, including mutations, PRAGMAs, ATTACH, extensions,
virtual-table introspection and filesystem functions. Query-only mode provides
additional mutation defense. Multiple statements and unbound parameters fail.

Limits: four tables, 16 columns each, 256 rows each, 4 KB SQL and 32 KB standalone
tool input (the CLI/service task limit remains 12 KB including the prefix).
Results contain at most 64 rows and 16 KB serialized data. SQLite also limits SQL
expression depth to 32, compound SELECTs to eight, compiled VM instructions to
10,000, cell/row length to 64 KB and main database pages to 128. Temporary storage
stays in memory. Auxiliary SQLite workers are disabled.

Execution runs on Tokio's blocking pool with at most two queued/running SQL workers
per process; excess admission fails immediately. Workers retain their slots until
they exit, even if the caller disconnects. A progress callback checks cancellation,
an approximate 100,000-operation budget and a 100 ms query deadline every 100 VM
operations; row consumption also checks the deadline. Dropping the calling future
signals cancellation, including when the worker is still queued. This is bounded
in-process SQL execution, not an OS sandbox or hard real-time/heap bound. Query
limits do not measure or cap the entire Rust process. No arbitrary native code,
external database connectors or user-defined functions are enabled.

Success is `completed` with `verified:sqlite_inline/v1` evidence and input/answer
hashes. This means the supplied query executed over the supplied data; it does not
prove the data is true, the query matches an unstated intent, or results have a
stable order without ORDER BY. No language synthesis rewrites the output. Invalid,
denied or exhausted SQL is `rejected`; task/tool-budget denial fails the task.
SQL results are not automatically published as verified semantic memory.

Tests cover aggregates, finite/infinite recursion, row/input/identity limits,
task isolation, host/schema access, mutation, multiple statements, nonfinite data,
permission denials, baseline separation and durable terminal status. This advances
the data.sql.s1 domain and deterministic tool repertoire; general executable
experts, dynamic tool manifests and isolated external connectors remain incomplete.

The enforcement uses SQLite's documented [authorizer](https://www.sqlite.org/c3ref/set_authorizer.html)
and [progress callbacks](https://www.sqlite.org/c3ref/progress_handler.html).
