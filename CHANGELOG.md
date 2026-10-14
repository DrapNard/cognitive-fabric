# Changelog

All notable changes are documented here in English.

### Account for processing requests when selecting inference routes

- Prefer idle equal-rank peers even when a busy module has no queued requests.
- Preserve operator ranks, bounded queues and existing task pins; clear processing observations on cancellation and shutdown.
- Carry optional bounded load observations through FlatBuffers while preserving legacy unknown values.

### Admit workflows before preparing expert backends

- Separate bounded prompt preparation from execution while preserving existing budgets and prompts.
- Avoid model snapshots, managed backend setup and phase allocation for refused candidates.
- Keep comparison metadata aligned with admitted configurations and actual dispatch records.

### Select an affordable routed workflow after budget refusal

- Distinguish pre-admission refusal from an admitted expert attempt.
- Consider the next routed workflow when call or prompt reservations cannot fit.
- Preserve the single admitted opportunity and fail closed on audit/provider errors.

### Verify an operator-controlled native model lifecycle

- Add inspection-first load/unload validation with explicit approval and unrelated-instance checks.
- Exercise existing-instance refusal, approved lifecycle and drift abort through real CLI fixtures.
- Preserve one user-approved real Nomic embedding cycle with Core audits; keep ownership and generative-switching claims unproven.

### Reconcile recorded advisory results before continuation

- Recover the crash window between durable inference and its subtask-state write.
- Validate original checkpoints and atomically link unverified advice without replay or budget refunds.
- Exercise operator approval, leases, corruption/expiry denials, rollback and real managed CLI continuation.

### Persist experimental module authority in Core task storage

- Create operator-only task scopes with original deadlines and recovery leases.
- Enforce ordered, bounded SQLite audit reservations across duplicate wrappers.
- Add an optional Wasm-to-Core sink without adding an interpreter to Core builds.

### Require audit acknowledgements for experimental Wasm execution

- Record bounded allocation, dispatch reservation and terminal metadata before exposing results.
- Keep uncertain/failed allocations spent and block unaudited guest execution.
- Recheck cancellation after callbacks while excluding scalar and error contents.

### Share experimental Wasm allocations across approved artifacts

- Pin a task's artifact set, expiry and total nonrefundable invocation allowance.
- Deny version/content substitutions and revoke issued/future grants together.
- Require the authority API instead of public per-artifact issuance.

### Expire and revoke experimental Wasm task grants

- Bound grant lifetime and deny queued expired or revoked execution.
- Order admission/result commitment atomically and discard cancelled/late results.
- Keep admitted work spent while preserving interpreter fuel bounds and scope limits.

### Bind experimental Wasm artifacts to single-use task grants

- Validate immutable compiled artifact ID/version/hash before invocation.
- Bind task, actor and input; deny replay and charge admitted traps.
- Preserve old version snapshots and fresh guest state without production activation.

### Bound experimental Wasm shapes before translation

- Admit only bounded scalar module declarations, locals, operators and nesting.
- Reject imports, unsupported shapes and wrong interfaces before start execution.
- Retain full semantic validation and test small encoded allocation amplification.

### Test an isolated import-free Wasm expert foundation

- Add a non-publishable experimental Rust workspace with pinned interpreter and no host imports.
- Exercise fuel, memory/table and signature limits without exposing production permissions.
- Record remaining compilation, lifecycle and security integration requirements.

### Retain the complete RESEARCH phase comparison

- Independently verify all 96 public graph outcomes and three phase diagnostic families.
- Record expert successes, planning fallbacks and their additional inference/time cost.
- Preserve historical results and exclude general/causal or held-out quality claims.

### Reserve native-worker fixture state atomically

- Replace timestamp-only directory naming with bounded atomic reservation.
- Keep parallel test databases separate and owner-only without relaxing assertions.

### Report planning and exploration phase diagnostics

- Expose task-scoped advisory phase/fallback counts without strategy or error contents.
- Aggregate expert, planning and exploration counts over executed comparison tasks.
- Preserve unknown classifications and historical reports instead of inferring success.

### Prepare an inactive Rust reference candidate

- Add original source-linked ownership/borrowing/lifetime notes and hashed provenance.
- Verify immutable quarantine, integrity and rejection of unattested draft acceptance.
- Keep active retrieval and promotion gated; distinguish fixtures from independent quality.

### Verify real-provider phase handoff and current installation

- Retain the real Qwen RESEARCH task after a bounded planning timeout.
- Confirm original-budget final verification and unknown interrupted usage.
- Refresh both optimized installed workflows with source/binary hashes and proof limits.

### Bound research exploration before final solving

- Give the optional explorer a shared-task time ceiling and audit it before dispatch.
- Keep charges and unknown usage after typed timeouts; require durable failure auditing.
- Preserve fatal provider/policy errors and successful unverified alternatives.

### Bound fresh planning and decomposition wall time

- Share one advisory phase ceiling before the first whole-goal solver.
- Discard timed-out partial strategy while retaining charges and unknown usage.
- Keep non-timeout failures fatal and preserve original task budgets.

### Await physical lease release in recovery regression

- Keep recovery denied while owned arithmetic/SQL sessions are live.
- Bound the wait after cancellation and reject every non-contention error.

### Verify current installed live socket reasoning

- Run FAST sorting and DEEP graph reasoning through the installed socket service with Qwen.
- Confirm independently checked outputs, charged calls, subtask execution and graceful shutdown.
- Retain exact source/binary identity and separate smoke evidence from quality/residency claims.

### Refresh current optimized installation evidence

- Install both current packages offline without a native FlatBuffers compiler.
- Pass installed CLI/socket and authenticated HTTP/typed-worker workflows.
- Record binary/source hashes and preserve clean-OS/live-inference proof limits.

### Scope final-answer formats during subtask reasoning

- Distinguish intermediate advisory results from the whole-goal answer format.
- Preserve original constraints, dependency boundaries and final verification.
- Build admission bounds from the same prompt used for dispatch.

### Retain the complete live graph comparison

- Record all 96 public Qwen workflow outcomes and independently verify scores and resources.
- Mark the public corpus workflow complete while preserving missing independent evaluation flags.
- Record zero advice fallbacks and no expert accuracy gain over the single-model baseline.

### Verify socket cancellation during optional expert advice

- Exercise disconnect, shutdown and both operator cancellation paths against a stalled provider.
- Retain expert reservations and unknown usage without starting fallback reasoning.

### Measure concise advice in the live collective

- Retain the complete repeated four-case Qwen comparison with matching operator settings.
- Verify every path score independently; record four expert successes and zero advice fallbacks.
- Preserve previous failures and distinguish observations from causal quality claims.

### Request concise optional expert advice

- Ask legacy and structured workflows for one short constraint/pitfall sentence.
- Preserve runtime budgets and unverified advice boundaries.
- Retain direct Qwen latency probes and their sampling/quality limitations.

### Record live Qwen expert timeout diagnostics

- Retain all 24 outcomes from a bounded public four-case, six-profile run.
- Check every path score independently and freeze diagnostic/resource regressions.
- Record four advice timeouts and no expert quality advantage without rewriting older results.

### Expose expert advice fallback diagnostics in benchmarks

- Record typed inference-timeout classifications in optional advice audits.
- Report bounded per-task advice phase and fallback counts without error contents.
- Keep historical and unknown failure kinds unclassified instead of guessing.

### Verify cancellation of optional expert workflows

- Cover pending legacy advice and cancellation after a completed structured step.
- Verify local backend cleanup, retained charges and no timeout fallback reasoning.
- Preserve unknown provider usage and avoid claiming external inference stopped.

### Reject unsupported native agent control shapes

- Validate exact named tool-choice fields for direct/managed semantic parity.
- Reject extra text-part metadata instead of silently discarding it.
- Keep valid named controls and client-owned schema/action permissions unchanged.

### Verify optimized installation without flatc

- Install both current packages using bundled bindings and pass installed CLI/socket/HTTP workflows.
- Record source/binary/bundle hashes and successful hosted dual-mode CI.
- Update server setup to remove the obsolete native-compiler prerequisite.

### Build without a manually installed FlatBuffers compiler

- Bundle pinned generated Rust bindings with schema/output integrity checks.
- Require exact byte-identical regeneration when FLATC is explicitly configured.
- Verify normal and packaged builds without FLATC; keep stale bindings and wrong compilers fatal.

### Record pinned capability route provenance

- Audit requested aliases and their pinned provider/capability versions.
- Distinguish available-but-unselected providers from missing capabilities.
- Preserve route ceilings and keep declarations separate from execution and permissions.

### Discover versioned declared expert capabilities

- Add bounded immutable capability aliases while preserving legacy serialized hashes.
- Expose semantic-version lookup and resolve advisory intent/gaps against pinned providers.
- Keep declarations separate from permissions and skip oversized optional interface prompts.

### Reject ambiguous reasoning and residency provider fields

- Extend bounded duplicate-key rejection to reasoning responses and advisory JSON.
- Reject ambiguous native inventory before effects; preserve unknown outcomes after POST.
- Verify actual HTTP and CLI denials without retries or real lifecycle mutations.

### Reject ambiguous agent JSON objects

- Reject nested and escaped duplicate keys in HTTP bodies, provider objects, native schemas and tool arguments.
- Preserve existing byte/depth limits and valid value semantics.
- Verify real HTTP rejection before inference and keep client-owned tool permissions unchanged.

### Align inference charges with durable reservation outcomes

- Preserve prior charges and usage when a database reservation is rejected.
- Retain committed reservations when observer publication fails, without invoking inference.
- Distinguish post-commit notification failure by type; preserve fail-closed behavior.

### Keep bound expert audit failures fatal

- Restrict bound advice fallback to typed runtime timeouts.
- Prevent policy, audit and unclassified backend errors from invoking primary inference.
- Preserve timeout fallback and charges; keep pre-dispatch unavailable-binding skips.

### Verify partial structured advice containment

- Prove dependent workflow steps share one phase deadline without reset.
- Verify timed-out partial advice never enters main reasoning, with all calls still charged.
- Confirm behavior through actual CLI/HTTP and independent final-answer checking.

### Contain optional expert timeouts before whole-goal reasoning

- Bound the whole advice phase within original remaining task time.
- Permit audited shared-advice timeout fallback without refunding spent calls or tokens.
- Preserve fatal audit/non-timeout errors and unknown upstream completion/usage.

### Retain real Qwen graph costs and partial failures

- Freeze a bounded six-profile Qwen graph study: 56 of 96 tasks before deadline.
- Preserve expert timeout/transport failures and unmatched/unexecuted cases.
- Validate every pass with a separate path oracle; record no demonstrated expert benefit.

### Verify custom scheduling through public HTTP continuation

- Resume a non-default committed subtask order without replaying completed work.
- Assert original settings, charged calls and output allowances remain authoritative.
- Reject damaged schedule records before HTTP dispatch or task-state changes.

### Make dependency scheduling replaceable and recovery-pinned

- Add a trusted versioned subtask scheduler interface with strict dependency-order validation.
- Audit and persist selected order before execution; retain it through supported recovery.
- Reject missing or inconsistent new schedule records without silent policy fallback.

### Refresh current optimized installation evidence

- Install both current packages and pass complete CLI/socket and offline HTTP workflows.
- Record exact source, toolchain, binary/script hashes and successful hosted CI.
- Preserve the distinction between current host checks and older clean-OS validation.

### Preserve a revised solver before spending on critique

- Admit critique/reflection only when the complete revision chain fits original ceilings.
- Check remaining cycles, calls, output reservations and bounded prompt bytes.
- Audit skipped chains and preserve the current explicitly unverified candidate.

### Freeze reproducible public graph diagnostics

- Add 16 hashed shortest-path cases with separately stored oracle metadata.
- Verify labels by exhaustive path enumeration independently of runtime search.
- Keep public-data provenance and unavailable live-model evaluation explicit.

### Verify bounded shortest-path reasoning independently

- Check original directed unweighted graph criteria with deterministic Rust search.
- Integrate budgeted revision, checker bindings, recovery accounting and comparison scoring.
- Accept alternate optimal paths and verify unreachable goals without model confidence.
- Validate with 459 passing tests and a deterministic CLI/HTTP fixture.

### Contain an explicitly selected dependent expert set

- Add `skill-disable-set ID...` with a bounded, distinct operator selection.
- Validate the remaining dependency graph and commit all disable audits atomically.
- Reject incomplete selections without inferred cascades or manifest repair.

### Allow explicit containment of a damaged expert

- Validate the remaining graph when an operator disables a corrupt target.
- Preserve reverse dependencies, unrelated corruption denial and atomic audits.
- Keep damaged bodies intact and allow separately verified version rollback.

### Reject active expert routing metadata drift

- Match projected descriptions/tags against verified active manifest content.
- Deny inconsistent snapshots and health without silently repairing metadata.
- Preserve reviewed version activation and previously pinned task snapshots.

### Bound manifest reads and reject ambiguous active versions

- Guard stored manifest/hash/identity fields in SQL before Rust copies.
- Verify active rows incrementally and reject duplicate active identities.
- Verify stored bodies before accepting idempotent registration.
- Preserve corrupt records while denying implicit repair, activation or audit success.

### Bound skill inventory reads and shared identity admission

- Cap skill rows and SQL field bytes before copying metadata into Rust.
- Reject malformed tags and invalid metadata while preserving valid legacy empty tags.
- Share transactional ID capacity across legacy and manifest registration.
- Verify concurrent writers, no silent data repair and denial before inference.

### Separate executive expert selection behind a trusted router interface

- Add versioned Rust expert-router callbacks while preserving default tag routing.
- Validate selections against pinned enabled skills and retain the two-route ceiling.
- Audit before callback invocation and preserve database re-entry and ablation exclusions.
- Disclose configured router identity without granting permissions or attesting native code.

### Disclose benchmark configuration and bounded model identities

- Record declared configuration hashes, package version and compiled role-prompt identity.
- Share content-minimized task model/registration reads between bench and compare.
- Reject invalid or oversized identity records without copying reasoning bodies.
- Keep backend/weight attestation false and deterministic tool-only labels empty.

### Bound benchmark runs and retain cancelled results

- Add run-wide deadlines and Ctrl-C propagation through Engine cancellation.
- Retain completed prefixes, cancelled task charges and partial JSON reports.
- Report executed/unexecuted coverage and stop reason; exit unsuccessfully on interruption.
- Verify ready cancellation, active cancellation, deadline expiry and real CLI SIGINT.

### Preflight complete benchmark datasets before execution

- Reject invalid later cases and unsupported fields before any task or model call.
- Report dataset identity, runtime statuses and charged model/tool task counts.
- Distinguish exhausted no-call runs from successful deterministic tool benchmarks.
- Preserve exact scoring, label isolation and historical report contents.

### Record frozen public Rust diagnostic rerun

- Retain unchanged-label before/after reports and exact source/binary metadata.
- Record 6/20 and 8/20 strict matches, paired regressions and actual resource usage.
- Preserve imperfect format adherence and factual errors as release evidence.

### Respect explicit solver answer-format constraints

- Condition explanatory fact/guess separation on the original user's output format.
- Preserve strict scoring, labels, reference boundaries and resource permissions.
- Retain the original public Rust diagnostic and no-call admission preflight.

### Add compiler-backed Rust ownership diagnostics

- Add 20 balanced public compilation questions with source/dataset hashes.
- Verify labels and negative error codes using bounded rustc metadata compilation.
- Keep snippet execution and compiler tools outside the runtime and model permissions.
- Preserve explicit public-data and independent-review limitations.

### Verify residency inspection credential isolation

- Test dedicated credential forwarding and Unicode-escaped provider echo rejection.
- Keep provider metadata and both fixture credentials out of CLI output.
- Document the native REST ownership limitation without claiming live ownership.

### Inspect uncertain residency operations without changing state

- Compare a bounded recorded operator intent with one fresh native inventory GET.
- Reject corrupt, ambiguous or endpoint-mismatched intent before networking.
- Use read-only CLI startup; preserve tasks, configuration and registry state.
- Keep causality and ownership unproven and deny automatic retry or cleanup.

### Validate core and server installation on clean Arch

- Verify fresh prerequisites, pinned FlatBuffers and all 417 workspace tests.
- Install both optimized packages and pass CLI/socket and offline HTTP workflows.
- Record exact source/image references, binary hashes and completed HTTP CI.
- Refresh current verification summaries while preserving historical milestones.

### Verify installed HTTP workflows without model inference

- Document separate core/server package installs and verify both optimized packages.
- Add an offline HTTP smoke for authentication, origin denial and typed tool workers.
- Run the HTTP workflow in CI and include it in clean-OS validation instructions.
- Verify installed managed OpenCode/Qwen read/edit/read separately from offline tests.

### Reap collective workers after dropped task futures

- Retain inference/tool child, socket and execution lease through bounded drop cleanup.
- Drain collective and expert process ownership during HTTP shutdown.
- Block collective worker admission and fail shutdown after cleanup/audit errors.
- Verify dropped startup, HTTP disconnect, immediate shutdown and audit failure.

### Run native agent turns in owned typed workers

- Add authenticated one-task Unix conversation RPC with bounded generation-only grants.
- Support opt-in same-image native workers, registered models and scoped credentials.
- Retain child/socket/lease/capacity ownership until verified reaping, including cancellation.
- Append compatible proposal ceilings to bootstrap; preserve existing role workers.
- Prevent backend construction on already-cancelled turns and bound partial frames.
- Verify real managed OpenCode read/edit/read and five normal child exits.

### Define typed native conversation worker packets

- Add a dedicated FlatBuffers codec for conversation history, tools and completions.
- Validate packet identity, enum/allocation/history bounds and shared-string copies.
- Preserve tool-only answers and unknown usage; keep managed mode disabled until
  authenticated transport and child cleanup are integrated.

### Pin registered models for native agent turns

- Support selected immutable local model registrations in the native adapter.
- Record per-turn registration snapshots without claiming verified weight identity.
- Preserve admitted turns after operator disable and deny later admissions.
- Verify actual registered HTTP model selection and output-ceiling consistency.

### Serve modular OpenAI-compatible agent workflows

- Add a separate `cogfabric-server` workspace crate over the reusable core.
- Support authenticated loopback model discovery, native chat/function proposals,
  correlated tool results, buffered SSE, admission limits and cancellation.
- Preserve collective Engine aliases and same-image typed math/SQL workers.
- Add native turn budgets, limited generation-only policy and content-minimized audit.
- Verify OpenCode v2.0.26 read/edit/read with real LM Studio Qwen3 8B.
- Document setup and limitations; add a reproducible isolated OpenCode smoke script.
- Include the server in workspace CI while preserving the default CLI command.

### Verify installed tool ceilings through the socket service

- Exercise one-tool arithmetic/SQL execution and budget-preserving restore.
- Verify zero-tool direct/socket inference and arithmetic denial before launch.
- Retain worker reaping, service shutdown and socket cleanup checks.

### Reserve inline tool budgets for execution

- Skip unrelated retrieval for explicit arithmetic and inline SQL objectives.
- Complete these tasks under one tool allowance while preserving independent
  result checks, zero-budget dispatch denial and worker cleanup.
- Align typed progress, comparison totals and smoke checks with actual charges.

### Allow plain inference under a zero tool ceiling

- Skip and audit optional retrieval when no tool calls are available.
- Keep arithmetic, SQL and independent verifier dispatch denied at zero budget.
- Verify unverified plain generation without retrieved document/memory content.

### Prefer less queued alternatives at equal operator rank

- Use observed local channel occupancy when pinning inference, arithmetic and SQL
  routes with equal approved ranks, retaining module ID as the final tie-breaker.
- Preserve rank priority, existing task pins, permissions and bounded dispatch.
- Verify actual buffered contention and stable routing after queues drain.

### Release backend handles outside registry locks

- Commit retirement and shutdown metadata before dropping inference/tool backends.
- Prevent cleanup callbacks that inspect the registry from deadlocking on its lock.
- Cover inference, arithmetic and SQL retirement/shutdown cleanup with regressions.

### Retire inactive arithmetic and SQL backend handles

- Release inactive backend versions after their last task pins finish.
- Preserve immutable version and capability metadata after retirement, preventing
  identity reuse or cross-capability replacement.
- Verify retained execution, permission denial and actual backend release.

### Retire inactive inference backend handles

- Add trusted-host retirement that removes inactive versions from future rollback.
- Preserve existing task pins and release backend handles after their final use.
- Keep immutable identity tombstones and lifetime capacity limits; model residency
  and managed-child shutdown remain separate responsibilities.

### Preserve the running Linux worker image across upgrades

- Launch managed inference, arithmetic and SQL workers through `/proc/self/exe`.
- Verify a live service still launches and reaps all three worker types after its
  original executable pathname is atomically replaced.
- Retain the existing pathname fallback on other Unix platforms; no attestation
  or sandbox guarantee is added.

### Recover abandoned tool-worker audit sessions

- Extend exclusive-lease worker recovery to arithmetic and SQL sessions alongside
  inference, using one bounded transaction and combined session ceiling.
- Validate tool identity/capability/version metadata, retain compatibility with
  initial unversioned tool audit records, and reject oversized payload copies.
- Preserve task answers and existing charges; record interruption without tool
  replay, PID killing or invented external-effect/resource completion.
- Test mixed idempotent recovery, malformed metadata, rollback and live tool leases.

### Interrupted inference-worker startup cleanup

- Retain child, socket and execution-lease ownership before connection/setup awaits.
- Persist launch intent before spawning and owned PID before handoff; fail closed
  when audits fail and refuse implicit retry of an unfinished initialization.
- Reap failed/cancelled startups through the normal bounded cleanup path.
- Test real child cancellation, failed ownership audit, retry refusal and socket
  cleanup. Host crashes and stale-resource recovery remain separate work.

### Managed SQL task workers

- Add opt-in `configure-sql-worker` for lazy task-scoped built-in SQL processes
  in direct CLI and socket-service execution.
- Reuse the bounded tool broker and owned startup/reaping lifecycle; retain
  independent result checking and original task budget admission before launch.
- Add SQL-only COTB v2 setup while preserving arithmetic-only v1; reject
  capability/version mismatches and custom registry substitution.
- Test real CLI/service SQL, unsafe-query cleanup, exhausted-budget refusal and
  negative setup/scope boundaries. Crash cleanup and executable sandboxing remain open.

### Authenticated SQL process transport

- Add task-scoped structured COGQ Unix RPC with approved UID/PID peers,
  fixed session deadlines, cumulative call grants and replay rejection.
- Fail closed before backend execution when authorization audit persistence fails;
  close cancelled or malformed exchanges and omit query/result bodies from audits.
- Bind SQL registration to transport identity and reject cross-capability grants.
- Verify actual child-process SQLite execution and negative transport boundaries.
  Automatic SQL worker provisioning and executable sandboxing remain incomplete.

### Installed tool lifecycle verification

- Extend installed smoke checks to managed arithmetic authorization/reaping,
  rejected-task cleanup, SQL routing and restored worker configuration.
- Verify arithmetic/SQL service asks and retain binary/script hashes with evidence.
- Validate a fresh optimized installation and all 292 Rust regression tests;
  keep clean-OS setup and live-model release gates explicitly open.

### Typed SQL capability modules

- Route SQL through separately pinned, versioned tool modules and bounded queues.
- Add structured COGQ FlatBuffers tables/cells/results with decoded allocation limits.
- Independently rerun the constrained SQLite checker before accepting module output;
  preserve task admission, cancellation and arithmetic protocol compatibility.
- Keep SQL process provisioning and arbitrary executable tools explicitly unfinished.

### Managed arithmetic task workers

- Add opt-in `configure-tool-worker` and same-binary lazy arithmetic workers for
  direct CLI and socket service tasks, preserving bounded broker dispatch.
- Deliver an authenticated typed one-call task grant without inherited credentials.
- Retain ownership through interrupted startup, reap children and clean sockets;
  refuse custom registry substitution and exhausted tool allowances.
- Test real CLI/service execution, invalid arithmetic cleanup and unfinished-startup
  reaping. General executable lifecycle and crash cleanup remain incomplete.

### Authenticated arithmetic process transport

- Carry task-scoped arithmetic commands over mutually authenticated Unix sockets.
- Enforce immutable task/version/call/session grants and audit before execution.
- Reject duplicate correlations, forged responses, malformed frames and pipelining;
  close cancelled exchanges and drop pending worker futures on disconnect.
- Verify actual arithmetic in a separate process; automatic tool-worker provisioning
  and executable lifecycle remain incomplete.

### Typed arithmetic module dispatch

- Route arithmetic through bounded version-pinned FlatBuffers module queues.
- Retain task permission/budget enforcement and independently reject wrong answers.
- Cancel active and retired module versions during service shutdown.
- Keep general process command routing explicitly incomplete.

### Durable module event replay

- Add explicit COGS v2 catch-up with filtered cursor advancement and deduplicated
  replay/live delivery, preserving live-only v1 compatibility.
- Restore host-retained task/version/stage grants after registry changes; reject
  backwards cursors, version changes and receiver frame/time exhaustion.
- Keep automatic provisioning and general module commands outside this observer API.

### Authenticated process event modules

- Deliver filtered task progress to host-approved local processes using versioned
  FlatBuffers envelopes, mutual UID/PID checks and pinned module/stage grants.
- Bound frames, queues, lifetimes and writes; reject upstream commands and report
  loss explicitly while preserving task execution during observer cancellation.
- Test real child-process delivery, malformed identities/frames, overflow and
  deterministic deadlines. General module command routing remains incomplete.

### Failed-answer revision reconciliation

- Prepare an admitted revision boundary after a fresh independent rejection of a
  fully recorded whole-goal solver answer, without repeating inference.
- Preserve original allowances and repeat stopping; roll back proof, tool charge
  and boundary together on checkpoint persistence failure.
- Report revision readiness separately from verified success and require explicit
  continuation before generating a new answer.

### Repeated boundary recovery

- Admit later unconsumed solver boundaries while retaining the original context,
  deadline, settings and cumulative resource ledger.
- Validate each repeated context against chronological reservations and immutable
  bytes; bound assembly history and reject changes or exhausted allowances.

### Whole-goal boundary recovery

- Resume validated unconsumed boundaries for FAST and whole-goal fallback tasks
  without requiring an advisory graph.
- Preserve explicitly skipped plans and reject missing valid graph history;
  verify HTTP revision, original budgets and consumed-boundary denials.

### Durable revision recovery

- Persist exact bounded solver-boundary state before charging each reasoning cycle;
  stop dispatch if checkpoint persistence fails.
- Resume unconsumed boundaries after completed advisory graphs with original cycle,
  resource and repeat-detection state; reject altered or consumed checkpoints.
- Verify resumed independent checks through local HTTP without extra planner or
  advisory calls. Arbitrary within-cycle and no-graph recovery remain incomplete.

### Managed worker task recovery

- Continue validated advisory graphs through a fresh authenticated inference worker
  with remaining resource grants and the original host deadline.
- Convert admission ownership to shared execution under the database transaction;
  reap resumed children on completion or cancellation without replaying unknown calls.
- Verify actual CLI/process/HTTP behavior and exclusive recovery refusal.

### Registered model recovery

- Continue interrupted advisory graphs with their original enabled registered
  model version after verifying saved manifest and configuration identity.
- Reject missing, disabled or changed registrations without model substitution;
  verify actual HTTP model selection while retaining weight-attestation limits.

### Explicit interrupted advisory continuation

- Resume validated pending advisory nodes without repeating completed steps or
  resetting original resource/deadline ceilings; require operator approval.
- Correlate planner graphs, saved context and solver results with charged calls;
  refuse unsupported or unresolved checkpoints and preserve fresh authorization.
- Atomically persist terminal reports and typed outcomes; disclose partial timing
  and backend identity limits. General multi-phase recovery remains incomplete.

### Boot-bound recovery deadline evidence

- Persist original pre-setup Linux boot-time snapshots and reconstruct remaining
  wall allowance across processes on the same boot.
- Reject malformed/backwards clocks and preserve unavailable time after reboot
  or for legacy settings, without authorizing task replay.

### Measured advisory subtask costs

- Record per-node reservation and timing deltas, preserving unknown provider
  usage and distinguishing completed inference from verified results.
- Audit cancelled/skipped work and stop further advisory dispatch on cost-audit
  failure without changing task budgets.

### Installed binary workflow verification

- Add a repeatable fresh-state CLI workflow for installed binaries, covering
  deterministic tools, socket lifecycle, skills, RAG rollback and restore.
- Document and verify isolated optimized source installation; include the
  workflow in CI without claiming live-model or independent benchmark results.

### Atomic native lifecycle outcomes

- Commit final residency audit metadata and task status together, rolling back
  both on audit or status-update failure without retrying external effects.

### Explicit native model lifecycle

- Add operator-approved load/unload with exclusive workspace leases, exact native
  inventory preflight, durable intent and fresh inventory confirmation.
- Preserve uncertain external outcomes without automatic retries or unloads;
  disclose that native instance IDs do not establish exclusive ownership.

### Native local model residency inspection

- Add bounded operator-only LM Studio native inventory with installed keys,
  loaded instance IDs and reported context capacities.
- Require an explicit native endpoint and separate opt-in credential; reject
  redirects, malformed/oversized inventories and protected credential echoes.

### Service-owned broker cleanup

- Stop the shared inference broker after service handlers finish cancellation,
  revoking retained inference pins when the service exits.
- Verify typed STOP and host shutdown alongside durable task cancellation.

### Host inference broker shutdown

- Stop current and retired task-pinned inference modules with one permanent
  host-controlled signal; cancel active futures and discard queued work.
- Refuse new registration, pinning and dispatch after shutdown while preserving
  ordinary replacement and unregistration behavior.

### Release evidence reconciliation

- Track all 18 goal acceptance criteria with current evidence and remaining proof.
- Refresh the matrix test count and superseded recovery limitation; add the
  offline security command to CI and the README.

### Reproducible offline security evaluation

- Add `security-eval` with 24 fixed boundary probes, positive controls, fixture
  hashes, detailed results and unsuccessful exit status on mismatches.
- Explicitly report unmeasured security risks and exclude neural safety claims.

### Bounded interrupted-task reconciliation

- Bound and validate recovered task identities before updating durable state.
- Apply a five-second SQLite progress deadline with atomic rollback and callback
  cleanup, including when stored triggers perform expensive work.

### Reviewed comparison dataset pins

- Allow comparison runs to require an operator-reviewed SHA-256 before creating
  tasks, rejecting malformed pins and changed bytes without inference.
- Distinguish prior dataset pinning from the observed hash in reports while
  retaining independent-review and held-out limitations.

### Registered endpoint discovery

- Add operator-only discovery across enabled registered model endpoints, including
  expert configurations, with deduplicated bounded concurrent requests.
- Report exact registration versions/checksums and missing advertised IDs; skip
  disabled models and refuse oversized endpoint sets before network requests.
- Keep advertisement separate from generation, loaded weights and residency claims.

### Optional operator GPU diagnostics

- Add opt-in whole-device NVIDIA sampling with a fixed read-only command, bounded
  output/deadline and child termination/reaping on failure.
- Parse memory units, optional utilization/power and distinct device identities;
  retain unsupported or failed measurements as unavailable.
- Keep GPU observations outside task accounting and deny the capability to models.

### Preserve whole-goal reasoning budgets

- Include worst-case dependency prompt bytes and transport bounds in decomposition
  admission, preserving capacity for the first whole-goal solver.
- Skip research exploration when its cost would consume that solver's remaining
  call, output-token or input-byte budget; audit admission decisions.
- Cover four constrained-budget combinations without expanding operator limits.

### Provisional v1 evaluation corpus

- Add 100 original self-contained synthetic tasks in ten categories with stable
  IDs, hashed provenance inventory and explicit incomplete review flags.
- Validate corpus bounds/uniqueness and recompute thirty deterministic tool cases.
- Fix column-less SQLite reads for declared inline tables, with regressions retaining
  schema, unknown-table, temporary-database and attach denials.
- Document independent review and live evaluation gates without claiming held-out
  independence, neural safety or measured model performance.

### Structured SQL comparison scoring

- Add strict SQL result JSON comparison against operator-provided expected labels,
  preserving types and ordering while ignoring JSON object order and whitespace.
- Validate expected shapes before task creation and exclude failed/demo outcomes.
- Cover six-profile inline SQL execution and wrong-value/type/status denials.

### Offline dataset split manifests

- Require reviewed versioned train/validation/test identities before offline LoRA
  training; verify hashes and reject cross-split normalized prompt reuse.
- Record split provenance and hashes while keeping held-out rows out of optimization.
- Add small format examples and negative preflight tests; neural evaluation and
  promotion remain disabled.

### Ranked expert fallback

- Select workflow experts in relevance order instead of registry ID order.
- Skip unavailable model bindings without inference, allowing the next routed
  workflow while retaining one attempted workflow and shared task limits.
- Cover ranking, version replacement, disable fallback and admission audits.

### Expert model comparison disclosure

- Include per-case registered expert configurations and checksums in comparisons.
- Flag separately bound expert configurations as confounding same-model ablations,
  including failed/refused advice, without treating metadata as weight attestation.

### Registered-model workflow experts

- Bind workflow advice to immutable enabled local model registrations while sharing
  task budgets, permissions and bounded typed inference dispatch.
- Preserve in-flight bindings, honor live disable for new frozen-corpus tasks and
  retain failed-call charges without fabricating missing usage measurements.
- Cover independent HTTP selection, failure fallback and managed expert cleanup.

### Worker crash audit reconciliation

- Hold runtime execution leases through host startup and persistent worker serving;
  refuse launching inference for terminal tasks.
- Add operator-only recover-workers with bounded transactional interrupted-session
  records, preserving task results and never replaying inference or killing PIDs.
- Test live-worker exclusion, real child crashes, durable idempotency, malformed
  metadata and transactional audit failures.

### Task-bound worker lifetime

- Pin worker lifetime to the approved task wall ceiling through an additive
  bootstrap field, retaining the legacy 300-second default.
- Permit idle gaps within that lifetime while limiting incomplete frames to ten
  seconds from the first byte; verify expiration with deterministic virtual time.

### Managed same-binary inference workers

- Add opt-in configure-worker mode with lazy per-task child launch, authenticated
  typed bootstrap and pinned generation/database ceilings.
- Reuse one child across role calls, explicitly close/reap on task completion or
  cancellation, and retain kill-on-drop fallback plus identity-checked cleanup.
- Record launch handoff/reaping metadata; test actual CLI process execution,
  zero-process arithmetic, pending HTTP cancellation and custom-registry refusal.

### Typed single-task worker bootstrap

- Define bounded COGB FlatBuffers settings for a worker's approved task and pinned
  generation configuration, with no filesystem paths or executable grants.
- Validate version/role/configuration bounds and derive matching worker ceilings;
  test decoded bootstrap through the adapter/worker exchange.

### Durable worker decision auditing

- Require a trusted SQLite audit anchor for worker sessions; record bounded
  metadata-only decisions, reservations and outcomes without prompts or answers.
- Refuse dispatch on authorization-audit failure and atomically reject missing
  anchors; test database reopen, privacy and injected write failures.

### Authorized inference worker sessions

- Add worker-side host credential checks and immutable task/role grants.
- Enforce cumulative task budgets, bounded sessions and active cancellation on
  host disconnect/shutdown; reject forged identities before inference.

### Host-connected inference process transport

- Add a Unix inference adapter with host-approved OS peer UID/PID checks,
  bounded FlatBuffers exchanges and immutable task/module correlation.
- Close cancelled or invalid exchanges permanently; verify communication with a
  separate local fixture process without exposing process spawning to models.

### Task identity propagation correction

- Preserve caller task UUIDs in direct broker inference dispatch, matching the
  pinned engine path; reject malformed IDs before invoking the backend.
- Test concurrent distinct tasks through both entry points.

## [1.0.0] - 2026-10-09

### Added

- Local binary MessagePack protocol with versioned, bounded frames.
- Unix socket service and standalone CLI with English messages.
- SQLite event traces, FTS5 memory and dynamically registerable text skills.
- Pluggable deterministic demo and localhost OpenAI-compatible inference adapters.
- Resource-bounded Jev+-inspired routing and Jev-R logical cooperative roles.
- Retrieval candidate isolation, reproducible held-out retrieval evaluations,
  checksum verification and explicit human-gated RAG promotion.
- Optional, manually invoked offline LoRA/QLoRA experimental trainer.
- Security policy boundary and formal threat-model documentation.
- Automated regression suite, mutation test and CI configuration.

### Scope limitations

This is a **working orchestration reference runtime**, not a novel pretrained
neural reasoning model. FlatBuffers runtime, automatic training, LoRA deployment,
GPU-specific resource scheduling and performance parity with frontier LLMs are
not part of this release. See `docs/IMPLEMENTATION_MATRIX.md`.

## [1.1.0-alpha.1] - 2026-10-09

### Rust migration

- Ported the cognitive runtime to Rust 2021 with Tokio.
- Replaced MessagePack IPC with a typed, bounded FlatBuffers schema.
- Added Rust CLI, SQLite-backed task/skill/RAG candidate lifecycle, local model
  adapter, bounded Jev-R roles and independent capability gating.
- Preserved the original Python runtime in Git history on the `main` branch.
- Retained Python only for the optional offline LoRA experiment.

### Verification status

- **Build and tests not run**: rustc, cargo and flatc are unavailable in the
  authoring environment. This is an alpha port, not a verified replacement.

## Unreleased — 2026-10-10

- Compile the Rust alpha against the pinned FlatBuffers generator: fix required
  field accessors, alloc visibility and SQLite query lifetimes.
- Format the Rust workspace and record a reproducible Cargo.lock.
- Support an explicit FLATC path and reject nonmatching compiler versions.
- Verify 14 tests, Clippy, compilation and the CLI self-test; reconcile the
  implementation matrix with remaining v1.0 requirements.
- Constrain inference HTTP to literal loopback, disable redirects and proxies,
  bound request parameters and cap response bodies at 256 KiB. Add local HTTP
  boundary regressions (17 total tests passing with socket permission).
- Migrate unversioned SQLite state transactionally with future-schema rejection
  and bounded lock contention. Add candidate snapshot links, active-version
  retrieval, integrity-checked operator rollback and lifecycle audit records.
  Verify 19 tests, formatting, compilation and Clippy.
- Cancel tasks durably on disconnect, Ctrl-C and service shutdown; join bounded
  socket handlers before cleanup, authenticate peer UID and preserve replaced
  socket paths. Count dispatched inference calls and return failed CLI outcomes
  with nonzero exit codes. Add Unix socket and cancellation integration tests.
- Add bounded Jev-R critique/reflect/re-solve cycles with persisted role dependency
  nodes, replaceable inference backends and no-progress stopping. Keep generated
  results unverified without executable evidence. Measure optional token usage,
  runtime inference duration, prompt bytes, output reservations and tool counts;
  carry resource metrics through backward-compatible FlatBuffers response fields.
- Add immutable versioned reference manifests, dependency and cycle validation,
  staged activation, metadata rollback, disable, health checks and registry audit
  records. Pin task routing/reference snapshots transactionally and validate
  retrieved content hashes. Add trainer.s1 metadata without a neural backend.
- Persist validated loopback inference settings and independent role profiles
  through atomic owner-only config saves. Add configure-local/config-show and
  bounded no-follow regular-file reads across reviewed input boundaries.
- Parse bounded structured goals with a separate interface role, preserve original
  user instructions, handle clarification and missing capabilities, isolate
  explicit conversation references and optionally synthesize labelled results.
  Carry conversation/synthesis options through additive typed IPC fields.
- Copy reviewed candidate artifacts into bounded content-addressed read-only
  quarantine, verify deduplicated content, enforce aggregate storage limits and
  allocate candidate versions transactionally under concurrent imports.

- Require immutable held-out dataset provenance and active-baseline retrieval regression gates before RAG promotion; migrate evaluation storage to schema 3.

- Add typed service-status, cancel-active and stop controls that remain available during inference, with bounded connections and busy responses.

- Execute validated advisory subtask dependencies under the shared reasoning budget, persist graph transitions, and preserve a final solver for the original goal.

- Bind inference and tool dispatch authorization to immutable per-task actor grants, with persisted scope snapshots and denial decisions.

- Reconcile the current implementation matrix with verified code; document security boundaries and reproducible benchmark limitations.

- Add reviewed, reproducible same-model comparison profiles with optional larger baseline, per-arm resource totals, expected-answer isolation and durable run cancellation.

- Route inference through a bounded FlatBuffers module broker with trusted registration, discovery, version pinning, identity checks, backpressure and cancellation.

### Development: independently verify dependency-order answers

- Validate explicit original-request ordering constraints with a bounded Rust checker.
- Feed independent failure diagnostics into bounded solver revisions, stop on success
  or repeated answers, and preserve the exact verified result through presentation.
- Audit and budget every verification invocation; document the limited proof scope.

### Development: preserve rejected verification outcomes

- Keep failed checked candidates unchanged and mark them rejected through task results,
  typed progress, explicitly published history and reviewed retention.
- Fix progress reporting for rejected arithmetic expressions and test zero-call malformed
  criteria, single-call rejection and explanation requests after failed checks.

### Development: compare independently checked dependency orders

- Accept versioned dependency-order comparison criteria and score alternative valid
  answers fairly across all baseline and collective profiles.
- Validate the complete dataset before creating tasks and identify criteria in reports.
- Preserve existing exact-match datasets and test the full six-arm loopback HTTP path.

### Development: diagnose local storage and inference configuration

- Add operator diagnostics with optional bounded loopback model discovery, explicit
  missing-model reporting and meaningful failure exits without launching a task.
- Document initialization effects and distinguish model advertisement from inference,
  loaded weights, residency and GPU validation.

### Development: prepare reviewed RAG candidates from recorded gaps

- Inspect bounded capability-gap evidence and report reuse/review steps without granting permissions.
- Import reviewed gap-guided RAG artifacts with atomic candidate/provenance records
  and evidence hash revalidation; protect source-task evidence from retention.

### Development: measure bounded inference queue waiting

- Measure broker queue delays independently of backend claims and retain optional
  timing through compatible module/socket schemas, task traces and comparison totals.
- Preserve unavailable metrics on cancellation and legacy frames; test contention
  and reject invalid metric values without increasing runtime budgets.

### Development: invoke bounded versioned prompt/workflow experts

- Register and activate immutable reviewed workflow checklists alongside reference skills.
- Invoke at most one routed checklist under shared budgets and task permissions,
  retaining version/checksum audits and reserving whole-goal reasoning capacity.
- Keep workflow advice unverified and isolate it from baseline comparison profiles.

### Development: back up consistent SQLite state

- Publish bounded, private, checksummed SQLite snapshots including committed WAL
  data using the backup API and serialized operator admission.
- Verify copy/reopen behavior, permissions, contention and capacity failures while
  keeping configuration/artifacts separate and never replaying saved tasks.

### Development: stream bounded task progress over local sockets

- Add owner-authenticated `watch-task` with durable cursor catch-up and typed live
  events, JSONL output, terminal states and resumable timeouts.
- Bound watcher admission, queues and writes; report loss explicitly and preserve
  task execution when an observer disconnects.

### Development: report optional Rust process memory

- Sample bounded Linux procfs RSS and process-lifetime high-water RSS at task
  completion and in diagnostics; preserve unavailable values through IPC.
- Aggregate comparison memory as maxima and document scope, warm-up effects and
  exclusion of inference-server/GPU memory.

### Development: bound and audit the offline training experiment

- Fail when truncation leaves no supervised examples instead of looping forever.
- Freeze and hash validated dataset input, reject conflicting/ambiguous provenance,
  reserve separate output and record seeded configuration/dependency versions.
- Hash bounded inactive adapter bundles; add standard-library preflight CI tests
  while keeping neural evaluation/promotion disabled.

### Development: execute bounded SQL over task-local inline data

- Add deterministic SELECT tasks with isolated in-memory tables, audited task
  permissions and no inference requirement.
- Deny external access, mutations, unsafe functions and multiple statements;
  bound VM work, cancellation, data sizes and results, with hashed evidence.

### Development: run dependency-ordered expert advice workflows

- Extend immutable manifests with bounded acyclic advice steps while preserving
  legacy hashes and shared-backend behavior.
- Execute declared dependencies under shared budgets, audit step/result identities,
  and keep advice unverified and isolated from baseline profiles.

### Development: restore verified database snapshots into new workspaces

- Add operator-only checksum/integrity-checked database restoration with private
  staging, no-overwrite publication and a durable provenance record.
- Refuse existing destinations, unsupported schemas and live WAL inputs; retain
  task state without automatic replay or configuration/artifact substitution.

### Scoped operator inference authentication

- Support opt-in environment bearer credentials for one local API scope across
  generation, discovery and managed workers without persisting secret values.
- Preserve backward-compatible typed bootstrap and reject literal or JSON-escaped
  provider credential echoes before task results are recorded.

### Authentication boundary regressions

- Verify scoped primary and registered discovery with actual CLI requests, including
  absence of authorization on other scopes and safe escaped-echo failures.
- Verify missing credentials prevent direct and managed connections and model
  actors cannot authorize authentication configuration.

### Paired comparison changes

- Report per-arm case improvements and regressions against small-model-alone,
  with paired coverage, explicit denominators and unavailable empty rates.
- Exclude unexecuted deadline/cancellation cases from comparisons and expose their
  count instead of treating missing outcomes as successes or failures.

### Checked configuration restoration

- Include bounded effective runtime configuration in checksummed SQLite backups
  without persisting workspace paths or environment credentials.
- Add explicit restore-configuration opt-in with validated settings, private
  no-overwrite output and backward-compatible database-only defaults.

### RAG artifact recovery

- Add explicit RAG payload capture and restoration with hashes, bounded deduplication,
  private read-only quarantine and transactional candidate path rebasing.
- Reject missing/tampered/oversized payloads and candidate triggers; preserve candidate
  status and keep neural weights outside backups. Default backups omit payloads.

### Explicit snapshot schema migration

- Restore recognized schema-1–5 snapshots only with migrate-schema opt-in, applying
  bounded transactional migrations to staging without modifying the source.
- Record source/result schemas and preserve running tasks without replay; reject
  unknown versions and publish nothing on migration failure.

### Versioned event module routing

- Register/discover bounded host event modules and pin task/stage subscriptions to
  immutable versions while retaining in-flight subscriptions across replacement.
- Filter before shared bounded queue admission and preserve explicit overflow and
  durable catch-up without granting model observation or execution permissions.

### Event observer cancellation and recovery

- Close pending/buffered observations explicitly without cancelling task execution,
  releasing queue capacity while handles remain alive.
- Replay durable progress through the module's pinned task/stage filter, preserving
  pagination cursors across replacement, overflow and cancellation.

### Bounded complete audit inspection

- Add task-scoped task-events pagination with durable event sequences and stable
  cursors, preserving full histories without unbounded trace allocation.
- Refuse oversized/malformed full traces explicitly and read comparison model
  identities from bounded selected audit records.

### Durable inference reservation correlation

- Record charged call identity with reasoning results and expose a bounded,
  transactional inference-audit summary for recovery inspection.
- Validate result identity/hashes and preserve unknown legacy or unrecorded usage
  without refunding reservations, exposing answers or authorizing replay.

### Durable task execution settings

- Validate embedded runtime configuration before task creation and persist settings,
  effective reasoning ceilings and execution options before model dispatch.
- Fail closed on snapshot writes and preserve original settings without archiving
  workspace paths, environment credentials or restorable permission grants.

### Validated advisory graph recovery inspection

- Add operator `graph-audit` inspection of durable plans and subtask histories.
- Reject changed definitions, invalid transitions and premature dependent execution;
  report hashes and recorded states while keeping model results unverified.
- Preserve missing state explicitly without granting resumption permissions.

### Original inference budget reconstruction

- Validate durable execution settings and mode ceilings during inference auditing.
- Report remaining inference admission allowances while retaining all charges for
  unrecorded calls; reject histories exceeding their original saved limits.
- Keep unproven time/cycle/tool allowances unavailable and resumption unauthorized.

### Durable tool and reasoning-cycle charges

- Persist versioned reservations before tool admission and reasoning cycles;
  stop execution when reservation writes fail.
- Reconstruct remaining tool/cycle allowances conservatively for versioned ledgers,
  rejecting forged, discontinuous or over-budget charges.
- Keep older histories unknown and preserve deny-by-default permissions.
- Enforce the original cycle ceiling across repeated solve calls on one Session.

### Shared task deadlines

- Charge setup time to the original task wall budget and share its monotonic
  deadline with reasoning sessions.
- Reject expired tool/cycle/inference admission and cap inference timeouts by
  remaining task time, preserving cancellation priority.

### Model registry read integrity

- Reject model discovery entries whose embedded identities differ from stored row
  identities, even when their manifest checksums are valid.
- Bound stored manifest, identifier, version and checksum reads before Rust
  allocation and add tampered/oversized registry regression coverage.

### Independent integer sorting verification

- Add bounded `sort:` goals with deterministic ordering and multiplicity checks.
- Route checker failures through existing bounded Jev-R revision and charge each
  verification under an explicit task-scoped capability.
- Record the actual successful criterion in Engine evidence instead of assuming
  every structured verifier is dependency ordering.

### Independently checked structured-memory reuse

- Reuse exact structured sorting/ordering answers from operator-published episodes
  only after a fresh budgeted deterministic check.
- Record current verification evidence and memory provenance, completing valid
  matches without inference while retaining incorrect answers as untrusted data.
- Preserve revocation, profile isolation and verification tool-budget denials.

### Durable assembled reasoning inputs

- Preserve exact objective/reference/advice context and hashes before planning or
  solving, independent of later source-document changes.
- Bound raw/serialized snapshots and prevent subsequent inference when persistence
  fails; keep saved external data separate from permissions.

### Checked saved-result recovery

- Add operator recovery of abandoned structured tasks from a saved solver answer,
  with a fresh independent check and no inference replay.
- Validate and charge the original resource ledger transactionally; reject live
  leases, unresolved calls, missing allowances and terminal-task rewrites.
- Preserve failed-check charges while rolling back audit-write failures atomically.

### Versioned compiled checker experts

- Support reviewed checker manifests for existing integer-sort/dependency-order
  implementations using the registry's immutable lifecycle and dependency checks.
- Pin routed checker metadata and record its version/checksum in actual proof
  events, keeping compiled criterion identity separate from manifest version.
- Reject unsupported checker identities, workflows, model bindings and permissions;
  preserve existing manifest hashes and fixed kernel authorization.

### Work in progress: journaled worker-file cleanup

- Preserve the unfinished socket-journaling and operator preview/apply slice for
  private repository publication. It formats and compiles; cleanup-specific tests
  and full validation are pending. This is experimental and not a release gate pass.

### Verified journaled worker-file cleanup

- Complete the previously unfinished cleanup slice with real host-death and
  negative filesystem/lease/plan/journal/audit tests.
- Revalidate identities after intent persistence and document partial failures,
  pagination, Linux requirements and unjournaled/same-owner limitations.
- Pass all mandatory checks with 318 Rust tests; installation verification follows.

### Immutable tool rollback and real local integration

- Retain count-bounded immutable arithmetic/SQL backend versions for trusted host
  rollback, preserving in-flight pins and rejecting historical capability changes.
- Test conflict/shutdown/redefinition/full-history rollback and SQL results.
- Record successful direct Ollama generation and managed sorting with independent
  verification and confirmed reaping; retain outstanding live study limitations.

### Immutable inference backend rollback

- Retain original backend versions and queues for trusted host reactivation.
- Preserve in-flight role snapshots; refuse active role conflicts and unknown
  versions without granting permissions or renewing remote sessions.
- Verify rollback at the history ceiling, original backend/queue identity,
  role isolation and shutdown. Retained history is count-bounded, not a GPU quota.

### Preserve connected inference identity

- Expose fixed inference transport descriptors and reject mismatched registration
  metadata before dispatch.
- Preserve connected IDs/versions/roles in initial broker discovery and task pins.
- Provide fallible initial construction and test no-dispatch rejection plus actual
  child-process role isolation. No loaded-weight attestation is implied.

### Event module version rollback

- Retain bounded immutable event descriptors for trusted host reactivation.
- Preserve in-flight subscription and durable replay filters across rollback.
- Enforce active/history capacity and reject unknown/redefined versions without
  adding observer permissions or executable loading.

### Configure bounded interface reservations

- Add operator-selected structured-interpretation output allocation within task
  ceilings, preserving the legacy 128-token default and configuration identity.
- Pass the admitted reservation through existing typed inference commands.
- Test legacy settings, invalid limits, exact charging and refusal before dispatch
  when the reservation cannot fit. Empty provider output remains a visible failure.

### Preserve opt-in provider reasoning controls

- Add a strict optional local provider effort setting without changing default
  requests, permissions or task budgets.
- Carry explicit settings in COGB v2; retain legacy v1 and reject mismatched
  versions/fields and invalid controls.
- Verify direct/managed HTTP field propagation, absence by default and schema
  compatibility. Provider compliance and quality remain separately measured.

### Preserve solver budget after interface work

- Admit initial planning from remaining call/output/input allowances, preserving
  capacity for the first whole-goal solver after already charged advisory work.
- Audit skipped/admitted planning and retain decomposition/exploration reservations.
- Regress independent call, token and prompt-byte limits after malformed intent;
  preserve the original objective and still dispatch an affordable solver.

### Accept exact fenced structured intent

- Accept one outer Markdown JSON fence around a bounded schema-valid intent.
- Reject surrounding instructions, multiple objects and malformed wrappers while
  preserving original-goal authority and deny-unknown-fields validation.
- Regress real observed formatting through bounded interface/solver execution.

### Structural integer sorting comparisons

- Score sorting answers against supplied integer multisets and direction across
  all comparison arms, independently of answer whitespace and runtime verdicts.
- Reject mixed criteria/expected labels and preserve ordinary task/resource reports.
- Cover duplicates, integer limits/types and actual six-arm HTTP execution.
- Supply public demonstration cases and disclose shared compiled verifier code;
  no held-out independence or live performance claim follows.

### Parse bounded structured advisory roles consistently

- Reuse exact raw/fenced JSON parsing for interface, planner and critic outputs.
- Retain per-role byte bounds, graph/schema checks, budgets and unverified status.
- Exercise fenced dependency execution and bounded critique revision; reject forged
  permissions, surrounding instructions and multiple objects.

- Add explicit public comparison workflow checks with distinct report provenance;
  retain held-out review requirements for benchmark runs and reject conflicting
  run purposes.
- Expose observed local inference adapter availability and refuse routing or
  rollback activation of permanently discarded Unix connections.
- Apply observed local transport state and closed-connection routing refusal to
  arithmetic and SQL capability modules without renewing task grants.
- Exclude expired and call-exhausted Unix tool grants from routing and rollback
  without resetting authority or cumulative charges.
- Propagate managed inference/arithmetic/SQL adapter availability through worker
  wrappers; report expired tool deadlines before any child launch.
- Make managed worker shutdown terminal before initialization and coordinate
  handoff publication with cleanup to prevent post-shutdown launches.
- Add explicitly approved ranked inference alternatives with deterministic task
  selection, immutable version policy and no hidden retries of pinned calls.

### Route explicitly approved tool alternatives

- Add immutable bounded ranks for arithmetic and SQL candidates, deterministic new
  task selection, and independent active-module capacity limits.
- Preserve exclusive registration, pinned backends, rollback, typed dispatch and
  task authorization; test selection, capability boundaries and registry capacity.
- Record a successful operator-requested LM Studio Qwen3 8B sorting smoke and
  clarify how to select an out-of-PATH matching FlatBuffers compiler.

### Explain empty local-model answers

- Give actionable fixed errors for reported output-limit termination and
  reasoning-only responses without exposing internal reasoning or retrying.
- Make HTTP response fixtures independent of saved operator model settings and
  cover null, blank and reasoning-only answers.

### Record Qwen3 8B comparison workflow evidence

- Measure all six public workflow profiles with the operator-selected local model,
  retaining exact usage and audited normal worker cleanup.
- Distinguish baseline formatting failure from numeric correctness and record
  collective overhead, empty retrieval context and absent revision gains.
- Record two successful default-parallel regression runs without erasing earlier
  unexplained failures.

### Separate retrieved evidence from solver instructions

- Quote nonempty reference data as a JSON string and restate the original task
  after it; retain empty-context prompt hashes and tight-budget behavior.
- Give all model roles fixed reference-boundary advice without granting authority.
- Record a live answer-contamination failure, failed system-only mitigation and
  one successful quoted-reference trial without claiming general safety.

### Bound SQL admission and worker waiting

- Admit two executing and at most four additional queued inline SQL requests per
  process, with immediate overload rejection and a one-second queue timeout.
- Preserve cancellation safety and hold permits through actual blocking completion;
  reject oversized requests before admission.
- Verify queue exhaustion, deadline, cancellation, permit restoration and successful
  queued execution with independent capacity fixtures.

### Verify retained recovery lease ownership

- Test duplicate-descriptor lock retention, close-on-exec flags and fail-closed
  recovery without audit mutation until the last descriptor closes.
- Refresh release-status evidence for 354 tests, successful LM Studio smoke and
  restored private GitHub synchronization; preserve unresolved contention limits.

### Verify current installed runtime with live socket inference

- Record current optimized installation, complete installed smoke and real Qwen3
  8B generation through the installed Unix socket service and managed worker.
- Preserve binary hashes, exact resource metrics and normal worker/service cleanup
  evidence without extending the claim to general model quality or release readiness.

### Declare durable reasoning prompt formats

- Record the quoted-reference format in new reasoning-context snapshots and reject
  unsupported declarations before recovery dispatch.
- Retain compatible undeclared checkpoints under exact hash validation and provide
  an explicit error for incompatible saved prompts.
- Verify compatible metadata preserves original context and cumulative charges;
  reject unknown/null formats and altered hashes without automatic prompt migration.

### Preserve explicit restrictions in language interpretation

- Instruct the advisory interpreter to retain explicit prohibitions and limits
  when simplifying objectives, without changing runtime authority.
- Record a real omitted constraint and one successful repeat after the change.
- Document the exercised Qwen3 8B public CLI setup with a 384-token interface
  reservation and explicit no-thinking advice; keep prose results unverified.

### Bootstrap the exact FlatBuffers compiler

- Add an unprivileged compiler bootstrap pinned to the reviewed upstream commit,
  with bounded build concurrency, exact version checks and overwrite refusal.
- Use the same script in CI, document persistent FLATC setup and make missing
  compiler build errors point to an actionable command.
- Exercise the actual upstream source build and negative argument/destination cases.

### Add typed operator tool discovery

- Expose bounded host tool inventories through local CLI and an append-only
  FlatBuffers service control, preserving capability/version/rank/availability.
- Validate inventory identities, conflicts, limits and enum values; cover wire
  compatibility and malformed entries with local tests.
- Bound borrowed fields before copying and cap envelope verifier resource use.
- Verify socket inventory and expanded CLI/service smoke; all 358 tests, formatting,
  all-target check, Clippy and pinned-generator schema conformity pass.

### Verify hostile inbound tool inventories

- Exercise registry conflicts and capacity limits with forged FlatBuffers,
  bypassing the trusted encoder.
- Verify early borrowed-field rejection and shared-reference verifier exhaustion.
- Pass all 359 tests and mandatory quality checks.

### Verify clean Arch installation

- Include build and smoke-test prerequisites and locked builds in Arch setup.
- Document the image-pinned disposable clean-install workflow.
- Verify fresh dependencies, all 359 tests, quality checks and optimized installed
  CLI/service smoke on Arch Rust 1.99.0 without the host Cargo cache.

### Expose typed inference lifecycle inventory

- Add local and socket `inference-modules` discovery for retained versions, roles,
  active selection, local availability and immutable routing ranks.
- Validate inventory conflicts, bounds and unknown states with backward-compatible
  typed FlatBuffers fields and forged-frame regressions.
- Pass all 361 tests, mandatory checks, schema conformity and expanded CLI/service
  smoke; model residency and general executable provisioning remain separate.

### Equalize structured output instructions across comparison profiles

- Provide sorting/ordering output contracts independently of executable checking,
  so the model-alone baseline receives the same format instruction.
- Preserve baseline permission limits and unverified runtime status; leak no
  expected answers into prompts.
- Verify identical solver prompts and unchanged enabled verification; all 362
  tests and mandatory checks pass with isolated-build CLI/service smoke.

### State arithmetic and SQL baseline output contracts

- Supply numeric-only arithmetic and columns/rows SQL JSON instructions without
  executing tools or exposing expected outputs to the baseline.
- Verify one-call, zero-tool unverified baseline execution with captured prompts.
- Pass all 363 tests, mandatory quality checks and isolated CLI/service smoke.

### Bind continuation to compiled role prompts

- Record a canonical fingerprint of compiled default role/system instructions
  before dispatch, including the shared untrusted-reference boundary.
- Refuse continuation when its saved fingerprint is missing, null or changed;
  preserve history and charges without granting permissions or automatic migration.
- Pass all 364 tests, mandatory quality checks and isolated CLI/service smoke.

- Add read-only `resume-preview` with shared admission validation and remaining budget reporting; approved resume still revalidates.

- Bypass initialization and migrations for recovery preview; preserve workspace permissions and reject missing or legacy state.

- Observe inference and tool inventory availability outside registry locks to avoid callback lock recursion.

### Reconcile current V1 evidence and module security diagnostics

- Extend deterministic security probes to cover operator-only module permissions and reasoning task-scope denials.
- Require those controls in the reproducible offline smoke workflow.
- Replace stale current test/endpoint claims with scoped release evidence and preserve historical records.

- Refresh optimized CLI/HTTP installation evidence at `e1f3ffd`; both reproducible offline workflows pass without an installed FlatBuffers compiler.
