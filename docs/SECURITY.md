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
descriptor transfer. Worker-side policy and production process lifecycle remain
unimplemented; see `PROTOCOL.md` for the adapter's tested scope.
