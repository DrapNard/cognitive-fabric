# Runtime security

The deterministic Rust kernel authorizes actions independently of generated
content. Each task receives an immutable scope bound to its UUID and actor.
Scope grants can narrow global policy, never expand it. Dispatch decisions and
grant snapshots are recorded in task traces. Interpreted capability names,
planner subgoals, retrieved documents and expert metadata confer no permissions.

Default runtime actions are local inference, reference retrieval and bounded
arithmetic. Shell execution, unrestricted filesystem/network access, secret
access, untrusted code and automatic production/weight modification are absent.
Operator CLI operations use explicit source/license review and human promotion
or rollback flags. A command flag records operator intent, not a neural safety
assessment.

The Unix service uses a 0700 state directory, 0600 socket and OS peer UID checks.
Frames are verified FlatBuffers, capped at 256 KiB. Incomplete reads and response
writes time out. Eight bounded handlers share one inference permit; excess asks
receive busy errors. Cancellation drops inference futures, records terminal task
state and joins handlers during shutdown. Socket cleanup checks device/inode.

HTTP inference accepts literal loopback addresses; localhost is pinned without
DNS. Proxies, redirects, credentials and query/fragment endpoint data are denied.
Response size/time and model result bounds apply. Provider text is untrusted.
Reviewed file reads reject final symlinks and non-regular or oversized files.
These safeguards do not isolate an already-compromised same-user process.

Artifacts remain inactive in content-addressed quarantine. Evaluations bind to
content/dataset hashes and current baseline versions. Immutable snapshots support
RAG rollback. Retrieval gates do not authorize neural weights. Arbitrary executable
experts require a separately enforced sandbox and review before integration.

See `THREAT_MODEL.md`, `EVOLUTION.md` and the negative tests in `security.rs`,
`wire.rs`, `backend.rs`, `registry.rs`, `evaluation.rs` and `service.rs`. The code
and these tests do not prove alignment or safety after an open-source fork removes
policies. No cryptographic module attestation or multi-tenant protection is claimed.

Inference modules use runtime-owned registration handles and task-pinned adapter
versions. FlatBuffers headers are checked against those handles; string actor
fields alone confer no authority. Queues, expiry and cancellation are enforced
outside the backend. Trusted in-process Rust adapters are not a sandbox for
third-party executable code. See `PROTOCOL.md` for remaining process boundaries.

The optional host-connected Unix inference adapter checks peer UID/PID against
explicit host-approved values and validates task/module-correlated FlatBuffers.
It closes a connection on cancellation or invalid transport responses to prevent
stale reply reuse. It grants no shell, file or worker-launch permission. Process
credentials are not executable/weight attestation or isolation from same-user
descriptor transfer. Worker sessions use trusted immutable task grants and
cumulative ceilings, and drop inference on disconnect/shutdown. Opt-in managed
workers launch only the current executable and receive authenticated single-task
grants. General executable lifecycle/isolation and stale-resource cleanup remain unimplemented;
see `PROTOCOL.md` for the tested scope.

Worker sessions require a trusted audit anchor. Atomic SQLite inserts record
metadata-only decisions/outcomes there, including denied task claims. Authorization
audit failures prevent inference; missing anchors cannot create orphan events.
Abrupt process/future termination can leave an open session record and is not
represented as successful cancellation or completion.

Only human-cli can configure managed worker mode. It defaults off, clears the
child environment and creates private socket resources; task ownership closes
and reaps the child, with kill-on-drop fallback. This adds process coordination,
not shell execution or a sandbox. Same-user worker code retains database access.
Custom registries are rejected in this mode instead of silently replaced.

Persistent workers and host startup hold shared runtime leases; exclusive
operator-only recover-workers reconciles abandoned audit sessions transactionally.
It preserves task results, records unknown external-inference state, never kills
stored PIDs and never replays inference. Audit/metadata failures roll back changes.

Run `cogfabric --json security-eval` for the offline
`deterministic_boundaries/v1` suite. Its 24 fixed probes include positive controls
and expected denials for global permissions, task scope, malformed FlatBuffers
and checker manifests requesting unsupported execution or permissions. Reports
identify runtime version and a SHA-256 of fixture inputs and expectations,
include each observed result, and exit unsuccessfully on any mismatch.
The unexpected-accept rate uses only expected-denial fixtures as its denominator.
It is not a population estimate or a neural safety evaluation. No live prompt
injection, poisoning, sandbox escape, credential exfiltration or general module
peer authentication is tested; the report lists those gaps. This command grants
no candidate promotion authority and invokes no inference or external tools.

### Live reference-content contamination and mitigation

A real Qwen3 8B trial obeyed an answer-marker instruction inside a retrieved
synthetic document despite untrusted-data labels. Runtime permissions stayed
unchanged, but answer integrity failed. Fixed system advice alone also failed.
Quoting nonempty reference context as JSON and restating the original objective
resisted the same attack in one subsequent trial. Neither representation nor
prompt wording is an enforceable security boundary. Keep prose unverified;
retain per-dispatch authorization and independent structured checks. See
LIVE_INFERENCE.md for failed and successful trials with exact scope.

### Bounded inline SQL backpressure

Inline SQL admits at most six requests per process: two execution permits and at
most four additional admitted waiters. Further admission fails immediately. Worker
queue waits have a one-second deadline, are cancellation-safe, and reject inputs
above 32,000 bytes before queue admission. Both admission and execution permits
remain owned by blocking work until it actually ends, even if its caller cancels.
The existing SQLite operation/row/byte limits and 100 ms evaluation deadline stay
in force; queue waiting adds bounded latency rather than expanding SQL authority.
Managed processes each have their own capacity; this is not a host-wide quota.
