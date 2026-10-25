# Implementation matrix — 1.1.0-alpha.1

## OpenCode cache placement after live VRAM failure — 2026-10-25

**Implemented; live CPU-cache loading verified.** Architecture sections 6 and 9:
new launcher loads request CPU KV caching through the existing Rust native adapter,
which validates echoed configuration and fresh inventory before issuing a receipt.
`COGFABRIC_OPENCODE_KV_GPU=true|false` selects placement (default false); this does
not alter model-layer GPU placement or reconfigure existing instances. CPU caching
reduces VRAM demand but increases host RAM use and may reduce inference speed.
Live LM Studio logs showed a 32768-token/four-slot load failing to allocate a
4608 MiB CUDA KV buffer. Uncertainty remains recorded and requires explicit operator
stop acknowledgement/resolution; no automatic retry or history reset was added.
All ten launcher scenarios passed, including explicit GPU selection and invalid
settings. Shell syntax, formatting, all-target compilation and strict all-target
Clippy passed. Full all-target/reflection tests were not run, following the
operator's restriction. After explicit operator stop acknowledgement, the original
failed task was resolved without changing its unknown outcome. One corrected live
load succeeded in 17.087 provider-reported seconds; echoed configuration and fresh
inventory confirmed Qwen3-8B at 32768 tokens with CPU KV caching. The installed
launcher then passed live discovery/context checks, started its authenticated
HTTP server, reached a non-interactive substitute client, and cleaned up its own
server. This checks startup, not interactive OpenCode inference or model quality.


## OpenCode startup model activation — 2026-10-25

**Implemented for installed LM Studio models; live loading unverified.** Architecture
sections 4, 6, 9 and 10: `scripts/opencode-local.sh` invokes the existing audited
Rust `residency-managed-load` API for the exact configured unloaded LLM at 32K
context before inference diagnostics. Compatible instances are reused, unrelated
instances remain intact, missing models are not downloaded, and undersized loaded
instances require manual reload. `COGFABRIC_OPENCODE_AUTO_LOAD=0` disables activation.
Dedicated residency credentials remain separate from inference/client credentials.
Pending/blocked lifecycle history refuses activation/reuse in automatic mode;
uncertain calls are never retried or unloaded. Confirmed models remain resident
on exit, consuming provider-managed memory. No model or client tool receives
residency authority; no physical GPU quota or weight attestation is implied.

`tests/opencode_autoload.rs` covers unloaded/reused/undersized/missing models,
disabled activation, unknown load outcomes, blocked history and offline providers.
All eight launcher scenarios and 14 existing managed-residency HTTP integration
tests passed (the latter require loopback access outside the restricted sandbox).
Shell syntax, formatting, all-target compilation and strict all-target Clippy passed.
The installed launcher reached live LM Studio and refused the existing 4096-token
Qwen instance; live loading was not exercised. Full all-target/reflection tests
were not run, following the operator's restriction on reflection testing. See [OpenCode setup](OPENCODE.md).



## Task-pinned alternate typed expert providers — 2026-10-24

**Implemented for trusted typed modules; broader adaptive scheduling remains
Partial.** Architecture sections 4, 6, 7 and 9: `src/fabric/modules.rs` permits
multiple active identities per capability and atomically captures a task's ordered
candidate sets. Engine snapshot v5 records exactly those pins. Queue reservations
precede dispatch intent; full/closed candidates may be skipped only before intent.
Reservations bind their original deadline and, for candidate routing, capability.
After intent, errors never trigger an alternate-provider retry. Exact-pin dispatch
retains deadline-bounded queue waiting and ordinary authorization remains required.

`src/recovery/continuation_modules.rs` and dispatch-history validation restore
original v5 identities, including retained closed candidates without treating them
as healthy; missing/retired/mismatched identities still refuse. Versions 1–4 retain
single-provider rules. Explicit expert bindings remain exact; verifier diagnostics
record the selected identity without replacing compiled acceptance criteria.
This supersedes the earlier single-provider limitation for trusted typed modules.

Fifty-one focused broker, recovery, registry, engine, CLI worker and Wasm checks
pass, including full-queue selection, atomic snapshot retention, no retry, grant
limits, cancellation and v4/v5 compatibility. Default/all-feature all-target build
checks, strict all-feature Clippy and formatting pass. The full all-target test
suite and reflection/live-model campaigns were not run. Selection probes at most
32 candidates, adds no model call or permission, and is not calibrated quality
ranking, executable isolation or automatic model residency. See
[typed provider selection](TYPED_PROVIDER_SELECTION.md).


## Per-call provider timing inspection — 2026-10-24

**Implemented; 16 focused unit/CLI checks passed.** Architecture sections 4, 7 and 9:
`src/recovery/inference_metrics.rs` links optional reported metrics to validated
call reservations and preceding reasoning hashes in the public operator audit.
Missing reports remain null; coverage is explicit and timings/rates are not summed.
`src/recovery/inference.rs` uses one read transaction and a shared five-second
cooperative deadline, clearing SQLite's progress handler after errors and success.
`src/main.rs` runs `inference-audit` against an existing read-only current-schema
Store before bootstrap, migration or engine construction. Recovery admission still
uses the original ledger validator, independent of advisory metrics.

No call is retried, refunded, verified or authorized by inspection. Physical
measurement attribution, provider accuracy and calibrated scheduling remain Partial.
See [provider metrics](PROVIDER_METRICS.md) for field coverage and boundaries.







## Gap-bound reviewed source catalogs — 2026-10-24

**Implemented catalog discovery; open-web discovery remains Partial.** Architecture
sections 4 and 8: `src/evolution/catalog.rs` validates a hash-pinned reviewed catalog,
selects exact-capability source declarations by stable priority/ID, deduplicates
content hashes and writes an unapproved acquisition plan. Initial/publication
checks bind the exact recorded gap and refuse available capabilities so existing
skills can be reused. Completed/failed gap tasks remain eligible.

The human-only `candidate discover-remote-sources` CLI uses an existing read-only
Store, no bootstrap or network. Strict catalog/history parsing, bounded source
counts, metadata bytes and SQLite inspection deadlines govern the operation.
`src/gaps.rs` rejects duplicate JSON keys and clears its deadline callback on
both successful and failed inspection. Selection creates no candidate, model
permission or evaluation result. Source priorities/review labels are assertions,
not measured usefulness, legal assurance or independent curation. See
[source catalogs](SOURCE_CATALOG.md). Twenty-six focused checks pass across catalog,
gap inspection and the existing acquisition/preparation/import pipeline. Formatting,
all-feature compilation and strict Clippy pass. Full reflection tests were not run.


## Acquisition lineage through candidate registration — 2026-10-24

**Implemented.** Architecture sections 7, 8 and 10: acquisition drafts bind exact
plan/report hashes; `src/evolution/acquisition/lineage.rs` strictly validates
bounded offline origin metadata without network access. Preparation reads fixed
metadata files through its retained directory descriptor, matches every selected
source to its acquired ID/path/hash/labels/bytes, and emits report version 3 with
exact acquisition snapshots. Unbound local reports retain version 2.

`src/evolution/prepared.rs` validates report-version/evidence consistency and source
correspondence during import and durable inspection; `src/store.rs` invokes that
same validator at transactional candidate registration. Existing audit failure
rolls back registration and its anchor. Complete reports remain capped at 65,536
bytes; metadata-heavy bundles can be refused before publication. This preserves
origin metadata, not held-out document text, across source-bundle removal.

Focused acquisition/pipeline, local preparation/import and persistence checks pass;
formatting, all-feature compilation and strict Clippy pass. The pipeline regression
also removes original/prepared bundles before reading durable provenance. The first
v3 import exposed a stale v2-only Store gate, fixed through the shared validator.
No full reflection suite was run.

No new tool/network permission, evaluation authority, neural promotion or model
call is introduced. Hash-bound local records are not proof of remote contact or
independent licensing/quality review against a hostile owner. See
[source acquisition](SOURCE_ACQUISITION.md) and [preparation](SOURCE_PREPARATION.md).


## Operator-selected remote source acquisition — 2026-10-24

**Implemented bounded acquisition; remote discovery and independent curation
remain Partial.** Architecture section 8: `src/evolution/acquisition.rs` fetches
approved hash-pinned HTTPS text into a private unreviewed source bundle. Entire
manifest preflight, public-only DNS validation/pinning, no proxies/redirects/auth,
strict framing, exact body hashes and document/aggregate/deadline limits govern
network work. At most four independent resolver threads retain permits through
libc completion; timeouts cannot trigger a later HTTP request. The original plan,
reviewer assertion, hashes and labels remain in private provenance files.

The human-only `candidate fetch-sources` command runs before runtime initialization.
Generated preparation drafts have no review or held-out assertions. Existing
preparation, quarantine, independent evaluation, promotion and rollback gates
remain required. No model network capability or new active weights are introduced.
Focused URL/DNS/framing/pin, private HTTP, bundle/preparation and CLI-refusal
checks cover supported mechanics; public HTTPS acquisition and independent
licensing/poisoning resistance remain unverified. Each source uses a separate
bounded resolution/client; this adds network/TLS work only when the operator
invokes acquisition. See [source acquisition](SOURCE_ACQUISITION.md).


## Runtime WAL growth boundary — 2026-10-24

**Implemented for runtime file connections.** Architecture sections 7, 9 and 10:
`src/storage/wal_vfs.rs` registers an explicitly selected, non-default SQLite VFS.
WAL writes/truncations beyond 16,000,000 bytes fail before the delegated file
operation. Preallocation hints cannot bypass the check, and WAL mmap fetch is
disabled. Store creation/continuation/inspection/migrations, token ledgers and
backup/restore staging select it. `storage-status` reports the actual connection's
marker and byte ceiling; in-memory/unwrapped connections report no hard limit.

Pinned-reader tests with two runtime writers stop at `SQLITE_FULL`, retain all
committed rows and resume after checkpointing. An external oversized WAL remains
readable and unchanged by a denied write, then recovers through checkpointing.
The adapter adds a file-size observation per WAL write and can reject large single
transactions. No task result is manufactured on storage failure. This bounds
cooperating connections' WAL growth, not total disk allocation, external writers,
SHM, backups or arbitrary artifacts. Workspace-wide physical quotas remain Partial.
The human-only `storage-checkpoint` CLI uses a short lock wait and reports busy
readers; inspection/maintenance avoid bootstrap writes. Focused storage, callback,
legacy migration, backup/restore, token-ledger/managed-worker and CLI checks pass,
with default/all-feature compilation and strict all-feature Clippy. The full
reflection suite was not run. See [storage limits](STORAGE.md) for exact commands.


## Token preparation requires a live task — 2026-10-24

**Implemented.** Architecture sections 7, 9 and 10: `src/token_admission.rs`
checks the task status through a bounded read-only database connection before
rendering or tokenization can expose a prompt to the provider. Missing and
terminal identities fail without native HTTP requests. The independent immediate
write transaction still rechecks status before reserving tokens and dispatching
generation; a task stopped during preparation cannot obtain that reservation.
Two new regressions in `tests/input_token_admission.rs` cover both boundaries.
This adds one local database read per admitted attempt, grants no permissions and
makes no claim that cancellation stops already-started remote preparation.
The WAL hard-size bound remains an open storage requirement.


## Exact wire-input token admission — 2026-10-24

**Implemented for the optional llama.cpp native adapter; general provider support
remains Partial.** `src/token_admission.rs` renders complete text history, counts
and submits identical token IDs, and durably charges a cumulative task allowance
before generation. Context admission includes the reserved output. Concurrent
reservations, malformed ledger history and audit failures fail closed; failures
never refund input. `Config.input_token_admission`, the human-only CLI setting,
typed bootstrap v3, backend capability gates and managed credential forwarding
preserve the limits across supported dispatch paths. Default behavior is unchanged.

Five loopback/managed-worker integration tests pass. They cover exact IDs, context
refusal, provider failure without refund, concurrent admission, audit-write failure,
chat history, early unsupported requests and dedicated credential separation.
See [input token admission](INPUT_TOKEN_ADMISSION.md) for commands and restrictions.
Tools, role profiles, non-none provider effort and native LM Studio lifecycle are
unsupported in this mode. Real llama.cpp inference, physical GPU/energy enforcement
and reasoning quality remain unverified. No reflection campaign was run.


## Kernel bounds for managed workers — 2026-10-24

**Implemented; focused resource-boundary verification passed.** Architecture
sections 4, 9 and 10 now have optional Linux per-process virtual address-space and
CPU-time limits for managed native chat, inference, arithmetic, SQL and compiled
typed workers. `src/process_limits.rs` restricts only the child before exec,
preserves stricter inherited limits, disables child core dumps and refuses setup
failure. `Config.worker_process_limits` defaults to None; the human-only
`configure-worker-limits` command validates and saves it. Existing services require
restart, and existing tasks preserve their admitted settings.

Typed module identities and reconstruction retain exact original limits. New
launch/ownership audits report requested bounds without claiming physical usage
measurement. See [worker limits](WORKER_LIMITS.md). This does not constrain an
external model provider, GPU, total task CPU, RSS, filesystem or network, and is
not a full sandbox. The logical scheduler's allocation declarations remain distinct.

Four subprocess regressions pass (one ignored fixture is explicitly invoked by
them), along with two configuration/permission tests, one original-limit recovery
test, seven bounded native-worker tests and the executable-upgrade worker test.
A temporary CLI workflow verifies typed arithmetic under limits and clearing the
setting. Default and all-feature workspace all-target checks and strict Clippy pass.
The NVML feature check also exposed and fixed an unnecessary return. No reflection, live reasoning-quality or
performance benchmark was run for this milestone.


## Same-model per-turn reasoning effort — 2026-10-24

**Implemented and focused verification passed.** Native `cogfabric-agent` requests
accept `reasoning_effort` (`none`, `low`, `medium`, `high`). `Turn` validates and
pins it; `protocol/agent.fbs` appends a typed enum with an unspecified legacy
default. Direct and managed adapters forward explicit effort over the saved
configuration default while retaining the same model and token/call/time ceilings.
Invalid values fail before inference; provider rejection does not trigger a model
switch. Collective aliases retain their separate single-goal contract.

Chat exposes `--reasoning-effort`, `COGFABRIC_CHAT_REASONING_EFFORT`, and interactive
`/effort` display/change/default commands. `/reset` preserves the selected effort
and spent budgets. OpenCode variants on the single `cogfabric-agent` model use
`settings.reasoningEffort`; the launcher accepts configured variant suffixes.
The existing local OpenCode config was extended without changing permissions.
See [chat](CHAT.md), [API](HTTP_API.md) and [OpenCode](OPENCODE.md).

Verification: pinned FlatBuffers 25.12.19 regeneration, workspace all-target check,
strict workspace all-target Clippy, formatting, five protocol tests, two provider
fixture tests, eleven HTTP tests, seven managed worker tests and the explicit
installed OpenCode high-variant read/edit/read regression pass. The latter checks
that tool turns carry `high` while the same API model remains selected. A local
chat command-only session verified environment selection, runtime changes, reset
preservation and return to the server default. No new live-model reasoning-quality
comparison was run; effort effectiveness depends on the configured provider/model.


## Configurable chat frontend — 2026-10-24

**Implemented; production compilation and strict chat-binary Clippy pass.** Chat model, endpoint, credential
variable name, timeout, temperature, turn/token/session budgets and input/history/
response capacities accept CLI flags and `COGFABRIC_CHAT_*` environment variables.
Flags take precedence; existing defaults remain. `src/chat_client.rs` exposes
validated `Options`/`Limits` while retaining the default constructor. The frontend
uses those same capacities for bounded input, history trimming and response reads;
its messages display the selected limits. Checked total token reservation, finite
outer bounds, loopback validation and tool refusal remain enforced independently
of the model. No server budget is raised by client settings. See [chat](CHAT.md).
No live chat or new test suite was run for this configuration-only milestone.


## OpenCode startup and real-client coverage — 2026-10-24

**Implemented; compatibility verified within the stated scope.** The local
provider configuration incorrectly used a credential value in OpenCode's `env`
list; the local file now references `COGFABRIC_SERVER_API_KEY`. Local OpenCode
config files are ignored by Git and `opencode.example.json` contains no secrets.
After reboot, neither API service was running. LM Studio was restarted on
loopback only; the helper starts/reuses an authenticated Cognitive Fabric API.

`scripts/opencode-local.sh` (installed as `cogfabric-opencode`) validates the
provider, saved backend/ceilings and upstream discovery, creates an ephemeral key
for its own service, passes the key to OpenCode and cleans up only that owned
service. Configurations and model weights are not automatically rewritten.
Private logs support diagnosis. See [OpenCode startup](OPENCODE.md).

The installed OpenCode **v2.0.32** completed **read → edit → read** with the real
local Qwen3-8B model in the isolated permission-limited smoke fixture
(`/tmp/cogfabric-opencode-smoke.GUS5Vt`). The new explicit opt-in regression
`crates/cogfabric-server/tests/opencode_client.rs` also passes against the real
OpenCode executable with a deterministic in-process backend. All **10 HTTP
compatibility regressions** pass. This proves the tested tool/stream/history path,
not general model quality or compatibility with every agent client.

A normal repository session additionally exposed a provider-context mismatch:
Qwen was actually loaded with 4096 context while OpenCode declared 32768. Model
advertisement alone is insufficient. The launcher now checks provider-reported
loaded context when native inventory is available and refuses a smaller value.
Unknown native inventory is reported explicitly for other providers. After explicit
operator approval, the same Qwen model was reloaded with 32768 context, one parallel
slot and partial GPU offload (automatic placement failed). The normal repository
OpenCode session then returned READY and successfully executed a Cargo.toml read
and answered with the correct package name. Ten HTTP tests and strict server
Clippy pass; GPU-option map simplifications fix warnings exposed by that check.
No general release-readiness or performance claim follows.


## Source setup and standalone chat — 2026-10-24

**Implemented; live installation/chat verification deferred.** `scripts/setup.sh`
builds the main CLI, `cogfabric-chat` and HTTP server from a selected source ref
with locked dependencies, no sudo and an explicit replacement flag. It preserves
runtime state and supports optional NVML observation. README and
[installation guide](INSTALL.md) document curl download, review and revision
pinning. This is a source installer, not a published binary release.

`src/bin/cogfabric-chat.rs` and `src/chat_client.rs` implement bounded interactive
multi-turn text chat through the authenticated loopback HTTP API. Tools are never
advertised or executed; tool proposals are refused. Context, input, output-token
reservation, HTTP response and session time limits apply. Nonblocking input permits
cancellation without a blocking stdin thread; terminal control characters in model
text are escaped. `/reset` clears context without refunding session budgets.
Chat and external agentic clients can use the service in separate processes,
subject to shared admission limits. See [chat guide](CHAT.md).

Shell syntax and formatting checks pass. Rust 1.99 production workspace checks
(`cargo check --workspace --bins --locked`, default and all features) pass.
No installer run, live chat or new test suite has
been executed for this milestone. Release readiness and model quality remain
unproven; see [completion audit](V1_COMPLETION_AUDIT.md).


## GPU sensor and explicit KV-cache placement — 2026-10-24

**Implemented; runtime verification deferred at the operator's request.**
Optional `nvml-gpu` adds a read-only dynamically loaded NVML sensor. Diagnostics
and comparison observations accept `--gpu-sensor nvml`; the default remains
`nvidia-smi`. A fixed same-image child returns a bounded, versioned FlatBuffers
frame (`protocol/gpu.fbs`). Both sensor children have time/output/device limits
and tracked cancellation cleanup. Missing metrics stay unknown; observations are
whole-device and do not establish task-attributed energy or direct CUDA inference.

Explicit `offload_kv_cache_to_gpu` is supported in approved residency loads,
plans and native lifecycle policies for LLM/decision models. The provider must
echo and subsequently report the requested boolean. Explicit settings use v2
receipts/plans, pin KV placement and reject drift; legacy v1 inventory hashes
retain their original field projection. This controls KV cache, not model-layer
placement, GPU isolation or physical resource enforcement. See
[GPU observations](GPU_OBSERVATIONS.md) and [native lifecycle](NATIVE_LIFECYCLE.md).
No new test, live GPU query, benchmark or readiness claim accompanies this work.


## Explicit immutable project profiles — 2026-10-24

**Implemented and verified.** Architecture §3 now has explicit
operator-declared project goals and typed presentation preferences, selected only
by exact `id@version`. `src/projects.rs` defines bounded strict manifests,
checksums and context; `src/store.rs` migrates SQLite to schema 9 and provides
immutable versioned publication/read/list APIs with fixed quotas. Management
permissions remain human-only. No preference inference or automatic learning is
claimed.

`project-publish`, `project-show`, `project-list` and `ask --project` expose the
feature. The optional FlatBuffers AskRequest field is propagated through the UDS
service. `src/engine.rs` records the exact selected profile before inference,
preserves default prompts when absent and conveys compatible presentation
requests without changing the original query or verification criteria.
`src/recovery/continuation.rs` validates v2 execution options and restores the
original bounded snapshot independently of later registry versions. Legacy tasks
retain their original continuation rules.

New store, wire, engine, recovery and CLI/socket regressions cover bounds,
immutability, quotas, schema migration, tampering, explicit selection, unchanged
deterministic answers, original prompt caps and exact snapshot continuation.
See [project profiles](PROJECT_PROFILES.md). Nineteen focused unit tests and four
CLI/socket integration tests pass, including exact recovery and prompt budgets.
Final serialized suites pass **1,048 default tests** and **1,057 all-feature
tests**, each across 72 targets, zero failed or ignored. Formatting, Rust 1.99
workspace check and strict default/all-feature Clippy pass. The suites completed
before the operator requested stopping further tests. Installed smoke scripts were
not rerun for this change; CLI/socket workflows are covered by the completed
integration suites;
no live-model adherence or general preference learning is proven.

## Exact Wasm registry reconstruction — 2026-10-24

**Implemented and verified.**
Architecture §6/7/9/10:
`recovery::continuation::reconstruct_wasm_registry` restores only operator-supplied
compiled guests matching one strict v4 snapshot. It preserves the original broker
UUID, capabilities, implementation hashes and queue/concurrency limits. The CLI
accepts explicit `resume-task --human-approve --restore-wasm-expert` manifests,
compiled before state initialization. No stored pathname supplies code. Exclusive
lease, bounded transaction/history checks and atomic audit refusal preserve the
existing unknown-dispatch and continuation gates. Native scopes, mixed registries
and earlier snapshots without original limits are refused by this reconstruction
route; retained legacy continuation remains supported.

New integration regressions cover reopen reconstruction with nondefault limits,
no approval, substituted identities, wrong versions/capabilities, old snapshots,
invalid/missing/unknown limit fields, duplicate snapshots, native scope, terminal
tasks, extra guests, uncertain history and audit rollback. CLI approval refusal
is checked before state initialization. See [Wasm recovery](WASM_EXPERTS.md).

Final serialized suites pass **1,032 default tests** and **1,041 all-feature
tests**, each across 71 targets, zero failed or ignored. Rust 1.99 workspace
check and strict default/all-feature Clippy, formatting, diff checks and final
offline CLI/socket/authenticated HTTP smoke checks pass. An earlier source-edit
fingerprint failure and cross-feature shared-executable test conflict were
resolved by frozen-source, serialized full reruns. Live model inference remains
unperformed; the local endpoint probe refused connection. This entry supersedes
the guest reconstruction limitation in the earlier milestone below.

## Optional bounded untrusted Wasm expert adapter — 2026-10-24

**Implemented adapter; broader native isolation remains Partial.** Architecture
§6/7/9/10: `src/fabric/modules/wasm.rs` provides immutable hash-pinned no-import
Wasmi guests, typed payloads, host authorization, bounded execution and retained
permits on cancellation. `protocol/general_module.fbs` appends CGWI/CGWO payload
roots without changing existing CGMD packets. `src/main.rs` loads operator
manifests before state initialization; the optional feature is off by default.
`src/engine.rs`, `src/fabric/modules.rs`, `src/recovery/continuation_modules.rs`
and `src/registry/providers.rs` retain implementation identity in v3 snapshots,
refusing substitution and preserving legacy snapshot rules.

`tests/wasm_experts.rs` covers real CLI/engine dispatch, independent acceptance,
artifact pinning, mutation, hostile imports/start/loops/memory/output, manifests
and disabled-feature startup. Protocol tests cover aliased SQL aggregate bounds;
recovery tests reject guest identities on builtin reconstruction. The full all-feature suite passes **1,039 tests across 71 targets**, zero failed
or ignored. Rust 1.99 check/all-feature strict Clippy, formatting, exact generator
and CLI/HTTP smoke checks pass. The repeated default suite passes **1,031 tests
across 71 targets**, zero failed or ignored; strict default Clippy also passes. See [ABI and limits](WASM_EXPERTS.md).
No general native executable sandbox, hard interpreter wall-clock preemption,
automatic guest reconstruction or scientific reasoning-quality claim is made.

## Prepared candidate provenance transaction — 2026-10-24

**Implemented for version-2 RAG bundles.** `src/evolution/prepared.rs` imports
operator-pinned preparation reports with fixed-entry descriptor-relative reads,
artifact/held-out hashes, exact normalized byte framing, source metadata, duplicate
lineage and split checks. It imports a validated byte snapshot rather than
reopening a mutable artifact pathname. `preparation.rs` now emits byte lengths and
the held-out snapshot hash in explicitly versioned reports.

`src/store.rs` atomically records the inactive candidate and non-evaluation
provenance anchor; audit failures roll back versions/candidate/task rows. Existing
quarantine quotas remain in force. `src/storage.rs` protects provenance anchors.
`candidate create-prepared` imports, and `candidate provenance` opens the existing
store read-only to inspect metadata. No quality evaluation, permissions or neural
promotion are inferred. Version-1 bundles require re-preparation for this route.

`tests/prepared_candidate.rs` covers actual CLI import/inspection, held-out
exclusion, embedded headings, retention, unchanged promotion gates, fifteen
tampered bundle scenarios and atomic audit-failure rollback. Combined checks pass
**1,030 tests across 70 workspace targets**, zero failed or ignored, plus **four
optional Wasm tests**. Formatting, Rust 1.99 workspace check and strict Clippy with
default/all features pass. Offline CLI/HTTP smoke scripts pass.

## Bounded benchmark GPU observation windows — 2026-10-23

**Implemented device observations; task-attributed physical accounting remains
Partial.** `src/gpu/window.rs` observes execution futures using one irreversible
run-wide query budget, bounded cadence/device counts, sampled maxima and partial
trapezoidal device-power intervals. Missing samples/power, large gaps and device
identity changes cannot fabricate complete energy. In-flight query cleanup is
drained and timed; late samples are excluded. No sampler task is detached.

`src/benchmarking.rs` and `compare --observe-gpu` explicitly enable per-case/arm
observations while preserving default output and original budgets. Existing
fixed-command GPU query limits and reaping remain in force. Five deterministic
window tests plus `tests/gpu_observation.rs` cover interpolation, missing data,
identity changes, shared budgets, cleanup, invalid settings and optional CLI
reports. Combined checks pass **1,027 tests across 69 workspace targets**, zero
failed or ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99
workspace check, strict default/all-feature Clippy and both offline CLI/HTTP smoke
scripts pass. See
[GPU observations](GPU_OBSERVATIONS.md). No task energy, true peak, physical
isolation or efficiency gain is claimed.

## Bounded local source discovery — 2026-10-23

**Implemented local inventory; remote discovery/independent curation remain
Partial.** `src/evolution/discovery.rs` walks an operator-selected root through
retained directory descriptors and Linux procfs, with no-follow/nonblocking
`openat`, strict count/depth/path/byte/time bounds and no successful truncation.
Visible local text documents get sorted stable identities, original hashes and
operator-declared provenance/license metadata. Review remains false and all splits
remain reference-only drafts. No model permission or source-quality claim follows.

`candidate discover-sources` runs before runtime initialization and writes only a
new private draft bundle. `preparation.rs` shares its no-overwrite bundle writer.
`tests/source_discovery.rs` checks deterministic CLI output, unused-home/privacy,
mandatory review, subsequent preparation with held-out exclusion, no overwrite,
and nine unsafe/empty/exhausted inventory scenarios. Combined checks pass **1,021
tests across 68 workspace targets**, zero failed or ignored, plus **four optional
Wasm tests**. Formatting, Rust 1.99 workspace check, strict default/all-feature
Clippy and both offline CLI/HTTP smoke scripts pass. See
[source preparation](SOURCE_PREPARATION.md).

## Bounded reviewed-source preparation — 2026-10-23

**Implemented local preparation; automatic discovery and independent curation
remain Partial.** `src/evolution/preparation.rs` prepares operator-selected,
hash-pinned local source manifests with explicit review/provenance/license labels,
strict bounded parsing, directory-relative no-follow reads, deterministic text
normalization, document deduplication and exact cross-split overlap refusal.
Reference artifacts exclude held-out text; reports retain source/split/hash lineage.
`candidate prepare-sources` runs before runtime/database initialization and writes
only a new private bundle. It creates/promotes no candidate or neural model.

`tests/source_preparation.rs` covers CLI output/privacy, unused-home preservation,
deduplication, held-out exclusion, no overwrite, changed input/manifest hashes and
sixteen review/path/symlink/FIFO/control/UTF-8/size/split refusal scenarios. Combined
checks pass **1,019 tests across 67 workspace targets**, zero failed or ignored,
plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace check and strict
Clippy with default and all features pass. See
[source preparation](SOURCE_PREPARATION.md).

## Completion audit and generation scan deadline — 2026-10-23

**Overall Partial.** [V1_COMPLETION_AUDIT.md](V1_COMPLETION_AUDIT.md) reconciles the
full canonical architecture with inspected source/tests, separating implemented
runtime mechanisms from unproven live quality, physical resource measurements,
untrusted executable isolation, automatic source preparation and neural research.
Historical rows below must not be used alone as the current completion decision.

**Implemented:** `src/residency/lifecycle.rs` now bounds the native generation gate
with a five-second SQLite interrupt deadline and per-record elapsed checks, in
addition to existing row/payload bounds. All result paths clear the handler. Two
new regressions verify expiry does not permit clearance, later SQL still works,
and incomplete/duplicate generation evidence remains refused. Combined quality
checks pass **1,016 workspace tests across 66 targets**, zero failed or ignored,
plus **four optional Wasm tests**. Formatting, Rust 1.99 check and strict Clippy
with default/all features, exact ten-schema generator output and both installed
CLI/HTTP smoke scripts pass. No schema, index, model permission or provider mutation
is added. This is a cooperative SQLite bound, not hard process/I/O isolation.

## Explicit native-owner continuation — 2026-10-23

**Implemented for supported original checkpoints and reviewed runtime objects.**
`resume_native_with_runtime` and `resume-task --native-policy` retain the validated
original owner, native receipt/role targets and same-boot deadline through normal
continuation admission. The ordinary native guard still refuses missing authority;
database metadata cannot construct the private restoration object.

- `src/residency/lifecycle/restoration.rs`: sealed owner context, revalidation,
  exact BoundBackend restoration and per-dispatch durable generation anchors.
- `src/recovery.rs`, `continuation.rs`: explicit owned admission, preserved
  resource configuration, original charges/routes and cancellation during setup;
  no OpenAI-compatible fallback for a native task.
- `src/fabric/modules.rs`, `continuation_modules.rs`: source-checked builtin
  reconstruction under the same owner without reacquiring conflicting leases.
- `src/residency/lifecycle.rs`: existing operator generation resolution supports
  continuation lifecycle anchors, preserving unknown original outcomes.
- `tests/native_lifecycle.rs`: fresh CLI resume with a real typed worker and
  configured resource pool, no model reload/unload, exactly one new inference;
  wrong-instance refusal, durable blocking, explicit resolution and no replay.

Combined verification passes **1,014 tests across 66 workspace targets**, zero
failed or ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace
check and strict Clippy with default and all features pass. The ten focused native
tests include both successful and uncertain continuation. Unknown
or incomplete original calls, unsupported checkpoints, custom callback loading,
physical instance/weight attestation and real-model quality remain unproven or
refused. No automatic provider mutation or neural promotion is introduced.

## Native restoration preflight — 2026-10-23

**Partial: read-only receipt preflight implemented; native continuation remains
unsupported.** `src/residency/lifecycle/restoration.rs` validates one unfinished
task's original native scope, exact policy hash, configuration/role model set,
same-boot remaining clock, unique bounded lifecycle bindings, matching completed
generation admission, and fresh complete inventory against every original receipt.
Exclusive runtime/resource ownership spans inspection; original-record reads have
SQLite deadlines and the GET retains the original task's remaining allowance.
Original configuration, clock and bindings are revalidated after observation.

`native-resume-preview TASK_UUID --policy POLICY.json` uses a read-only store and
reports receipt identity separately from checkpoint eligibility. It cannot load,
release, adopt, rebase, replay, acknowledge provider stopping, admit a checkpoint,
or grant permissions. `tests/native_lifecycle.rs` verifies library/CLI operation,
unchanged child and parent histories, no chat/unload, and early refusal of policy,
clock, role, generation endpoint/phase and binding changes. Combined verification
passes **1,012 tests across 66 workspace targets**, zero failed or ignored, plus
**four optional Wasm tests**. Formatting, Rust 1.99 workspace check and strict
Clippy with default and all features pass.
Next implementation step: retain this validated owner and
exact backend through independently validated continuation and durable admission.

## Builtin registry reconstruction — 2026-10-23

**Implemented for the fixed compiled worker; native-owner recovery remains
Partial.** Version-2 typed snapshots bind optional source fingerprints to captured
pins. Retained registries remain compatible with version 1. The approved
`reconstruct_builtin_registry` API and `resume-task --restore-builtin-worker`
restore only the exact compiled builtin descriptor under matching source inputs.
Neither persisted executable paths nor arbitrary callbacks are loaded.

- `src/fabric/modules.rs`, `process.rs`, `src/engine.rs`: captured bounded source
  declarations, immutable task bindings and version-2 snapshots.
- `src/recovery/continuation_modules.rs`, `continuation.rs`: exclusive ownership,
  bounded transactional source/history validation and required operator audit;
  ordinary continuation gates remain independent.
- `src/main.rs`, `src/registry/providers.rs`: explicit CLI opt-in, combined policy
  restoration and provider source identity checks.
- Unit regressions cover reopened stores plus twelve refusal/rollback scenarios;
  `tests/typed_modules.rs` covers actual prior child history without replay and
  fresh CLI continuation with a real typed worker after the original host exits.

Combined verification passes **1,011 tests across 66 workspace targets**, with
zero failures or ignored tests, plus **four optional Wasm tests**. Formatting,
Rust 1.99 workspace check and strict Clippy with default and all features pass. Focused process tests pass (five tests).
Matching sources are not executable/compiler attestation, and arbitrary host
callback deserialization remains intentionally unsupported. Real-model quality
and performance are still unverified.

## Native task worker ownership — 2026-10-23

**Implemented and regression-tested.** Native lifecycle setup forks
the template's active typed catalog before provider mutations. Task-binding
callbacks run outside registry locks and must preserve immutable descriptors.
Each task receives a fresh Broker identity without mutating the service template.
Automatic builtin installation rejects conflicting preinstalled modules before
model loading. Standalone native tasks now record the same policy digest as
service tasks.

`ProcessModule` holds only a weak reference while idle, rejects expired ownership
before launch, and transfers a strong owner to each live child through reaping.
This fixes retained exclusive leases and duplicate builtin registration across
service requests. Files: `src/engine.rs`, `src/fabric/modules.rs`,
`src/fabric/modules/process.rs`, `src/residency/lifecycle.rs`.
The seven native lifecycle integration tests pass, including three sequential
service tasks with distinct registry identities and a reaped arithmetic worker.
An additional regression checks that an expired adapter cannot borrow a newly
acquired owner. This does not implement native-owner continuation after restart,
attest provider weights, or change task capabilities or process isolation.

Combined verification: **1,007 workspace tests across 66 targets**, zero failed
or ignored, plus **four optional Wasm tests**. Formatting, Rust 1.99 workspace
check and strict Clippy with default and all features pass.

## Explicit retained-registry continuation — 2026-10-23

**Implemented for retained host registries; cross-process reconstruction remains
Partial.** `RuntimeRestoration` combines original typed registry, escalation and
verifier objects. New `preview_with_runtime` / `resume_with_runtime` APIs validate
the original Broker UUID and Store identity, exact immutable descriptors and
available retained versions, independently of the active version. Default APIs
continue to refuse missing registry restoration.

- `src/fabric/modules.rs`: private exact retained-version pins and Store identity
  checks; snapshot metadata cannot construct code or task grants.
- `src/recovery/continuation_modules.rs`: bounded snapshot parsing, immutable pin
  reconstruction from the retained registry, and hashed restoration evidence.
- `src/recovery/continuation_module_history.rs`: bounded chronological checks of
  reservations, unique intents/returned outcomes, process dispatch links and
  completely reaped ownership. Unknown/failed/forged/duplicate histories refuse.
- `src/recovery/continuation.rs`: read-only preview, revalidated transactional
  admission, combined policy restoration, retained pins in Session and conversion
  to shared execution ownership before process-backed module dispatch.
- `src/recovery/continuation_module_tests.rs`: active-v2/original-v1 restoration,
  original model/tool charge preservation, two-call loopback resume without old
  callback replay, approval/write denials, foreign Store/Broker and retired-version
  refusal, tampered dispatch history and synthetic process-audit completeness.

The full workspace suite passes **1,005 tests across 66 targets**, with no ignored
or failed tests; all four optional Wasm tests pass too. Format/check and Rust 1.99
strict Clippy in default and all-feature configurations pass. The prior
snapshot `82a88ac` also passed GitHub CI; that result does not verify new source.
No new real-model, GPU, binary-attestation or neural-safety claim is made.

## Integrated runtime verification — 2026-10-23

**Implemented and tested runtime fixes; overall scope remains Partial.**

- `src/reasoning/modules.rs`, `src/reasoning/verification.rs`: malformed model
  candidate JSON is rejected rather than aborting verification; typed diagnostics
  remain advisory. `tests/typed_module_reasoning.rs` covers sorting, ordering,
  shortest path, lying diagnostics, invalid JSON and opt-in finite assignments,
  including exactly one original tool reservation per check/search.
- `src/main.rs`: keep Tokio alive through bounded agent/collective worker drains
  on both successful and failed CLI exits. `tests/typed_modules.rs` requires a
  cancelled task status, exact task identity, reaped-child audit and absent PID.
- `src/fabric/modules.rs`: dropping the Broker signals idle/active queue shutdown
  instead of retaining the Registered/sender lifetime cycle. A weak-reference
  regression proves initialized module implementations are released.
- `src/recovery/continuation_modules.rs`: bounded, read-only validation refuses
  missing/forged/duplicate module evidence and nonempty task pins when no original
  registry is restored. Empty/legacy snapshots retain ordinary continuation.
  **Partial:** an explicit original-registry restoration API is still to build.
- `src/registry/provider_tests.rs`, native lifecycle and protocol bundle tests:
  enumerate every corruption case, reject wrong Broker identity, reference real
  historical context and validate all ten schemas.

The frozen source snapshot passes 1,000 workspace tests across 66 targets plus
four optional Wasm tests, format/check gates and Rust 1.99 all-feature strict
Clippy. Both offline smoke scripts pass. See [current implementation](CURRENT_IMPLEMENTATION.md)
for exact commands and limits. The local inference server was unavailable; no new
live-model quality, physical GPU enforcement or neural safety evidence is claimed.

## Rust 1.99 CI follow-up — 2026-10-23

**Implemented; workspace lint gate verified on Rust 1.99.0.** The failed GitHub
run for `fa1b354` identified a remaining deprecated `fetch_update` in
`tests/working_profiles.rs`. It now uses a sequentially consistent atomic
compare/exchange loop, preserving the zero floor and one blocked dispatch per
reservation. The same deprecated API in the separate Wasm expert experiment is
replaced without changing revocation transitions or their atomic ordering.
A repository-wide Rust source scan finds no remaining `fetch_update` calls.
`cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings` passes.
All nine `working_profiles` tests and all 23 separate Wasm experiment tests pass.
The experiment also passes strict Clippy on Rust 1.99.0. Both formatting gates pass.
This supersedes reliance on the older local compiler for the CI lint gate;
full integration readiness is still unproven.

## CI lint compatibility correction — 2026-10-23

**Implemented.** The reported strict-Clippy failures are corrected without
blanket lint suppression. Host execution parameters are grouped into
`engine::TaskExecution` and `residency::lifecycle::LifecycleRuntime`, preserving
original task identity, policy options, deadlines and cancellation. Module grants
reserve calls with an atomic compare/exchange loop instead of the deprecated
`fetch_update`; limits and memory ordering remain unchanged. FlatBuffers packet
initialization, copied UUIDs, optional queue checks, recovery comparisons, hex
parsing, CLI closures and regression fixture types are simplified.

`cargo clippy --locked --workspace --all-targets -- -D warnings` and the
corresponding `--all-features` command pass locally with Rust 1.97.1. This corrects the supplied CI diagnostics, including newer
compiler deprecation/chunk lints. Final runtime verification of the broader
integration remains separate; this lint correction does not mark v1.0 ready.

## Optional Wasm dispatch stack safety — 2026-10-23

**Implemented and targeted regression-tested.** `Cargo.toml` enables Wasmi 2.0.0
`portable-dispatch`. With default features disabled, the previous opcode dispatch
path exhausted the native worker stack on the infinite-loop fuel regression before
fuel could trap. Portable dispatch preserves the existing fuel, memory, concurrency
and recursion limits without increasing the host stack or suppressing the test.
`cargo test --locked --features wasm-verifier --test wasm_verifier` passes all four
tests, including fuel exhaustion and compiled-criterion authority. This resolves
the previously recorded optional Wasm stack overflow; full integration verification
is still pending.

## Implementation phase — task lifecycle and replaceable modules

**Partial overall; integrated paths verified by the newest entry above.**
Historical passing counts below describe their own snapshots. Cross-process registry
reconstruction, lifecycle-owner restoration and real-model measurements remain incomplete.

- `src/residency/lifecycle.rs`, `src/engine.rs`, `src/reasoning.rs`: operator-approved
  task model sets, retained exclusive runtime/resource ownership, receipt-bound
  generation preflight, model-specific expert adapters, original task budgets,
  conditional release of only newly loaded instances, and durable unknown-inference
  gates with explicit stop-acknowledgement resolution. Never adopt foreign instances.
- `src/main.rs`, `crates/cogfabric-server/src/main.rs`: explicit reviewed lifecycle
  policy and approval options for ordinary collective tasks, with policy validation
  before state initialization; HTTP same-image typed worker entrypoint. See
  [native lifecycle](NATIVE_LIFECYCLE.md).
- `src/backend/native.rs`, `src/backend.rs`: native LM Studio role adapters with
  response-instance checks, no provider integrations, bounded strict responses and
  sticky uncertain-dispatch counters. Requests select documented model keys; the
  returned instance is checked, without claiming request-time instance selection.
- `src/resource_scheduler/`, `src/evaluation/model_metadata.rs`, recovery resource
  validators: preserve explicit native adapter/instance identities and declared
  resource mappings; native/OpenAI protocol identities are not interchangeable.
- `src/registry.rs`: whole-graph expert version activation planning and atomic apply,
  immutable candidate/current-set hashes, dependency closure and stale-plan refusal.
- `src/fabric/modules.rs`: typed trusted-host module registration, version pins,
  task grants, bounded queues/concurrency/deadlines, cancellation and audits.
  `src/fabric/modules/{protocol,process}.rs` and `protocol/general_module.fbs`
  add bounded authenticated same-image process workers using generated FlatBuffers
  unions. Engine math/SQL dispatch can use task-pinned general modules; compiled
  checking remains independent. `--typed-module-worker` explicitly enables the
  compiled worker. Host code can register other trusted typed implementations.
- `src/recovery.rs`, continuation and reasoning verification helpers: explicit exact
  host-verifier restoration, preserved charges/deadline and refusal of unfinished
  callbacks. Completed-but-unpublished parallel-wave reconciliation now has source
  admission logic for uniquely charged, fully recorded response sets, without replay.

- `src/verification/wasm.rs`: optional default-off hash-pinned no-import verifier
  ABI with compiled memory/table/stack/recursion/fuel/concurrency ceilings. Secondary
  acceptance never replaces the original compiled criterion; cancellation keeps
  blocking-job permits until execution ends.
- `src/security_evaluation/corpus.rs`: bounded exact-hash manifest evaluation with
  closed deterministic probe families, independent expected labels and explicit
  operator-attested review/provenance. It cannot promote a model or alter gates.

No build, test, real-backend, sandbox, neural-quality or performance readiness claim
is made for this implementation phase. Arbitrary native executable isolation,
unknown-outcome recovery and independent empirical evaluation retain explicit
limitations. New neural architectures remain separate research.


### Track approved native loads before managed release — 2026-10-23

**Implemented and regression-tested; general lifecycle remains Partial.** Architecture sections 4, 6,
9 and 10 require sparse model lifecycle control with independent permissions.
A dedicated managed load/release API and CLI will bind local receipt UUIDs to
approved native requests, exact observed instance identities and full-inventory
digests. These receipts do not establish exclusive provider ownership, immutable
weights or instance-bound inference. Existing explicit manual administration stays
available; its mutations invalidate managed release eligibility before POST.

Schema 7 adds bounded receipt state and retained operation references without
adopting old provider instances. Known managed transitions atomically advance the
active endpoint cohort; uncertain effects retain blocking evidence and cannot be
retried through managed APIs. CLI mutation requires explicit approval and an
existing current-schema workspace; local receipt inspection does not contact the
provider. No model gains residency permissions or automatic cleanup authority.
Sources: `src/residency/managed.rs`, native manual-mutation integration,
`src/main.rs`, schema migration/restore and storage retention. See
[managed residency](MANAGED_RESIDENCY.md). Focused tests cover schema 1–6 migration
without adoption, atomic rollback, receipt integrity, bounded query plans and CLI/
loopback lifecycle behavior. General autonomous lifecycle remains Partial.



Verification on final frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**940 passed across 61 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Eleven CLI/loopback integration tests cover multi-model cohorts, provider-ID reuse
with fresh UUIDs, non-adoption, endpoint/approval/workspace gates, manual-plan
invalidation, before/after-POST audit failures, unknown outcomes, actual client
future cancellation during pending POSTs, resource leases, retained unknown loads
and capacity refusal with releases still allowed. Storage/helper regressions cover
schema 1–6 migration without adoption, atomic failure/data preservation, receipt
integrity, retention, malformed markers, indexed queries and SQL deadline cleanup.

The first full runs found historical migration fixtures requiring v7-object
removal and a general event index causing worker-recovery timeouts. The index is
now partial to native residency events; the unchanged 10,000-session worker gate
passes again. Test compile/lint fixtures were corrected without removing tests.
Malformed managed markers cannot downgrade receipt validation or erase retention
protection. Existing socket/process, mock, malformed-frame, policy-denial,
cancellation and candidate-rollback tests remain green. Canonical French
architecture and protocol schemas/bindings are unchanged. Bundled FlatBuffers
pairing/integrity checks pass; `flatc` is unavailable, so fresh generation was not
run. Port 1234 refused local-model discovery: no real provider load/unload,
inference quality, physical memory or efficiency result is claimed.

Jev+ and Jev-R remain **Partial**. The next verifiable lifecycle milestone is
explicit operator resolution of uncertain operations and reviewed receipt
retirement, followed by task-to-native-instance integration. No model gains
mutation permission and local receipts still do not prove exclusive ownership.

### Measure shortest-path revision plateaus — 2026-10-23

**Implemented and regression-tested; general reasoning remains Partial.** Architecture sections 5 and 9 require
stopping when independently measured improvement becomes marginal. The next slice
extends optional progress stopping to `shortest_path/v1` with a separate versioned
history, preserving the finite-domain policy. Only bounded declared-node arrays
are comparable; malformed answers reset streaks. Compiled shortest-path checking
remains the acceptance authority; a progress score grants no verification.

Fresh enabled tasks bind the selected policy in execution settings and immutable
reasoning context. Recovery must restore that exact policy and history; legacy
unmarked tasks cannot gain scoring midway through continuation. Original budgets,
permission checks, provider-veto handling and exact-recurrence stopping remain
independent. Controlled progress comparison accepts both supported structured
criteria with matching per-case verification labels. Sources: `src/verification/path.rs`, `src/reasoning/progress.rs`, Session/Engine
policy pinning, `src/recovery/continuation_path_progress.rs`, result-proof
deduplication and `src/benchmarking/progress.rs`. Runtime and recovery targets
pass 11 tests; focused helper tests cover score/checker equivalence, bounds and
malformed history. Final workspace results follow below. These are functional
regressions, not measured neural quality, latency or calibrated expected utility.



Verification on final frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**921 passed across 60 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
The 12 new integration tests cover runtime stopping, original-budget HTTP
continuation, repeated charged result rechecks with one proof, legacy behavior,
nine tamper/no-dispatch cases and a two-case HTTP comparison. Helper tests cover
exhaustive small-graph score/checker equivalence, bounds, history transitions,
criterion-label preflight and malformed/duplicate recovery evidence.

Tests exposed and fixed shortest-path result-proof deduplication missing from the
existing finite-only gate. Review also fixed duplicate cycle-zero proofs and
foreign/malformed progress audit acceptance. Initial fixture/compile/lint errors
were corrected; no tests were removed. Existing socket/process, migrations, mock,
malformed-frame, policy-denial, cancellation and rollback coverage remains green.
Protocol schemas/bindings are unchanged; bundled generator/runtime checks pass.
`flatc` remains unavailable, so fresh generation was not run. Localhost port 1234
refused discovery. No real-model quality, GPU, latency or energy gain is asserted.
Jev+ and Jev-R remain **Partial**: automatic ownership-aware provider lifecycle,
broader isolated executable verification and independent live-model effectiveness
measurements remain open. The next lifecycle step requires explicit provider
instance identity and durable ownership, without granting models mutation rights.

### Select eligible inference routes before pinning — 2026-10-22

**Implemented and regression-tested for declared eligibility. General Jev+/Jev-R remains Partial.**
With declared working profiles or residents, the Broker filters each supported
role against the immutable resource catalog before choosing by existing rank,
load and identity ordering. Ineligible preferred candidates can yield to an
eligible peer. Selection freezes the exact backend, module version and role map;
ordinary shared admission still controls actual capacity. Empty catalogs retain
the legacy path. Empty eligible maps permit deterministic tasks and deny an
actually requested inference role without provider dispatch.

Required intent precedes metadata callbacks; bounded selection and fixed exclusion
codes precede generation. Callbacks execute outside registry locks and publication
revalidates registry identities. Route scope binds policy evidence to the selected
role/module/target/profile map, excluding ephemeral pool identities and load
observations. Original budgets, permissions and unknown-outcome quarantine remain
unchanged. This is declared eligibility, not measured utility or physical residency.

Versioned recovery validates saved routing records and restores the exact supported
builtin role subset and route scope. Unmarked legacy histories retain their prior
rules; arbitrary custom modules remain unsupported for continuation. Sources:
`src/fabric.rs`, `src/engine.rs`, `src/resource_scheduler.rs`,
`src/reasoning/verification.rs`, `src/recovery/resource_routes.rs`.
Benchmark/evaluation details expose the stable route scope and bounded selected
module/role/profile identities with hashed targets, omitting endpoints and exclusion
errors. Empty-map recovery restores a deny-all backend only with no prior model
dispatch; it cannot acquire new inference roles. Regression coverage: `tests/resource_route_selection.rs` (12 tests),
`tests/resource_route_resume.rs` (3 tests), four empty-route recovery unit tests
and four route-report metadata unit tests. Existing `tests/working_profiles.rs`
retains direct admission audit-before-metadata and attempted-call accounting checks.
The queue regression observes an actual waiting parallel call before registry
replacement and verifies the original routes finish without provider fallback.
Automatic ownership-aware loading/unloading, broader isolated module lifecycle and
independent real-model quality/cost evaluation remain incomplete.



Verification on frozen sources: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**899 passed across 57 targets**,
zero failures or ignored tests), and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Initial new-test compilation/fixture errors and lint findings were corrected;
the queue test now uses actual same-task parallel work because the Store execution
lease prevents simultaneous task runs. Existing socket/process, migrations, mock
inference, malformed-frame, policy-denial, cancellation and rollback tests remain
green. Schemas/bindings are unchanged; bundled FlatBuffers pairing/integrity checks
pass. `flatc` is unavailable, so fresh generation was not performed. Localhost
port 1234 refused discovery; no real-model/GPU quality or speed claim is made.

### Admit heterogeneous model working sets under one resource pool — 2026-10-22

**Implemented (configured admission); Partial (general module lifecycle).**
Architecture sections 4, 6 and 9 require resource-aware coordination of replaceable
models. Optional
`resource_admission.working_profiles` binds bounded operator-declared allocations
to canonical local model targets and roles. An active table requires an exact
match; no unknown/unlisted/deterministic target may use the uniform fallback.
Absent/empty tables preserve legacy behavior and configuration bytes.

The runtime pins a backend before resolving its target and dispatches through
that same pin after selecting the allocation. Required profile intent/decision
audits bind the canonical catalog digest, role, target, profile and allocation.
Existing resident footprints, shared queue/capacity, task budgets, cancellation
and unknown-outcome quarantine remain authoritative. No model grants, automatic
provider load/unload or physical memory attestation are introduced.

Resource recovery now validates bounded per-reservation histories instead of a
single serial chain, allowing complete overlapping advisory waves. Every chain
must match an earlier charged same-role dispatch, original profile/target/
allocation and complete ordered admission/outcome audits. Saved configuration
retains authority; current config-file changes cannot replace it, and incompatible
live pools are refused.

Sources: `src/resource_scheduler/profiles.rs`, pool/adapter/admission integration,
configured Session/parallel wrappers, native ChatBackend snapshots and
`src/recovery/continuation_resources.rs`. Tests: `tests/working_profiles.rs`,
`tests/agent_working_profiles.rs`, `tests/working_profile_resume.rs`. See
[working profiles](RESOURCE_SCHEDULING.md#target-specific-working-profiles).
This addresses heterogeneous logical admission; broker eligibility/fallback,
automatic ownership-aware residency, untrusted executable isolation and independent
physical/quality measurements remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification after implementation: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**876 passed across 55 targets**,
none failed/ignored) and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass on the
final frozen sources. Nine admission/configuration tests cover strict bounds,
empty-table bytes, CLI roundtrip, per-role targets, real shared-pool pressure,
unknown/unlisted/deterministic targets, metadata-error redaction, audit failure,
exact quarantine and registry replacement while queued. Three native-turn tests
cover refusal without snapshot support, actual LocalChat HTTP dispatch and a
queued snapshot surviving a route change while new turns select the new route.

Three recovery tests validate profiled and legacy parallel histories, unchanged
original settings despite current config-file changes, exact retained calls/
tokens/cycles/allocations, nine altered audit-chain cases with no writes/HTTP, and
actual overlapping pending HTTP calls. The original first wave uses a bounded
barrier: both resource dispatches must precede either completion in the retained
checkpoint. No planner or completed subtask is replayed. Source review found and
fixed the native chat snapshot gap; initial build/test fixture errors (new struct
field, private Store access and moved values) were corrected without removing
tests or weakening resource/identity assertions.

Existing socket, database/migration, mock, malformed-frame, permission-denial,
cancellation, verifier, candidate-rollback and current-source-fingerprint checks
remain green. Protocol schemas/bindings are unchanged and bundled pairing/integrity
checks pass. `flatc` is absent, so fresh generation was not run. Local port 1234
refused connections; real-model/GPU performance was not measured. These fixtures
prove logical admission/reconciliation behavior, not physical memory use or a
quality/speedup advantage. Final logs: `/tmp/jev-working-profiles-{check,tests,clippy}-final.log`.


### Pin an optional trusted host verifier to structured reasoning — 2026-10-22

**Implemented (native extension); Partial (general executable verification).**
Architecture sections 4–6 and 9–10 require replaceable roles with independent
evidence and fixed permissions. Native hosts can select a `VerifierProvider` with
`Engine::with_verifier_provider`; sessions and Engine clones retain the captured
descriptor and callback instance. The provider receives bounded original objective/
candidate data and the original deadline. Its claim and the original compiled
checker must both pass. This covers the four existing structured criteria, solver
answers, direct search witnesses and matching published-memory reuse; ordinary
prose and generation-only comparisons gain no checker authority.

Existing tool authorization/reservation remains outside the provider. Required
snapshot, intent and outcome audits bind the identity, criterion and both hashes,
with separate provider elapsed wall time. Invalid claims/errors cannot fall back
to unchecked success. Cancellation drops the owned future without refunds.
Effective workflow/escalation/advice fingerprints include the captured descriptor,
preventing built-in-only evidence from silently matching a changed acceptance rule.
Recovery refuses custom-provider histories until their implementation can be
restored, rather than silently dropping the selected acceptance gate.

Sources: `src/verification/provider.rs`, Engine/Session integration, advice scope
construction and recovery admission guards. See
[host verifier contract](VERIFICATION.md#trusted-host-verifier-extension).
The digest is operator-declared, not binary attestation. Cooperative async timeouts
cannot isolate blocking native code. Arbitrary untrusted executable loading, new
criterion families, provider restoration and independent quality/physical-cost
measurements remain incomplete; Jev+ and Jev-R remain **Partial**.

Verification after implementation: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**861 passed across 52 targets**,
zero failed/ignored) and
`cargo clippy --locked --workspace --all-targets -- -D warnings` all pass.
Sixteen provider integration tests cover actual dispatch, exact retained budgets,
false acceptance, vetoes, errors/invalid claims, original-deadline exhaustion,
cancellation/drop, every required audit gate, captured descriptor/decision/history
scope, memory rechecks, direct-search fallback and recovery refusals. Two new
progress unit regressions preserve the original score-zero invariant and validate
veto cycle/scope/plateau behavior. Three saved-parallel-advice crash-window tests
also pass in this full run; the existing reconciliation implementation needed no
production change for that window.

Source review found and fixed unbounded JSON inspection in the new recovery guard.
Testing found and fixed a provider-veto/finite-progress composition issue before
the final gates. Test fixtures were corrected for their actual planner/solver
prefix, original wall ceiling, interrupted-intent semantics and retained search/
fallback checks; acceptance, accounting and no-dispatch assertions remain.
Default socket, persistence/migration, malformed-frame, permission denial,
cancellation, rollback and current-source-fingerprint regressions remain green.
Schemas/bindings are unchanged and bundled generator/runtime checks pass; `flatc`
is unavailable, so fresh generation was not run. Local discovery at port 1234
refused connections; no real-model/GPU, general quality or speedup result is claimed.
Final logs: `/tmp/jev-verifier-{check,tests,clippy}-final.log`.


### Continue completed parallel advisory waves without replay — 2026-10-22

**Implemented (completed-wave checkpoints); Partial (general in-flight recovery).**
Architecture sections 4, 5, 7 and 9 require persistent task progression under
original budgets. Operator-approved continuation now validates the original full
parallel schedule and a prefix of entirely recorded advisory waves. Every completed
node must match a unique charged cycle-zero solver result, original graph/order,
text hash and bounded result, with the original reservation/admission/publication
chronology. Later waves must remain pending. Running, unresolved, partial or
conflicting work cannot create a new inference allowance.

Resumed execution skips completed waves, retains the full original schedule and
its single marker, and keeps the original configured broker concurrency. It
preserves the original model/context/settings, calls, outputs, prompt bytes, tools,
cycles, deadline and shared-resource admission. A ready final-solver boundary can
reuse all recorded advice without rerunning planner or subtasks. Independent
whole-goal checking retains success authority; advice and history metadata cannot
grant permissions or establish a verified answer. No weights or capabilities are
promoted. General incomplete-wave reconciliation and independent live utility/
physical-cost evidence remain incomplete; Jev+ and Jev-R remain Partial.

Sources: `src/recovery/continuation.rs`,
`src/recovery/continuation_parallel.rs`, validated graph schedule access and
`src/reasoning.rs` and `src/reasoning/parallel.rs`. See
[parallel recovery](RECOVERY.md#completed-parallel-advisory-waves).

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**840 passed across 50 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Five HTTP integration tests cover a completed
non-scheduler-prefix wave, actual overlapping pending calls, original criterion
checking, exact final-solver boundary, pre-marker initialization, nine altered
history/state/clock/budget cases with no writes or HTTP dispatch, and admission
audit rollback. The completed first wave retains three prior calls; three new
calls finish with six total calls and 1408 reserved output tokens, matching the
uninterrupted authored fixture. This is replay/accounting evidence, not a live-
model efficiency or quality comparison. Nine helper tests include two new executor
regressions rejecting invalid saved ceilings/partitions or unmet dependencies
before any reservation. Independent source review found no blocking issue.

Initial borrow-checker errors were fixed; integration fixtures were corrected to
use a pending cancellation future and the retained ledger counts after rewind.
Assertions for actual overlap, budgets and independent satisfaction remain intact.
Existing socket, migration, mock inference, malformed-frame, policy-denial,
cancellation, shared-resource, recovery and rollback coverage remains green.
Protocol schemas/bindings are unchanged; bundled generator/runtime pairing and
integrity checks pass. `flatc` is absent, so fresh generation was not run. Approved
localhost discovery at port 1234 refused connections; no real-model/GPU measurement
or general reasoning-quality claim follows. Incomplete/unknown waves and broader
module recovery remain separate work.

### Execute dependency-ready advisory subtasks in bounded parallel waves — 2026-10-22

**Implemented (opt-in bounded concurrency); Partial (general module scheduling).**
Architecture sections 4, 5 and 9 require coordinated multi-role work under fixed
budgets. Optional `subtask_parallelism` (1–4, absent by default) groups the pinned
four-node planner graph into deterministic dependency-ready waves. Independent
subtasks can overlap; dependent nodes wait for prior-wave results. Existing
committed scheduler order determines wave selection and result reconciliation.
Each wave authorizes and durably reserves individual calls before backend polling,
retaining the final solver allowance and original deadlines. Shared resource-pool
admission remains authoritative. Local future ownership preserves cancellation;
unknown outcomes retain charges and do not establish provider stopping.

A required metadata-only parallel schedule binds the graph/order/ceiling/waves
with no permission or verification authority. Graph observation validates overlap
against that partition; default sequential auditing remains unchanged. Parallel
checkpoint continuation is explicitly refused pending separate reconciliation.
Sources: `src/config.rs`, Engine construction, `src/fabric.rs`,
`src/reasoning.rs`, `src/reasoning/parallel.rs`,
`src/recovery/{graph,graph_parallel,continuation}.rs` and integration tests.
See [scheduling](SCHEDULING.md#opt-in-parallel-advisory-execution).
No new external capability, active weight change or physical GPU allocation is
introduced. General module scheduling, wave recovery and independent usefulness/
efficiency measurements remain incomplete; Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**833 passed across 49 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Seven helper tests cover configuration compatibility,
wave selection/caps, malformed orders and local future cleanup; three broker tests
cover concurrent cap enforcement, queueing, shutdown/drain and invalid caps.
Six Engine integration tests prove actual backend overlap, reverse completion with
dependency gating, deterministic result order, exact reservations/final-solver
reserves, cancellation/unknown usage, serial compatibility, insufficient budgets
and both schedule/partial-wave reservation audit failure. Four graph tests cover
bounded observation, 14 marker alterations, premature results/wave crossing and
read-only continuation refusal. Existing socket, migration, mock inference,
malformed-frame, policy-denial, recovery and candidate-rollback coverage remains green.

The first overlap test exposed the legacy broker serialization; the implementation
was extended rather than weakening that test. Broker panic behavior was restored
after the existing health regression caught JoinSet isolation changing it. Lint
findings were fixed without suppression. An intermediate full run detected source
edits during execution through the existing fingerprint check; the final frozen
rebuild and full rerun pass. Protocol schemas/generated bindings are unchanged;
bundled generator/runtime pairing and integrity tests pass. `flatc` remains absent,
so fresh generation was not run. Approved localhost discovery at port 1234 refused
connections; no live-model/GPU efficiency or quality result is claimed.

### Prune impossible partial finite-domain assignments — 2026-10-22

**Implemented (bounded deterministic pruning); Partial (general solving).**
Architecture sections 5 and 9 require algorithmic reasoning under fixed compute
ceilings. Jev-R now rejects impossible partial linear ranges and assigned-value
collisions in all-different constraints before enumerating their remaining
variables. Signed i128 arithmetic and once-per-search true domain extrema keep
bounds conservative, including sparse domains and i64 extremes. Disequality only
prunes an interval fixed to the forbidden value. Declaration-order traversal and
the independent complete-assignment witness checker remain intact.

The root and each visited partial assignment still consume one node; unvisited
pruned descendants consume none. The existing 4096-node ceiling, cancellation
polls, original tool reservations and final stop check remain authoritative.
Contradictions can now be established earlier; a limit is still inconclusive.
Receipt schemas and recovery budgets are unchanged, and historical receipts are
not replayed. No external execution, new permission or neural adaptation is added.
Sources: `src/verification/finite.rs`, its unit tests and
`tests/finite_search_pruning.rs`. See [search contract](FINITE_SEARCH.md#search-and-outcome-contract).
This is necessary-condition pruning, not full domain propagation, Hall-set
filtering, a general SAT solver or evidence of improved model intelligence.
Jev+ and Jev-R remain Partial; general search recovery and independent quality/
cost evaluation remain incomplete.

Verification: formatting, locked all-target workspace build, **813 tests across
48 targets** (none ignored) and Clippy with warnings denied pass. Fourteen focused
finite-domain unit tests and six new integration tests cover every relation,
wide signed arithmetic, sparse domains, deterministic exhaustive comparisons on
72 generated cases (both satisfiable and unsatisfiable), original checker events,
root accounting, exact witness/cap edges, cancellation and the hard 4096 ceiling.
The independent legacy traversal reference visits 10 nodes versus the new 4 for
a sparse linear contradiction and 31 versus 11 for an all-different contradiction.
These are logical search-work examples, not latency, live-model quality or
physical-efficiency measurements. The initial test fixtures were corrected for
sign and earlier pruning; the cap fixture now uses a 16-variable parity case that
intervals cannot resolve. Three test-only lint findings were fixed, with no
suppression or gate removal. Existing socket, migration, mock inference, malformed
frame, denial, cancellation, recovery and rollback coverage remains green.
Schemas and generated bindings are unchanged; bundled generator/runtime pairing
and integrity checks pass. `flatc` is unavailable, so fresh generation was not run.

### Find a complete declared capability cover within pinned route limits — 2026-10-22

**Implemented (bounded exact complete coverage); Partial (general calibrated routing).**
Architecture section 4 requires dynamic capability discovery and robust routing;
section 9 requires sparse activation under fixed ceilings. Jev+ now uses
`bounded-cover/v3`: preserve initial route order/pins and version exclusions, then
find a full compatible available cover with the fewest additional providers when
one fits the remaining four-route capacity. Equal coverage masks share a stable
representative. A two-provider-union table plus superset transform is exact for
complete sets of at most four additions; ties are deterministic, not a claimed
globally lexicographic optimum. If no full cover fits, the old greedy partial rule
remains and is not presented as maximum partial coverage.

Bounds remain 4096 skills/manifests and 16 requested IDs. Compact indices cap
pair enumeration at 8,386,560 visits and tables at 65,536 entries each; cheap full
cover/impossible-union paths avoid unnecessary transforms. The engine yields
before/after synchronous resolution so cancellation/deadline can stop publication
and later dispatch, without claiming mid-loop preemption or physical CPU limits.
The additive declaration-only `coverage_search` audit records feasibility, provider/
mask/work counts, additions and fallback with no permission authority.

Sources: `src/capabilities.rs`, `src/capabilities/cover.rs`, Engine audit dispatch,
resolver unit regressions and `tests/capability_cover.rs`. See [capability selection](CAPABILITIES.md#selection).
No tool grant, model call, network probe, active-version mutation or new provider
execution is added. Existing workflow/resource/verification gates still apply.
Minimum declared route count is not measured inference cost, backbone sharing,
physical memory saving or calibrated usefulness. Jev+ and Jev-R remain Partial;
independent provider utility/cost evidence and broader resource-aware scheduling
remain subsequent milestones.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**804 passed across 47 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Eighteen focused capability tests cover the greedy
trap, independent exhaustive small-case minimum-cardinality oracles, order
invariance, retained routes (including unavailable pins), version/availability
filters, ablation exclusions, fast paths and the full 4096-provider/16-bit bound.
The actual Engine integration exercises interpretation, fixed metadata audits,
pinned bindings and workflow dispatch across Full/Collective/Experts profiles.
Two unoptimized selection-kernel observations were 1089.067/1083.882 ms with
8,386,560 pair visits and 786432 bytes of table entries, excluding fixture/Cargo
and total-process overhead. These are synthetic local timings, not a model or
physical-efficiency comparison. See the reproducible [cost check](CAPABILITIES.md#deterministic-cost-check-2026-10-22).
Initial fixture expectations were corrected for a provider's compatible subset;
three test-only allocation lint warnings were fixed without suppression, followed
by a complete final rerun. Existing socket, database migration, mock inference,
malformed frames, permission denial, cancellation, continuation and rollback tests
remain green. Protocol/bindings are unchanged; exact bundled pairing/integrity
checks pass. `flatc` is unavailable and fresh generation was not run.


### Persist and independently recover direct search witnesses — 2026-10-21

**Implemented (durable direct witnesses); Partial (general search-phase recovery).**
Architecture sections 5, 7, 9 and 10 require persistent reasoning evidence with
independent checks and retained budgets. New `finite_search` witness receipts use
format 2: the exact bounded integer assignment and SHA-256 are one durable event
before the separate original verifier check. Non-witness receipts retain exact
format 1. SQLite audit receipt versioning does not replace typed FlatBuffers IPC.

The existing operator result-recovery API can now select this durable candidate
without a model solver event. A strict bounded validator checks original settings,
context, authorized search policy/charge lineage, unique receipt, assignment hash,
node limits and allowed later verifier work. It exposes only metadata/hash history;
the answer body remains in the owner-local task trace. Every fresh recovery check
uses another original tool allowance and checks the complete original criterion.
Prior search/checker reservations are retained, even if an original proof was lost.
No model or search replay, permission grant, fresh reasoning cycle or wall budget.

Successful fresh proof/status/audit writes commit atomically; a malformed receipt
or exhausted allowance writes nothing, and audit failure rolls back the new charge.
A hash-consistent but failing candidate stays unverified and cannot trigger model
revision. Saved receipt flags and old verifier events cannot complete a task.
Sources include `src/reasoning.rs`, `src/recovery/{finite_search_witness,finite_search,result}.rs`,
recovery module wiring and `tests/finite_search_witness_recovery.rs`. See
[direct witnesses](FINITE_SEARCH.md#persist-and-recover-direct-search-witnesses).
Legacy body-less witness receipts, contradictions and search-phase crashes before
receipt persistence remain unsupported. Jev+ and Jev-R remain Partial; resumable
search state and independent quality/cost evidence remain subsequent milestones.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**792 passed across 46 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen files. Eight new integration tests cover all three durable
receipt/checker crash windows, actual CLI recovery, fresh-check charging and
rollback, altered/duplicate/legacy receipt and phase rejection, task-answer distrust,
metadata redaction and a valid exhausted ledger with zero remaining allowance.
The focused eight-test binary also passes. Initial unused test imports were removed;
the exhaustion fixture was corrected to prove budget refusal rather than duplicate
charge refusal, followed by a complete final rerun. Existing limit/model recovery,
progress, socket, migrations, mock inference, malformed frames, policy denial,
cancellation, continuation and candidate rollback checks remain green. Exact
bundled FlatBuffers pairing/integrity checks pass; `flatc` is unavailable and fresh
generation was not run. No real-model, GPU/energy or general efficiency experiment
was performed for this change.


### Reconcile saved model candidates after bounded search — 2026-10-21

**Implemented (receipt-bound model-result reconciliation); Partial (general search-phase recovery).**
Architecture sections 5, 7, 9 and 10 require evidence-aware persistent reasoning
under independent permissions. `recover-verified-result` validates a completed
original search-limit receipt before reserving a fresh independent verifier check.
It recovers only a recorded ledger-backed solver candidate and checks the full
original criterion. A satisfying advisory candidate is still a genuine witness;
role labels and model assertions alone cannot complete a task.

Reconciliation never repeats search or inference and retains all original charges.
Failed checks remain charged, while identical recovery proof metadata may be reused.
Rejected advisory proofs are validated against their actual recorded solver text
and independent failure without advancing whole-goal progress. Failed whole-goal
checks may create a ready revision only under existing recurrence/plateau, graph,
escalation, resource and original deadline gates. Atomic audit failure rolls back
new proof/checkpoint/charge writes. Reports expose bounded `finite_search_history`.

Sources: `src/recovery/{finite_search,result,continuation,continuation_progress}.rs`,
progress regression tests and `tests/finite_search_result_recovery.rs`. See
[saved search candidates](FINITE_SEARCH.md#reconcile-a-saved-model-candidate-after-the-search-limit).
No capability grant, network access, generated code, protocol, migration, model
weight or acceptance threshold changes. Explicit proof reconciliation retains its
bounded SQL transaction; it does not create a new task wall allowance. Direct
search results and incomplete/skipped search phases remain unsupported. Jev+ and
Jev-R remain Partial; durable search-result recovery and independent quality/cost
measurement remain subsequent engineering milestones.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**784 passed across 45 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on frozen sources. Nine new integration tests cover receipt/candidate checks,
atomic audit rollback, repeated charged recovery, proof/assessment crash windows,
advisory witnesses and rejected-advice graph continuation, plateau refusal and
expired-clock reconciliation. Actual loopback HTTP confirms resumed call counts.
The eight focused progress-validator tests and six existing result-recovery unit
tests pass, including strict advisory proof tampering and legacy no-call proof
compatibility. Initial fixture errors (subtask state/plan shape, checkpoint count
and plateau length) were corrected against runtime semantics; production budgets,
thresholds and existing tests were preserved. Existing socket, migrations, mock
inference, malformed frames, denials, cancellation, continuation and candidate
rollback tests remain green. Exact bundled FlatBuffers pairing/integrity checks
pass; `flatc` is unavailable and fresh generation was not run. No real-model,
GPU/energy or general quality/performance measurement was performed for this slice.


### Continue model reasoning after a recorded search limit — 2026-10-21

**Implemented (validated limit-receipt continuation); Partial (general search recovery).**
Architecture sections 5, 7, 9 and 10 require persistent reasoning progress under
original budgets and independent permission checks. `src/recovery/finite_search.rs`
strictly validates a bounded original-context → policy → search reservation →
`limit_reached` receipt chain before restoring an existing reasoning checkpoint.
The exact objective hash, configured node ceiling, exhausted node count, event
sources/order, unique charge and non-authority flags must agree. Search history
cannot appear in a configuration/objective that did not enable it.

Supported ready solver/advisory-graph continuation retains all original tool,
call, cycle, token and prompt charges, context/model pins and deadline. It resumes
the saved strategy and never traverses the search tree again. Preview and durable
continuation reports expose bounded `finite_search_history` metadata. Repeated
admission revalidates the same receipt; it cannot create a new search budget.
Missing, duplicate, altered, oversized, post-boundary or incomplete search history
fails before new dispatch. No external solver, permission, migration, protocol or
neural promotion is added.

Search-phase crashes, terminal search witnesses/contradictions/deadlines,
budget-skipped searches and saved-result reconciliation remain unsupported.
Metadata consistency does not authenticate a malicious owner or prove physical
provider stopping. Jev+ and Jev-R remain Partial. Sources also include continuation
admission/reporting and `tests/finite_search_resume.rs`; see [search recovery](FINITE_SEARCH.md#continuation-after-an-incomplete-search).
The next search milestone is receipt-bound saved-result reconciliation and broader
phase recovery, followed by independently curated budget-matched quality studies.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**774 passed across 44 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on frozen sources. Eight helper unit tests validate receipt/cap/permission
shape, preserved verifier reserve, unknown/duplicate JSON and bounded histories.
Three integration tests exercise repeated actual HTTP solver continuation,
partially executed graph continuation, original model/configuration pins and 18
independent tampering cases with no admission writes. The focused helper and HTTP
suites also pass. Existing socket, database migration, mock inference, malformed
frame, permission denial, cancellation, continuation and candidate rollback checks
remain green. Exact bundled FlatBuffers generator/runtime pairing and integrity
checks pass; `flatc` is unavailable and fresh generation was not run. No real-model,
GPU performance or provider-stop measurement was made for this change.


### Search explicit finite-domain tasks under independent limits — 2026-10-21

**Implemented (opt-in bounded deterministic search); Partial (general solving and recovery).**
Architecture sections 5 and 9 require algorithmic search under enforced budgets.
`Config.finite_search.max_nodes` admits 1–4096 visited partial assignments, including
the root. Rust DFS respects declared variable/domain order, prunes only fully
assigned constraints, checks the original deadline and never runs generated code.
A separate Jev+ `constraints.solve` grant and tool charge precede traversal; a
second tool slot is preserved for independently checking a witness or fallback.
Default absence and the LLM-only profile retain their original behavior.

A witness must pass a separately charged original-criterion check. Exhaustive
contradictions stop with `proven_unsatisfiable` but remain rejected under the
satisfying-witness contract; witness-only evaluations cannot score them as solved.
Limit exhaustion retains charges and falls back to existing bounded inference.
Fixed-metadata search audit failures prevent further work. Saved enabled finite-domain tasks
cannot continue until pre-model search evidence has a validated recovery format;
no traversal or inference is replayed and no charge is refunded.

Sources: `src/verification/{finite.rs,finite/tests.rs}`, `src/reasoning.rs`,
`src/reasoning/finite_search.rs`, configuration, security grants, continuation
admission and `tests/finite_search.rs`. See [search contract](FINITE_SEARCH.md).
No model weights, protocol schema, migration or acceptance threshold changes.
Deterministic oracle/runtime checks establish this limited algorithmic behavior,
not held-out neural quality, physical efficiency or general intelligence. Jev+
and Jev-R remain Partial. The next search milestone is validated durable recovery
and a budget-matched evaluation on independently curated unseen problems.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**763 passed across 43 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final frozen sources. Eleven finite-checker unit tests and ten runtime
tests include independent small-domain enumeration, exact node bounds, deadline
hooks, audit denial, ready cancellation, fallback accounting and all 17 public
cases: 14 checked witnesses and three exhaustive contradictions, with no model
calls. Configuration and saved-result recovery denial tests also pass. Existing
socket, migrations, mock inference, malformed-frame, policy denial, cancellation,
continuation and candidate rollback tests remain green. The first full run failed
the source-fingerprint consistency test while the final permission fix changed
source; a complete frozen-source rerun passed. Exact bundled FlatBuffers pairing/
integrity checks pass; `flatc` is unavailable and fresh generation was not run.
No real-backend inference or physical efficiency study was run for this slice.


### Retain and independently check the public progress-policy diagnostic — 2026-10-21

**Measured public integration; independent utility remains Partial.** Source
08f6217 completed 17 paired RESEARCH cases (34 tasks) on existing Qwen3 8B Q4_K_M
weights with the same collective and budgets. The disabled/enabled arms found
14/13 witnesses, used 85/78 calls and accumulated 55.87/48.44 seconds task latency.
The corpus has 14 satisfiable cases and three exhaustive-oracle contradictions.
The separate Rust enumeration checks all 34 final scores, retained report hash,
resource sums and audit projection consistency without the production verifier.
Raw report and provenance are immutable in `evaluation/results/`.

Only one plateau stop was recorded, on unsatisfiable case 16. The satisfiable
regression at case 4 did not trigger plateau stopping; different planner/advice/
solver paths and one public run prevent causal policy-effect claims. Lower cost
with fewer witnesses is not a demonstrated efficiency win. Default policy remains
off. No larger baseline, repeated trials, held-out/independent curation, provider
weight attestation or attributed physical resource proof exists. Jev+ and Jev-R
remain Partial; matched counterfactual/repeated independent studies remain open.

The initially off/unloaded loopback server was temporarily started with existing
weights (requested GPU ratio 1, context 4096, parallel 1); its initial state was
restored and checked after the 104.37-second run. No weight download, persistent
policy installation, capability promotion or neural change occurred. See
[public results](../evaluation/results/README.md#public-finite-domain-progress-ablation--2026-10-21).

Verification: formatting, locked workspace/all-target build checks,
**745 tests across 42 targets** (none ignored), and Clippy with warnings denied
pass on final unchanged sources. The focused two-test corpus/evidence regression
also passes. Existing socket, migration, mock inference, malformed-frame, denial,
cancellation, recovery and rollback checks remain green. Protocol/generated
bindings are unchanged; exact bundled pairing/integrity tests pass, while `flatc`
is unavailable and fresh generation was not run. Real-provider observations are
retained separately from deterministic mocks; all 34 final answers are checked
by a separately implemented exhaustive oracle. General quality, causal utility,
independent human review and physical safety claims remain unproven.


### Measure progress stopping with paired collective runs — 2026-10-21

**Implemented (explicit controlled workflow comparison); Partial (independent utility evidence).**
Architecture sections 5, 9 and 13 require measurable reasoning under comparable
budgets. `compare-progress` and `src/benchmarking/progress.rs` run the same collective
with the stopping option absent/present, using the same backend, frozen corpus,
configured resource ownership and original ceilings. Cases alternate which arm
executes first. Effective configuration, scope fingerprints, task identities,
paired quality changes and actual costs are reported; unmatched cases cannot
contribute paired benefit. Cancellation and the overall deadline preserve partial
reports. An enabled adaptive advice guard is refused to avoid live-history bias.

Only strict bounded explicit finite-domain cases are accepted, with no supplied
expected assignments. Scoring requires an actual independently checked witness;
model impossibility claims never pass. Public workflow reports retain bounded
content for external checking; held-out reports omit objective/answer bodies.
No report installs a policy, changes persistent configuration, promotes a model
or expands permissions. Provider/cache/thermal state and physical resource use
remain outside the comparison's control.

The new 17-case CC0 public corpus covers linear relations, signed/wide arithmetic,
all-different, alternative witnesses and contradictions. Its manifest binds hashes
and exact solution counts. `tests/finite_domain_corpus.rs` independently enumerates
at most 256 assignments per case without invoking the production checker. This is
an independent computational oracle, not independent human curation or held-out
quality evidence. Scripted loopback tests expose both saved calls and a missed
later witness; the actual CLI is exercised with the deterministic demo adapter.
Sources also include `tests/progress_ablation.rs`, `tests/progress_ablation_cli.rs`,
main command wiring and [benchmark instructions](BENCHMARKING.md#controlled-progress-stopping-ablation).
General utility calibration and real-model benefit remain unproven; Jev+ and
Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**744 passed across 42 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on the final unchanged source. Seven focused tests cover the exhaustive
corpus oracle, scripted paired tradeoff, strict preflight, held-out body omission,
pre-dispatch cancellation, partial deadline accounting and actual CLI deployment.
Initial compilation/fixture failures (JSON macro/report assembly, test borrowing
and private store access) were corrected; no tests or limits were removed.
Existing socket, migration, mock inference, malformed-frame, denial, cancellation,
continuation and rollback regressions remain green under approved local socket
access. Protocol/bindings are unchanged; exact bundled pairing/integrity checks
passed. `flatc` is unavailable and fresh generation was not run. These checks
establish runner behavior, not a real-model efficiency or quality improvement.


### Stop finite-domain revisions using independent progress evidence — 2026-10-21

**Implemented (opt-in finite-domain plateau stopping); Partial (general progress/utility).**
Architecture sections 5 and 9 require bounded evidence-aware revision and marginal
improvement stopping. Optional `Config.progress_stopping.plateau_cycles` (2–4)
tracks the number of violated declared constraints for complete, in-domain `csp:`
assignments. Shared strict parsing and bounded i128 checks preserve full acceptance.
Malformed/incomplete/out-of-domain output is incomparable. Improvements reset the
streak; equal/worse scores increment it; incomparable outputs reset only the streak.
Full independent verification runs first, and valid witnesses complete immediately.
A plateau retains the latest rejected answer/charges and stops before another
solver reservation; default absence retains legacy behavior and serialization.

Required fixed-metadata `reasoning_progress` audits precede escalation. Supported
continuation recomputes progress from recorded solver answers and matching failed
checks, validates audit ordering/content and the saved compact state, and refuses
missing/altered/gapped/future or already stopped histories. Operator result recovery
retains fresh-check accounting without inference replay. No new capability, model,
weight, tool allowance, migration or protocol schema is introduced.

Sources include `src/reasoning/progress.rs`, Session/configuration, shared finite
verification, `src/recovery/continuation_progress.rs` and result reconciliation.
Unit/scripted regressions and `tests/progress_stopping.rs` cover score correctness,
optional configuration, plateau/improvement/incomparable behavior, witness precedence,
audit failure and continuation with one new actual HTTP solver call. See
[progress stopping](PROGRESS_STOPPING.md) for bounds and limitations. Constraint
counts are not distance-to-solution, impossibility or calibrated marginal utility.
Real-model quality/latency gains and unrestricted reasoning progress remain
unproven; Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**737 passed across 39 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on final unchanged sources. The focused progress selection passed 21 tests;
the two actual HTTP continuation tests also passed, including both operator
recheck crash windows. Existing socket, database migration, mock inference,
malformed-frame, denial, cancellation, continuation and rollback tests remain green.
Protocol/generated bindings are unchanged; reviewed exact generator/runtime pairing
and integrity tests passed. `flatc` is unavailable; fresh generation was not run.

An initial restricted test selection could not bind a service socket. Approved
host-socket runs passed after repairing new test fixture ordering/identity errors
and a missing import/helper attribute. The first full test pass was followed by a
Clippy match-style warning; its suggested simplification was applied and all gates
rerun successfully. Tests, limits and acceptance thresholds were retained. No live
model-quality, speed, GPU-use or held-out effectiveness result is claimed.


### Deploy reviewed workflow policies through the HTTP host — 2026-10-21

**Implemented (explicit host deployment); Partial (general policy lifecycle).**
Architecture sections 4, 9 and 10 require replaceable coordination decisions under
independent permissions. `crates/cogfabric-server/src/main.rs` now accepts paired
`--workflow-study` / `--workflow-thresholds` options. The existing bounded strict
no-follow loader validates and pins the policy before configuration/state/backend
initialization. Hidden worker entry points reject host policy options before
worker startup. Omission retains default tag routing.

Only the collective Engine receives the policy; native conversation turns retain
their existing path. Requests cannot select evidence files or thresholds, replace
the host router or raise budgets. Policy-file replacement after startup does not
change the retained policy. No persistent configuration, capability grant,
executable, model weight or provider lifecycle changes. Reviewed synthetic studies
still do not establish held-out quality, calibrated utility or physical efficiency.
Pre-routing checkpoint restoration and broader independent evaluation remain open;
Jev+ and Jev-R remain Partial.

The actual server-process regression covers default routing, evidence-matched
acceptance, exact accepted/declined policy identities, collective requests after
policy-file replacement, a stricter policy in a fresh host, native-turn isolation,
HTTP override rejection without inference, and invalid/partial/worker options
before state creation. These are synthetic loopback integration checks, not live
model-quality or speed measurements.

Sources and reproduction: server main, process integration in
`crates/cogfabric-server/tests/workflow_policy.rs`,
[workflow deployment](EMPIRICAL_WORKFLOW_ROUTING.md#direct-cli-and-local-service-deployment)
and [HTTP setup](HTTP_API.md#reviewed-collective-workflow-policy).

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**719 passed across 38 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The focused real-process test also passes. Its initial restricted-sandbox
run could not bind TCP (`EPERM`); the approved host-socket retry and full suite
passed. Existing socket, database migration, mock inference, malformed frame,
policy denial, cancellation, continuation and rollback regressions remain green.
Core protocol/generated bindings are unchanged; bundled exact-pairing/integrity
checks passed. `flatc` is unavailable, so fresh generation was not run. No live
model, GPU or independently held-out effectiveness claim follows from this change.


### Guard optional advice using recent scoped costs — 2026-10-21

**Implemented (opt-in recent-cost suppression); Partial (calibrated utility).**
`Config.advice_guard` configures bounded minimum/sample/window/failure thresholds.
`src/reasoning/advice_guard.rs` binds configuration, compiled prompts/source,
canonical modules, profile, mode, criterion and initial prompt-size bucket. It
examines a fixed newest-512-event tail with strict 4-KiB observation bounds and
same-boot time, taking at most 16 current matching samples. Timeout/error and
completed-over-current-budget observations count as failure; exact threshold
comparison can skip optional planning/exploration before new reservations.
Larger original phase-budget buckets are required; stale/future/other-boot samples
are ignored. No model, permission, inference allowance or schema change is added.

Required decision/outcome auditing fails closed. Tickets retain original phase
ceilings through history/audit time, and expired tickets start no inference.
No-call phases create no positive observation. Persistence survives reopen; model,
module/config/source/criterion/profile changes separate cohorts. Execution profiles
cannot suppress one another. Comparison discloses that enabled same-profile
history is live rather than frozen. Saved graph/revision continuation retains its
existing path and original budgets. Default absent serialization stays compatible.

Eight unit/engine tests plus an actual separate-process CLI/HTTP regression cover
history bounds/scope/time/budget, thresholds, expensive completions, both stages,
model/profile separation, persistence, expiration and audit failures. See
[advice guard](ADVICE_GUARD.md). Mechanical phase completion is not usable/verified
advice. Cancellation can leave no observation, the tail/window can forget history,
and concurrent warm probes remain possible. Real-model gains, expected utility,
physical resource proof and broader evaluation remain incomplete. Jev+ and Jev-R
remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**718 passed across 37 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass on the final unchanged source. The focused five-test implementation pass and
actual CLI/HTTP regression passed before final profile/cost/expiration coverage.
An earlier full run correctly rejected a compiled-source fingerprint after edits
continued during that run; the check was preserved and the final rebuild/full gate
passes. Existing sockets, migrations, mock inference, malformed frames, denials,
cancellation, recovery and rollback remain green under approved local socket
access. Protocol/generated bindings are unchanged; exact bundled pairing/integrity
checks pass, but `flatc` is absent and fresh generation was not run. No real-model
performance/quality gain is claimed for the new guard; its fixture evidence is
separate from the preceding public Qwen diagnostics.

## Retain and independently check public portfolio evidence — 2026-10-21

**Measured public integration / general quality remains Partial.** Two complete
24-task Qwen3 8B runs now have immutable raw reports, hashed provenance/settings,
public audit projections and a fixed four-case subset. The new regression checks
all 48 scores through independent exhaustive simple-path enumeration and
reconciles calls, latency and diagnostic event sums. Existing suites/labels and
historical reports are unchanged. See [results](../evaluation/results/README.md).

At source 5041478, all six profiles solved 4/4, with 12 accepted portfolios and
14 selection events. At b20d736, alone solved 3/4 and every other profile 4/4;
collective modes required more calls/time than tools/recursive. All 12 planning
and 12 exploration phases timed out, with zero accepted/selected portfolios in
that second run. Diagnostics therefore distinguish attempted phases from usable
strategy output. Original budgets remained enforced and failed usage stayed unknown.

The October 21 run used different manual provider load/offload/parallel settings;
no causal before/after claim is valid. The initially off/unloaded localhost server
was temporarily started with existing weights, then restored and checked. No
weights were downloaded or neural/capability promotion performed. Public/authored
cases, missing independent review/larger baseline, unknown weight/device identity
and unmeasured physical resources prevent general quality/efficiency claims.
Cost-aware optional planning/exploration admission is the next verifiable target;
Jev+ and Jev-R remain Partial.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**709 passed across 36 targets**,
none ignored) and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The focused frozen-report regression also passes. Existing protocol pairing,
malformed frames, socket integration, migrations, mock inference, denials,
cancellation, recovery and candidate rollback remain green with approved local
socket access. Protocol/generated bindings are unchanged; `flatc` is absent, so
fresh generation was not run. Real-provider validation is explicitly separate
from mocks; all outcomes and provider-setting differences are retained. The
previous automatic-review quota failure did not execute the pending comparison;
this date's approved retry is the recorded b20d736 run, not a silent restart.

### Report bounded hypothesis observations — 2026-10-20

**Implemented (event diagnostics and compiled scope); Partial (quality/utility evidence).**
`src/evaluation/hypothesis_diagnostics.rs` projects only fixed labels from at most
128 matching task events, with a sentinel and 16 KiB per-record ceiling. Known
source/version/non-authority/policy/shape gates distinguish accepted portfolios,
legacy advice, invalid portfolios, selected/exhausted hypotheses and unknown
metadata. Unknowns stay unclassified; oversized, malformed or excessive audit
fails reporting. No strategy/ID/hash/prompt/error text leaves this projection.

`bench`/`compare` detail rows include these counters; comparison resources sum only
executed rows. Headers bind runtime-source bytes; comparison also binds compiled
role prompts separately from operator-attested revision. Counts describe audit
observations, not unique dispatches, verification, utility or provider stopping.
Three reader tests cover known/scoped/private/additive counts, unknown variants and
bounds; existing scripted portfolio/comparison tests now verify actual projection
and per-arm sums. See [diagnostics](EXPLORATION.md#diagnostic-reporting). No budgets,
permissions, model calls or execution policy changes. Jev+ and Jev-R remain Partial.

Verification: formatting, locked workspace/all-target check, **708 tests across 36
targets** (none ignored), and Clippy with warnings denied pass. Existing socket,
migration, mock inference, malformed-frame, denial, cancellation, recovery and
rollback tests remain green under approved local socket access. Protocol/bindings
are unchanged; bundled pairing/integrity checks pass, while `flatc` remains absent
and fresh generation was not run. Approved host-level discovery found the existing
LM Studio server ON with Qwen3 8B loaded; earlier sandbox-only refusal observations
must not be interpreted as proof that the host server was unavailable. One live
portfolio smoke and a 24-task public comparison at preceding source 5041478
completed; their exact source/evidence scope is separate from this reporting change.
Independent held-out quality, calibrated utility and physical resource proof remain
incomplete. Jev+ and Jev-R remain Partial.

### Explore bounded alternative strategies — 2026-10-20

**Implemented (structured bounded hypotheses); Partial (semantic diversity/utility).**
`src/reasoning/exploration.rs` accepts two/three strictly bounded distinct approach
records. Canonical IDs select one unused approach per whole-goal cycle. JSON-framed
selected advice has priority within the unchanged strategy cap; checker feedback
and prose reflection remain available. Acceptance/selection audits fail closed.
Malformed structured responses are discarded; bounded legacy prose remains single
unverified advice. The explorer still reserves 128 outputs and no extra inference,
cycle, tool, deadline or permission allowance is added.

`RestoredState` and continuation reconstruct the portfolio from one original
validated cycle-zero explorer result. New advisory acceptance metadata is not
required for restoration, while ambiguous/misplaced explorer history is refused.
Saved next-solver strategy composition is idempotent, and custom escalation
admission/recovery retain matching actual prompt-byte coverage. Original prompts,
resources, contexts and independent verification retain authority. The changed
explorer prompt fingerprint intentionally refuses incompatible old checkpoints
and empirical scopes rather than silently migrating them.

Eight unit/engine tests and two actual local HTTP continuation tests cover strict
bounds/duplicates/unknown authority fields, canonical/once-only selection, worst
escaping/caps/idempotence, both revision paths, audit failure, invalid structured
output, original custom-policy allowance and no explorer replay. See
[exploration](EXPLORATION.md). Text normalization does not prove semantic diversity;
canonical order is not expected utility. Real-model effectiveness and novel neural
architectures remain unverified/incomplete. Jev+ and Jev-R remain **Partial**.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**705 passed across 36 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The initial seven focused tests, two HTTP continuation cases and first full
suite passed; review then tightened legacy fallback and added wrapped-JSON coverage,
and all final checks were rerun successfully. Existing socket, migration, mock
inference, malformed-frame, policy-denial, cancellation, recovery and rollback
checks remain green with approved local socket access. Protocol/schema bindings
are unchanged; bundled generator/runtime integrity checks pass, but `flatc` is
unavailable and fresh generation was not run. Discovery at 127.0.0.1:1234 refused
connections. No real-model portfolio effectiveness, calibrated ranking or quality/
performance improvement is claimed. Those measurements and broader independent
reasoning remain incomplete; Jev+ and Jev-R remain Partial.

### Configure dependency-chain priority — 2026-10-20

**Implemented (bounded built-in ordering); Partial (general calibrated scheduling).**
`Config.subtask_scheduling` optionally selects `critical_path`; omitted/null/stable
preserves legacy stable topological order and default serialization. Explicit
selection binds configuration snapshots/fingerprints. `src/scheduling.rs` validates
the four-node graph, computes unit-cost downstream heights, selects longest ready
chains first and ties by canonical ID. Engine and direct Session construction share
installation; trusted native Engine overrides remain supported.

Existing admission and intent/validated-order commits precede all execution.
Recovery retains the committed order after current configuration changes. No extra
inference, permission, budget or protocol field is introduced. Six unit tests cover
all 64 four-node edge subsets in every input permutation, chain priority, public
invalid plans, strict/default config/fingerprints, persisted execution, direct
Session construction and both intent/decision audit failures. One actual CLI/local
HTTP regression covers fresh execution and original-prefix continuation without
replaying planning or completed work. See [scheduling](SCHEDULING.md#configured-critical-path-ordering).
This sequential graph heuristic is not calibrated cost/utility or proof of better
latency, quality or physical resource allocation. Jev+ and Jev-R remain **Partial**.

Verification: formatting, `cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**695 passed across 35 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. Focused ordering tests and actual CLI/HTTP fresh/resumed execution also pass;
existing socket, migration, mock inference, malformed-frame, denial, cancellation,
recovery and rollback checks remain green under approved local socket access.
Protocol/schema bindings are unchanged. `flatc` is unavailable; fresh generation
was not run, while build-time bundled pairing/integrity checks pass. Discovery at
127.0.0.1:1234 refused connections, so no real-model performance/quality result is
claimed. Broader calibrated scheduling and physical resource/lifecycle verification
remain incomplete; Jev+ and Jev-R remain Partial. The next reasoning milestone is
structured exploration of multiple alternative strategies under original budgets.

### Stop nonadjacent whole-goal candidate recurrence — 2026-10-20

**Implemented (bounded exact-content stopping); Partial (general reasoning quality).**
`src/reasoning/convergence.rs` records the first whole-goal solver hash per reserved
cycle. A → B → A now stops before further escalation, retaining the latest answer
and all charges. Independent checker success takes precedence. Cycle-zero advice
and same-cycle recursive-ablation critique/reflection do not count as candidates.
A required versioned `candidate_progress` audit records prior matching cycles;
audit failure prevents further inference. History is bounded by 16 cycles/64 nodes.

`src/recovery/continuation.rs` reconstructs this history from validated original
nodes, including legacy records without progress events. Already recurrent
checkpoints are refused; revision-result reconciliation rolls back its prospective
boundary on refusal. Independently rejected resumed answers now remain `rejected`,
with `verification_attempted` and `stop_reason` in the report.

Two history unit tests, three engine tests and two integration tests cover bounded
invalid histories, nonadjacent structured/prose repeats, successful checker
precedence, recursive role aliases, audit failure, actual local HTTP continuation
and atomic reconciliation refusal. See [reasoning](REASONING.md#stop-nonadjacent-candidate-recurrence)
and [recovery](RECOVERY.md#candidate-recurrence-and-rejected-continuations).
This heuristic does not establish semantic progress, unsatisfiability or real-model
quality/performance. Jev+ and Jev-R remain **Partial**.

Verification: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**688 passed across 34 targets**,
none ignored), and `cargo clippy --locked --workspace --all-targets -- -D warnings`
pass. The first full run exposed an old custom-policy continuation assertion
expecting `unverified` after independent rejection; it now requires `rejected`
and an attempted verification, and the final full suite passes. Existing socket,
migration, mock inference, malformed-frame, policy-denial, cancellation and rollback
checks remain green with elevated local socket access. Protocol/schema bindings
are unchanged; `flatc` is unavailable, so fresh generation was not run. Discovery
at 127.0.0.1:1234 refused connections; real-backend quality/performance is untested.

### Negotiate declared capability versions during interpretation — 2026-10-20

**Implemented (bounded advisory SemVer selection); Partial (implementation/quality proof).**
`Interpretation.capability_versions` is an optional map tied to existing requested
IDs, capped at 16 entries and 64 bytes per requirement. Strict parsing rejects
hidden requests, ambiguous keys and invalid ranges while preserving legacy JSON.
The interface prompt requests versions only when specified by the original user.
Native Rust literals initialize the added map explicitly.

`greedy-cover/v2` in `src/capabilities.rs` matches direct IDs against pinned manifest versions and aliases
against pinned capability versions. Unknown legacy versions do not satisfy explicit
ranges. Joint coverage excludes incompatible credit, removes exclusively mismatched
initial routes, preserves valid mixed/unrelated routes and distinguishes incompatible
from missing, unavailable and unselected requirements. Audit bindings record the
requirement and compatibility result. Reference refinement requires matching bindings;
no model/skill activation, grant or original-goal criterion change occurs.

Tests cover alias vs provider versions, legacy unknowns, prereleases, order invariance,
mixed coverage, incompatible-vs-unavailable classification, parser bounds/duplicates,
actual workflow selection and in-flight version replacement. An unsatisfied range
cannot silently fall back to the incompatible tagged expert. See
[version requirements](CAPABILITIES.md#optional-declared-version-requirements).
Role-prompt fingerprints change, so existing recovery refuses differing old prompt
hashes rather than silently migrating checkpoints. Declared compatibility is not
API/weight attestation or measured extraction/usefulness. General execution and
independent effectiveness remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification: formatting, locked workspace/all-target check, final full tests
(**681 passed across 33 targets**, none ignored), and Clippy with warnings denied
pass. The earlier full-suite request was not executed because automatic approval
review hit a usage limit; the current approved retry and final suite passed.
Temporary prior logs were absent after environment restart, so verification was
rerun from the current worktree. Existing protocol pairing/malformed frames,
migrations, mock inference, denials, cancellation, recovery and rollback remain
green under elevated local socket access. `flatc` is unavailable; schemas/bindings
were unchanged and fresh generation was not run. Discovery at 127.0.0.1:1234 refused
connections; no live interpretation accuracy, quality or performance gain is claimed.



### Schedule inference roles with priority and bounded bypass — 2026-10-17

**Implemented (optional local resource policy); Partial (general/physical scheduling).**
`Settings.scheduling` accepts an operator role-priority map and hard bypass ceiling.
`src/resource_scheduler/queue.rs` selects higher-priority waiting work, FIFO ties
and oldest protected requests. Once bypassed the configured number of times, an
older request cannot be displaced again; a protected large request can block smaller
newcomers until its allocation fits. Running requests are never preempted. Default
FIFO serialization/audits remain unchanged. Native Settings constructors explicitly
use None for legacy behavior; existing CLI resource configuration supports the policy.

Role priorities and reservation bypass counts are recorded before provider entry
and exposed in pool status. Existing task/resource/resident/quarantine bounds retain
authority. Original configured continuation validates policy, role-derived priority
and recorded bypass bounds, rejects altered/missing fields and restores the compiled
policy without loading code. Live pool identity includes policy settings.

Seven focused tests in `src/resource_scheduler/priority_tests.rs` cover actual role
contention, priority/FIFO ties, bounded bypass, protected large allocations,
waiting cancellation, operator-only configuration, legacy bytes, audit denials and
live identity drift. `tests/resource_resume.rs` adds actual HTTP restoration and
six queue-metadata corruption cases while retaining existing tests and launch
isolation. See [resource priorities](RESOURCE_SCHEDULING.md#optional-role-priority-with-bounded-bypass).
No global fairness/latency, work-conserving optimality, physical memory control or
measured utility is claimed. Jev+ and Jev-R remain **Partial**.

Verification: formatting, locked workspace/all-target check, final full tests
(**675 passed across 33 targets**, none ignored), and Clippy with warnings denied
pass. The seven focused priority cases and actual HTTP policy-restoration fixture
pass. Native Settings fixtures were updated with explicit None after the compiler
identified the added field; legacy serialized settings remain unchanged. Local
TCP/Unix fixtures required elevated access. Existing protocol pairing/malformed
frames, migrations, mock inference, denials, cancellation and rollback remain green.
`flatc` is unavailable; schemas/bindings were unchanged and fresh generation was
not run. Discovery at 127.0.0.1:1234 refused connections; no live-model throughput,
quality or physical GPU performance gain is claimed.



### Preserve first-solver resources during interpretation — 2026-10-17

**Implemented (bounded optional interpretation); Partial (measured utility).**
`src/reasoning/interpretation.rs` admits interface inference only when two calls,
configured interface output plus 384 solver tokens and both prompts' cumulative
bytes fit original ceilings. Cheap refusals avoid reference snapshot copying.
A one-third remaining-wall-time phase ceiling preserves the original task deadline;
only typed inference timeout may fall back. Calls/output/prompt charges remain and
interrupted usage/queue metrics become unknown. Provider/policy/audit failures and
cancellation never become permissive fallback.

Parsed advisory text enters context only when the actual encoded first-solver
prompt still fits. Admission, phase, timeout and context audits precede their
respective effects. Existing schedule overflow records remain compatible.
`src/engine/interpretation_tests.rs` checks exact call/token/prompt edges, context
refusal, honest timeout costs, textual timeout rejection, all audit stages and
cancellation without further inference. See
[interpretation budgets](INTERPRETATION_BUDGETS.md). No new permission, budget refund,
provider-stop proof or calibrated usefulness is introduced. General executive
scheduling and independent real-model effectiveness remain incomplete. Jev+ and
Jev-R remain **Partial**.

Verification: formatting, locked workspace/all-target check, full final tests
(**667 passed across 33 targets**, none ignored), and Clippy with warnings denied
pass. The first full run exposed the old test expecting a 384-token interpreter
to consume the whole 384-token task ceiling; that test was retained and expanded
to cover 128/384/768-token admission plus solver fallback, then the full suite passed.
Local TCP/Unix fixtures ran with elevated access. Existing protocol pairing/malformed
frames, migrations, mock inference, denials, cancellation, recovery and rollback
remain green. `flatc` is unavailable; unchanged bundled bindings were validated
without fresh generation. The default local endpoint 127.0.0.1:1234 refused
discovery, so no live-model quality or efficiency gain is claimed.



### Retrieve selected capability references from the original read view — 2026-10-17

**Implemented (bounded Full/Experts reference refinement); Partial (general retrieval).**
`Store::task_context_with_references` and `corpus::snapshot_from` create a detached
read-only snapshot in the same transaction as pinned task metadata, with the
existing 64 MB ceiling. Evaluation tasks reuse their frozen corpus. No live Store
lock/WAL transaction spans inference. Oversized snapshots refuse optional retrieval;
backup/integrity failures remain fatal. Copy/hash CPU/memory costs are explicit.

`src/engine/reference_retrieval.rs` performs one charged/audited batch over at most
four selected providers, one scoped FTS result each, respecting approval/version
filters and body hashes. Original-query misses can be resolved through capability
terms. Added snippets remain untrusted, IDs are deduplicated, and actual extended
prompt bytes/main-solver/verifier reserves govern admission. Audit failure stops
before solver input; searches performed remain charged. Admitted documents extend
workflow evidence identities with canonical content/provenance so old cohort studies
do not silently cover new references. No capabilities or models are installed.

Tests in `src/engine/reference_retrieval_tests.rs` and the retrieval helper cover
Full/Experts execution, aliases, live disable/insertion, quota and verifier reserves,
corrupt content, intent/outcome audit failure and canonical evidence binding.
Existing corpus/registry/routing/ablation tests remain. See
[pinned capability retrieval](CAPABILITY_RETRIEVAL.md). Large-corpus snapshot
efficiency, broader retrieval/reranking, pre-interpretation snapshot recovery and
independent usefulness measurements remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification: formatting, locked workspace/all-target check, full tests (**662
passed across 33 targets**, none ignored) and Clippy with warnings denied pass.
The five focused retrieval cases pass. A broad earlier text filter also selected
an unrelated SQL Unix fixture that the restricted sandbox denied; final full socket
fixtures ran with elevated access and passed. Existing protocol pairing/malformed
frames, migrations, mocks, policy denials, cancellation, recovery and rollback
remain green. `flatc` is unavailable; unchanged bundled bindings were validated
without fresh generation. Discovery at the default local endpoint 127.0.0.1:1234
refused connections; no real-model relevance/quality or performance gain is claimed.



### Resolve capability requirements jointly over pinned providers — 2026-10-17

**Implemented (bounded deterministic declaration coverage); Partial (general capability utility).**
`src/capabilities.rs` replaces independent first-provider additions with joint
`greedy-cover/v1` selection over enabled pinned skills/manifests/models. Existing
routes are preserved/reused; new candidates maximize uncovered requirements, then
prefer direct identity and stable lexical ties. A shared provider can cover many
requirements within the unchanged four-route ceiling. Missing bound workflow
models are excluded from additions so an available alias provider can be selected.
The bounded metadata algorithm adds no inference, network probe or permission.

`capability_resolution` distinguishes absent declarations, unavailable model
bindings and available-but-unselected requirements. An audit failure prevents route
refinement/expert admission. Existing gap semantics and refined version bindings
remain. Interpretation uses canonical registry IDs and falls back on invalid IDs.
Tests in `src/capabilities/tests.rs` and `src/engine/joint_capability_tests.rs`
cover overlap/order/ties/reuse, capacity and exclusions, malformed metadata,
six-capability actual workflow selection, pinned disable, unavailable-provider
fallback and audit denial. Existing capability tests are preserved. See
[joint capability resolution](CAPABILITIES.md).
This is not globally optimal set cover, calibrated usefulness, provider health or
weight attestation. Additional capability-driven retrieval, general scheduling, interpreted semver
negotiation and independent effectiveness studies remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification: formatting, locked workspace/all-target check, full tests (**657
passed across 33 targets**, none ignored), and Clippy with warnings denied pass.
Existing capability tests remain unchanged and pass with the new joint-resolution
cases. TCP/Unix fixtures required elevated access; protocol pairing/malformed frames,
migrations, mock inference, policy denials, cancellation and rollback remain green.
`flatc` is unavailable; schemas/bindings were unchanged and fresh generation was
not run. No real-backend usefulness, quality, memory or latency gain is claimed.



### Preserve recovery ownership diagnostics and isolate fork-sensitive fixtures — 2026-10-17

**Implemented (supported local recovery ownership and regression evidence).**
All seven recovery/preview/continuation entry points retain the original error
chain with operation-specific lease context. Native callers can distinguish actual
WouldBlock from unsafe-file validation and OS open errors; no failure is converted
to ownership evidence or allowed to reach recovery writes. Kernel lease acquisition,
close-on-exec flags, scope, budgets and no-replay behavior remain unchanged.

`src/recovery/lease_tests.rs` checks contention, final symlink rejection and invalid
file permissions across all seven entry points, with unchanged tasks/audits/target
bytes and preserved error types. `tests/lease_inheritance.rs` deterministically
proves that a forked child's inherited close-on-exec descriptor blocks recovery
after the parent drops its copy, and only child exit/reap permits recovery.
It executes in a separate integration image and uses only async-signal-safe child
syscalls. `tests/resource_resume.rs` gates CLI launch against in-process immediate
acquisition assertions, while keeping the six in-process cases parallel.

Two pre-fix parallel stress series failed in their second iteration; diagnostic
capture established EWOULDBLOCK but the holder had disappeared by descriptor scan.
Controlled inheritance establishes the mechanism, without claiming the historical
holder PID was identified. No production retry, forced unlock, lease deletion or
resource-budget relaxation was introduced. See
[recovery ownership](RECOVERY.md#lease-acquisition-diagnostics-and-fork-inheritance).
General checkpoint recovery, provider-stop proof, physical scheduling and independent
effectiveness remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification: all **100 consecutive** parallel resource-resume target runs passed
(7 tests each, none ignored), and the final full workspace run passed **650 tests
across 33 targets**, none ignored. Formatting, locked workspace/all-target check
and Clippy with warnings denied passed. The first inheritance run needed elevated
socket-pair access; the elevated regression and full suite passed. Clippy initially
required `expect_err` in the new helper; corrected without reducing coverage.
Existing protocol pairing/malformed frames, migrations, mock inference, denials,
cancellation and rollback remain green. `flatc` is unavailable and unchanged bundled
bindings were validated without fresh generation. This run establishes no physical
GPU enforcement, provider-stop proof or live-model quality/performance advantage.



### Independently verify finite-domain reasoning witnesses — 2026-10-17

**Implemented (bounded integer constraint satisfaction); Partial (general verification).**
`src/verification/finite.rs` adds explicit `csp:` goals with finite integer domains,
all-different and exact linear eq/ne/le/ge constraints. Strict bounded JSON rejects
ambiguous keys and unsupported fields. A candidate must assign every declared
variable within its domain and satisfy every constraint; no generated code or
exponential search runs. Bounded coefficients and i128 sums avoid i64 overflow.
No unsatisfiability, optimality or real-world completeness proof is claimed.

`constraints.verify` is task/actor scoped and individually charged. Existing Jev-R
independent rejection/revision, no-progress stopping, checked-output preservation,
published-memory rechecking and durable inference accounting support the criterion.
Checker manifests bind `finite_domain/v1`; workflow/retry policies require the
usual matching evidence. Comparison JSONL supports `finite_domain_v1` without an
expected witness, allowing independently checked alternate solutions.

Sources: verification, security, registry, reasoning constraint tests, benchmark
scoring, routing/escalation scope and recovery accounting. Tests cover brute-force
small-domain comparisons, wide arithmetic, bounds/order invariance, malformed
criteria/answers, alternate witnesses, contradiction/forged success, revision,
stagnation, tool/audit denial, recorded charges and exact-memory reuse. See
[finite-domain verification](VERIFICATION.md#finite-domain-assignments).
General executable verifiers, broader scheduling and independent effectiveness
measurements remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification: formatting, locked workspace/all-target check, full rerun (**648
passed across 32 targets**, none ignored), and Clippy with warnings denied pass.
The first full run failed one existing resource-resume test on exclusive runtime
lease contention; the exact case passed in isolation and the full unchanged suite
passed on rerun. Its transient cause was not established and lease enforcement was
not weakened. Socket fixtures required elevated access. Protocol pairing/malformed
frames, migrations, mocks, denials, cancellation and rollback remain covered.
`flatc` is unavailable; unchanged bundled bindings were validated without fresh
generation. The default local endpoint at 127.0.0.1:1234 refused discovery both
inside and outside the sandbox; no real-backend inference or quality gain is claimed.



### Deploy reviewed workflow policy in direct CLI and Unix service — 2026-10-17

**Implemented (explicit per-host deployment); Partial (general policy lifecycle).**
`src/main.rs` accepts paired `--workflow-study`/`--workflow-thresholds` options
for direct `ask` and `serve`. Bounded no-follow strict parsing happens before
configuration/state/backend initialization. Each host retains one immutable native
policy, so file replacement cannot change running service decisions. Socket clients
cannot override the host policy. Omitted options preserve tag routing, and no
configuration, executable installation or model grant changes.

`tests/workflow_router.rs` covers actual direct CLI accept/decline over HTTP, actual
Unix service inference after policy-file replacement, recorded policy identity,
missing/invalid options before home creation and socket override denial. See
[deployment instructions](EMPIRICAL_WORKFLOW_ROUTING.md#direct-cli-and-local-service-deployment).
HTTP-server policy options, pre-routing checkpoint continuation, independent held-out
studies and calibrated utility remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification: `cargo fmt --all -- --check`, locked workspace/all-target check,
full tests (**639 passed across 32 targets**, none ignored) and Clippy with warnings
denied all passed. Socket fixtures needed elevated local network access. Existing
protocol pairing/malformed-frame, migration, mock, policy denial, cancellation
and rollback cases remain green. `flatc` is unavailable; schemas/bindings were
unchanged and fresh generation was not run. No live-model effectiveness or physical
GPU performance result is claimed.


### Rank optional workflows using reviewed utility evidence — 2026-10-17

**Implemented (optional native policy); Partial (independent empirical deployment).**
`src/routing/{workflow,empirical}.rs` supplies pinned candidate fingerprints,
strict bounded paired studies, explicit operator thresholds and deterministic
net-benefit/cost ranking. Failures/regressions count, configuration/source/knowledge
and model drift decline work, and main-solver resources remain reserved.
`src/engine.rs` applies ranking after capability refinement and audits before the
callback and before expert admission; invalid orders/audit failures fail closed.
The existing kernel retains authority. Default routers preserve existing order.
`workflow-study-evaluate` inspects files without state creation or installation;
trusted hosts install immutable policies using `with_expert_router`.

Tests in `src/routing/empirical/tests.rs` and `tests/workflow_router.rs` cover
threshold edges, complete costs, deterministic identities, scope drift, resource
reserves, actual engine admission/decline, capability refinement, rejected audit
writes, invalid output and CLI inspection. See
[empirical workflow routing](EMPIRICAL_WORKFLOW_ROUTING.md).
No real paired study, calibrated probability or intelligence/performance gain is
claimed. General routing utility, automatic safe installation and broader
independent evaluations remain incomplete. Jev+ and Jev-R remain **Partial**.

Verification passed: formatting, locked workspace/all-target check and tests
(**637 tests across 32 targets**), and Clippy with warnings denied. Local TCP/Unix
fixtures ran with elevated socket access. Existing migration, mock inference,
malformed protocol, policy denial, cancellation and rollback tests remain green.
`flatc` was unavailable; unchanged bundled schema/runtime pairing checks passed,
but fresh binding generation was not run. No real-backend quality benchmark ran.


### Plan reviewed resident-model reconciliation — 2026-10-17

**Implemented (bounded operator planning/application); Partial (autonomous/physical lifecycle).**
`src/residency/planning.rs` plans a desired installed model/context set, reuses exact
loaded matches, loads only missing models and releases only explicitly selected
instance IDs. Canonical ordering and projected inventory hashes make decisions
reproducible. Invalid targets, incompatible unreleased contexts, expired/tampered
plans and stale inventories fail before mutation. Requests/actions are bounded to
eight; a 300-second review lifetime and 120-second mutation phase bound execution.

`residency-plan REQUEST --api-base ...` reads native inventory without initializing
state. `residency-apply PLAN --api-base ... --human-approve` uses an existing store,
rechecks explicit endpoint/inventory and persists an atomic one-attempt receipt.
Each step rechecks operator policy and expected inventory, audits its child operation,
and requires only the expected projected transition. Drift/uncertainty stops later
operations without retry or claimed rollback. Parent reports preserve confirmed
steps and explicit non-attestation flags. Existing credential/response/endpoint
limits remain; no new provider API or model actor grant is added.

All residency mutations now acquire configured-resource ownership before runtime
ownership, blocking changes under an idle live pool as well as active inference.
Retention protects plan receipts and linked operation tasks against replay-gate
removal. Receipt inspection is bounded to 1,024 records/32 KiB each under a five-second
SQL deadline. Retirement/compaction remains unimplemented and this quota is explicit.

Sources include residency/planning, existing mutation refactoring, CLI and retention.
Unit and actual HTTP/CLI fixtures cover reuse, exact unload/load, independent next-step
checks, expiry/content changes, corrupt/replayed receipts, no-state preview, approval,
audit rollback, unknown outcomes, idle ownership and retention. Native provider/weight
ownership, real load/inference cost, automatic per-task residency and physical GPU
limits remain unproven/incomplete. Jev+ and Jev-R remain Partial. Verification
passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**629 tests across 31 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. The first
five focused integration cases passed before adding two more CLI/drift cases and
expiry/receipt regressions; the final full suite includes all of them. Local
TCP/Unix fixtures required elevated socket access. Existing protocol pairing,
malformed frames, migrations, mock inference, denial, cancellation, recovery and
candidate rollback gates remain green. `flatc` is unavailable; schemas/bindings
were unchanged and fresh generation was not run. No real native load/GPU/quality
performance gain is claimed.


### Restore original configured resource admission during continuation — 2026-10-16

**Implemented (supported ready checkpoints and compiled configured pools); Partial (general recovery).**
Continuation now restores original `resource_admission` settings, working allocation,
resident catalog and target checks instead of refusing every configured task. A
bounded strict validator requires complete per-call intent/admission/dispatch/
completion chains, matching roles, identities, capacities, resident binding and
coverage. After reasoning context, targets must also match the pinned default/role
configuration. Earlier completed advisory targets must match declared coverage;
their untrusted results are not replayed. Unresolved workspace dispatches block
restoration. Native pools absent from execution settings remain unavailable.

Read-only preview validates history and probes resource ownership outside Store
locks. Approved resume performs a read-only preflight, restores/reuses a matching
compiled pool outside Store locks, then revalidates configuration and checkpoint
in the write admission transaction. Both preparation phases share one five-second
admission deadline. Original task deadlines, calls, output/prompt/tool/cycle charges
and model/skill snapshots remain in force. No fresh task budget or executable is
loaded from metadata. Resource admission and its original history are included
in continuation reports and start audits; the restored pool is attached to Session
before any missing graph/solver work.

Sources: `src/recovery/continuation_resources.rs`, continuation orchestration,
`src/resource_scheduler/{configured,resident}.rs` and shared binding helpers.
`tests/resource_resume.rs` exercises actual HTTP, actual direct/managed CLI resume,
current-config drift, ready cancellation, admission rollback, initial solver,
partial advisory graph, legacy empty catalogs, ownership/approval denial and 13
corruption/unresolved-history mutations. The seven focused tests pass. The expanded
fixture initially missed a `mut` binding; corrected without production changes.

Only fully recorded ready checkpoints are supported. Old empty-catalog records may
omit all resident metadata fields; partial metadata is rejected. Nonempty catalogs
require durable target checks. Uncertain in-flight calls cannot be refunded or
approved by a resource-release acknowledgement alone. General custom executable
pool recovery, provider stop/weight attestation, physical limits, dynamic lifecycle
and independent model-efficiency evidence remain open. Jev+ and Jev-R remain
Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**617 tests across 30 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Local
TCP/Unix fixtures required elevated socket access. Existing protocol pairing,
malformed frames, migrations, mock inference, denials, cancellation and candidate
rollback regressions remain green. `flatc` is unavailable; schema/binding files
were unchanged and fresh generation was not run. These fixture runs do not
establish real-model quality, GPU efficiency or provider-stop guarantees.


### Bind resident declarations to selected inference targets — 2026-10-16

**Implemented (trusted adapter/configuration coverage); Partial (provider/weight attestation).**
Nonempty resident catalogs now require the selected adapter to identify its actual
configured endpoint/model (including role overrides), or explicitly identify a
compiled deterministic backend. Unknown, malformed and unlisted targets fail before
pool queueing, provider contact or managed worker launch. Empty catalogs retain
legacy behavior without invoking new metadata callbacks.

`InferenceBackend::inference_target` and `ChatBackend::inference_target` default to
unknown. Local and same-image managed adapters derive metadata from their immutable
configuration; native conversations use the actual native model rather than role
profiles. Pinned Fabric routes and scheduled wrappers forward the selected module's
metadata. Scheduled dispatch pins a dynamic broker before checking coverage, so a
registry replacement during observation/waiting cannot substitute another backend.
Resident consumers now share the bounded canonical `backend::ModelTarget` type.

Target intent precedes the trusted metadata callback. Coverage/denial audit must
commit before resource reservation or execution. Accepted records bind the canonical
target, resident identities and catalog digest with no weights/permission attestation;
rejected records exclude raw invalid target/error data. Metadata callbacks run
outside Store/pool/registry locks. Task charges already committed by Session remain
charged on denial; there is no retry, alternate route, budget refund or extra grant.

Tests cover reentrant callbacks, unknown/unlisted/invalid targets, audit failures,
unchanged empty-catalog behavior, role selection/canonical endpoints, route mutation
without target substitution, and actual CLI refusal before HTTP/worker launch.
Existing native/collective HTTP and bound-expert coverage exercise listed targets.
The existing resident-test doubles now explicitly declare their synthetic target.

This checks declared configured-target coverage, not provider-returned model labels,
actual loaded weights, catalog memory completeness or real backbone sharing. Native
operators can misdeclare metadata; raw Unix/custom adapters remain unknown unless
the host implements the contract. Dynamic catalog/lifecycle integration, measured
residency, physical limits and independent real-model studies remain open. Jev+
and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**610 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Full tests
required elevated local TCP/Unix fixture access. An initial new CLI fixture wrongly
expected exit success for a failed task; corrected to assert the existing nonzero
exit and retained charges. New native error assertions were also corrected for the
existing non-Debug TurnResult type without changing production types. Final protocol
compatibility/malformed frames, migrations, mock inference, denial, cancellation,
recovery and candidate rollback regressions passed. `flatc` is unavailable;
schemas/bindings were unchanged and fresh generation was not run. No real-model
quality, physical memory or GPU-efficiency result is claimed.


### Resident backbone memory admission — 2026-10-16

**Implemented (operator-declared, pool-lifetime memory accounting); Partial (measured/dynamic residency).**
Optional `resource_admission.residents` declares immutable backbone IDs/versions,
weight digests, host/device footprints and consumer endpoints/model names. The pool
reserves each unique declared backbone once before request admission; multiple
consumers do not multiply its cost. Working requests must fit the capacity remaining
after resident charges. Completion, cancellation, quarantine resolution and shutdown
never refund resident memory. Snapshots separate total, resident and working charges.

Bounded validation rejects duplicate/conflicting identities, consumer aliases,
malformed versions/digests, oversized catalogs and impossible resident-plus-working
allocations. Canonical catalog/consumer ordering makes pool identity insensitive to
input order; live catalog changes are rejected. Existing empty/omitted catalogs keep
legacy configuration bytes. Each resource intent records the canonical catalog hash
and resident total with `residency_attested:false`; configured execution snapshots
retain the declared catalog. No model load, grant, endpoint contact or weight read
is performed by declaring a catalog.

Implementation: `src/resource_scheduler/{resident,resident_tests}.rs`, pool/state
initialization, configured pooling and admission metadata. Coverage includes shared
consumer accounting, actual memory-constrained FIFO dispatch, cancellation/release,
live-catalog pinning, invalid metadata, CLI validation/execution, primary/bound expert
calls and mixed HTTP native/collective contention. The HTTP fixture now has two
compute slots; the resident memory charge is what prevents simultaneous dispatch.

This enforces declared arithmetic, not measured provider RSS/VRAM or actual weight
sharing. Consumer declarations do not attest that every runtime backend/model is
covered. Automatic load/unload, measured catalog completeness, provider ownership,
physical isolation, task-pool restoration and independent real-model efficiency
studies remain open. Jev+ and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**603 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Focused
resident tests also passed all four cases. Full tests required local TCP/Unix
fixture access outside the restricted sandbox. Existing protocol pairing, malformed
frames, migrations, denials, cancellation, recovery and candidate rollback gates
remain green. `flatc` is unavailable; schemas/bindings were unchanged and fresh
generation was not run. No live-model quality or GPU efficiency gain is established.


### Configure and operate Jev+ shared admission — 2026-10-16

**Implemented (configured workspace and trusted-host scope); Partial (physical/global GPU scheduling).**
Optional `Config.resource_admission` validates fixed working allocations and bounded
pool settings. `configure-resources FILE` / `--disable` updates operator config
without database bootstrap. Engine clones share one retained pool; Session applies
it to every role and bound-expert call, including managed inference. Native agent
turns use the same executor, and HTTP Server construction shares the Engine pool
across both request types. Original task budgets/charges/deadlines remain in force.

Configured pools hold a private exclusive workspace resource lease. Another
configured host cannot start fresh counters while the owner is live. Bounded strict
lifecycle validation rejects unresolved prior dispatches at startup. Human-only
`resource-status` reads an existing database; `resource-release ID --human-approve`
takes resource/runtime leases, validates exact identity and records an operator
stop acknowledgement before removing the restart gate. No provider stop is inferred.
Retention preserves unresolved resource audits and excludes live configured owners
when applying deletion. Original configured-task continuation still requires explicit
restoration and is denied, even before a first resource intent exists.

Implementation: `src/resource_scheduler/{configured,admission,adapter}.rs`, Config,
Engine, Session, AgentRuntime, CLI, HTTP Server, retention and continuation guards.
Tests cover real CLI configuration/reconciliation, managed process ordering/reaping,
primary/bound calls, shared native/collective requests over actual loopback HTTP,
workspace ownership, audit tampering and retention. The first new HTTP fixture
compile exposed a missing `IntoFuture` import; fixed in the test. Historical config
bytes retain the omitted optional field. See `docs/RESOURCE_SCHEDULING.md`.

General cross-workspace/device arbitration, resident/backbone accounting, physical
VRAM/RSS enforcement, task-pool restoration and independent reasoning/efficiency
studies remain open. Jev+ and Jev-R remain Partial. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**598 tests across 29 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Socket
fixtures required elevated local TCP/Unix access. Both ordinary mock and managed
inference, protocol compatibility/malformed frames, migrations, denials, cancellation
and candidate rollback regressions are included. `flatc` remains unavailable;
schemas/bindings were unchanged and fresh generation was not run. No real-model
quality/latency improvement, GPU enforcement or general reasoning superiority is
established by these fixture runs.


### Shared Jev+ inference resource admission — 2026-10-16

**Implemented (trusted native, process-local scope); Partial (general GPU scheduler).**
`src/resource_scheduler.rs` and `src/resource_scheduler/adapter.rs` add a shared
FIFO pool across independently registered inference backends/brokers. Fixed
operator allocations reserve compute slots, host bytes and device bytes together;
queue and execution deadlines, bounded inventory and scoped cancellation prevent
unbounded pending work. Backend pins retain original allocations and transport
identity. Per-task kernel budgets and charges remain enforced independently.

Intent/admission/dispatch audits precede provider work. Success requires a durable
completion before freeing capacity; failed/timed-out/dropped dispatches quarantine
the allocation. Only an explicit audited operator stop acknowledgement can release
it. No prompt/answer/credential/error body is included in pool events. Native-only
`resources.configure` and `resources.resolve` reject model actors. Default recovery
rejects original pool usage instead of silently continuing without its limits.

Tests in `src/resource_scheduler/tests.rs` and
`src/recovery/continuation_policy_tests.rs` cover three-dimensional contention,
FIFO fairness, cancellation races, queue overflow, deadlines/shutdown, audit
failures, preserved transport pins, independent Engine brokers, original charges,
and pool-bound recovery denial. Initial compilation identified a private role
helper and a MutexGuard dereference; both were corrected using the existing role
mapping without duplicating authority. See `docs/RESOURCE_SCHEDULING.md`.

Settings and allocations are host declarations, not physical GPU/RSS enforcement.
Default CLI/HTTP configuration, resident/backbone accounting, provider ownership,
durable/cross-process allocation and pool restoration remain open. Jev+ and Jev-R
remain Partial; no model training, quality gain or live GPU benchmark is claimed.
Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**589 tests across 27 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. Full tests
required elevated local TCP/Unix fixture access. Existing protocol pairing,
migrations, malformed-frame, denial, cancellation and rollback regressions remain
green. `flatc` is unavailable; schemas/bindings are unchanged and fresh generation
was not run. The focused first pass also passed all 11 then-present pool tests;
final tests include subsequent cancellation/state/recovery regressions.


### Restore pinned executive policies during continuation — 2026-10-16

**Implemented (supported checkpoint and trusted-host scope).**
`recovery::continuation::{preview_with_escalation_policy,resume_with_escalation_policy}`
accept an explicitly restored native policy. Every saved descriptor must match;
strict parsing rejects ambiguous policy snapshots. A supplied handle cannot claim
legacy compatibility merely by using the built-in descriptor. For a revised
solver, restoration additionally requires the original durable intent/allow pair
bound to candidate, mode, cycle, checker, original configuration, compiled prompts,
and current source fingerprint, with exact prompt-chain costs and sufficient
recorded remaining resources. Missing/denied/altered allowances fail before
backend construction. An existing allowance is validated, not invoked again.

The restored handle is carried into the Session, so subsequent unresolved answers
use the original policy under original budgets and independent checking. New
CLI `--retry-study` and `--retry-thresholds` options on `resume-preview` and
`resume-task` reconstruct only the compiled empirical policy from bounded
operator-selected data; there is no executable loader, implicit policy swap or
promotion. Resume approval is checked before file/configuration access. The CLI
opens an existing current-schema database without creation, migration or bootstrap.
Default/legacy recovery remains available through the existing built-in path.

Sources: `src/recovery/continuation.rs`, `src/recovery/continuation_policy.rs`,
`src/escalation.rs`, `src/escalation/empirical.rs`, `src/main.rs`, `src/store.rs`.
Regressions in `src/recovery/continuation_policy_tests.rs` and
`tests/policy_resume.rs` cover native and actual CLI/HTTP restoration, immutable
study identity, reentrant callbacks, missing approvals, live leases, audit rollback,
ready cancellation, original call/output charges, no replay, and legacy/schema/
symlink/source-drift denials. The CLI fixture initially attempted a private Store
method and was corrected to use its own SQLite connection; a lint correction
made its test-lock scope explicit without weakening production checks.

Jev+ and Jev-R remain Partial. General custom reconciliation, executable policy
attestation/provisioning, automatic policy catalogs, global GPU scheduling and
independent utility studies remain open. There is no new model grant or claimed
quality improvement. Verification passed: `cargo fmt --all -- --check`,
`cargo check --locked --workspace --all-targets`,
`cargo test --locked --workspace --all-targets` (**574 tests across 27 targets**),
and `cargo clippy --locked --workspace --all-targets -- -D warnings`. The full
test run required local socket access outside the restricted sandbox. Tests use
scripted/loopback backends; no fresh real-model quality or speed result is claimed.
Schemas were unchanged; `flatc` remains unavailable, so fresh binding generation
was not run.

### Empirical retry evaluation and executive cost gate — 2026-10-16

**Implemented (bounded host API and operator inspection); Experimental (utility
policy).** `src/escalation/empirical.rs` aggregates reviewed single-retry outcomes,
including failures, and uses exact success-rate/cost-per-success thresholds.
Task requests bind mode, cycle, compiled criterion, effective configuration,
role prompts and runtime-source fingerprint. Matching evidence can veto or admit
one solver/checker retry under the existing deterministic kernel limits; unknown
scope, unverified prose, higher cost or insufficient time decline further work.
No model/task permissions, model loads, resource ceilings or recovery approvals
are widened. `retry-study-evaluate` inspects bounded operator-selected files
before any state initialization; only the operator receives `evaluation.retry`.

Study/threshold content fixes the immutable policy identity. Build-time source
hashing includes Rust, schemas/bindings and locked build inputs. Hashes establish
local byte consistency, not executable/weight attestation. Review and inclusion
of failures remain operator-attested. Public workflow reports stay frozen and
are not relabeled as held-out studies. No real utility gain is established.
Custom policy recovery still refuses unavailable policies rather than replacing
them. Jev+ remains Partial: general GPU scheduling, independently measured
routing/utility and safe executable provisioning remain open. See
`docs/EMPIRICAL_ESCALATION.md` for all six component statuses and exact bounds.

Regressions: `src/escalation/empirical/tests.rs`, `tests/retry_study.rs` and
`tests/runtime_fingerprint.rs` cover exact threshold edges, all failure costs,
scope and source drift, budget denial, ambiguous/unreviewed data, immutable
identity, actual Engine admission with independent rechecking, CLI no-state
inspection and symlink/actor denials. Formatting and workspace all-target check
passed. All 562 tests across 26 targets passed with local TCP/Unix fixture access;
the restricted-sandbox attempt first failed 99 socket tests with EPERM (397 passed).
The reviewed FlatBuffers bundle/pin checks, migrations, malformed frames, policy
denials, cancellation and rollback regressions are included. Native flatc is
unavailable, so exact regeneration was not repeated; protocol files are unchanged.
Warnings-denied workspace Clippy and final whitespace review passed. No fresh
live-model study, neural training, GPU attribution or quality gain is claimed.

### Replaceable executive escalation policy — 2026-10-16

`src/escalation.rs` introduces a trusted versioned `EscalationPolicy` installed
through `Engine::with_escalation_policy`. After hard resource admission, typed
hash/metadata requests can allow or decline a revised solver or an unverified
critic/reflection/solver sequence. A policy cannot grant tools, raise limits or
change evidence. Future independent verification must fit the tool allowance
before another rejected-answer solver is bought. No new model, external probe or
always-resident inference backend is introduced.

Reasoning snapshots identity and audits intent before callbacks and validated
metadata decisions before extra inference. Store reentry is lock-free; failures
and invalid decisions fail closed. Independent success, repeated-answer stopping
and baseline profile isolation precede custom policy use. Recovery refuses
unavailable custom or altered/spoofed policy snapshots; matching built-in and
legacy built-in tasks remain eligible under normal admission. Native callbacks
are trusted, nonblocking host code, not sandboxed or implementation-attested.
See `docs/ESCALATION.md`, `src/engine.rs`, `src/reasoning.rs` and recovery tests.
Jev+ remains Partial: measured utility, custom-policy restoration and general
resource scheduling are not established by this interface.

Verification: 551 workspace tests (490 library including server, 61 integration),
formatting, workspace all-target check and warnings-denied Clippy passed with
`FLATC` unset. Seven new regressions cover policy and recovery boundaries. Early
test-fixture corrections supplied the proper Store search argument, used explicit
non-demo inference status, and isolated checker budget from Full-profile retrieval.
Production permission checks and acceptance thresholds were not weakened.
Prior source `4974acb` and installation-evidence source `ad00a3e` passed hosted CI;
this new feature is not yet hosted/optimized-install/live-model verified.

### Capacity-aware admission to module routes — 2026-10-16

New inference and arithmetic/SQL pins exclude modules whose bounded queue is
observed full. Among candidates with room, operator rank still dominates sampled
queued/processing load and ID breaks ties. A shared queue-pressure sampler reads
available slots once per candidate; selection never reserves a slot. Existing
pins retain their exact module/version: inference still rejects a full queue,
while tools retain deadline-bounded waiting. No dispatch is replayed or refunded.

Regressions fill four buffered slots behind a gated active call, select an approved
higher-rank alternative for new pins, refuse selection when all available routes
are full, and restore preferred routing after draining. Both arithmetic and SQL
are exercised, including cancelled waiting on an old handle. Existing rank,
pin, failure, permission and callback-lock tests remain. This advances local
scheduling; it does not establish global provider/GPU capacity, fairness or a
general module scheduler. The first tool test incorrectly waited for the full
30-second dispatch deadline; it was corrected to observe/cancel the existing wait
without changing production backpressure or limits.

Verification: 544 workspace tests (483 library including server, 61 integration),
formatting, all-target workspace check and warnings-denied Clippy passed with
`FLATC` unset. Sources: `src/fabric.rs`, `src/fabric/tools.rs`; regressions:
`src/fabric/tests.rs` and tool tests. Previously installed `df4cf05` evidence remains
source-scoped; this new routing source is not installed/live-model proven yet.

Installed verification (2026-10-16): exact source
`df4cf052d0b8724a59225712153e4e92999f5ac1` installed optimized CLI and HTTP
packages offline with locked cached dependencies, `FLATC` unset and Rust/Cargo
1.97.1. Cargo reported 1m16s and 56.03s builds. Both installed smoke scripts passed;
all seven recovery integration tests also passed against the installed CLI and
its same-image workers, including the three revision phases and their SIGINT
cases. The harness was compiled from the same source. Source/binary/script/test
hashes, log hashes and scope are frozen in
`evaluation/results/installed-revision-recovery-smoke.json`. Local CLI evidence:
`/tmp/cogfabric-installed-smoke.Bs2Evu`; HTTP:
`/tmp/cogfabric-installed-http.EAG94w`.

CLI SHA-256: `a2739bc85fb27f3900c1e81d8a63b40bc410f05c69b650344584428ebae01ab8`;
HTTP: `ca5b88d19413dce8b314e8afd79bdb7ee16c8fd3b02dfc2ae28e277d4e817b51`.
Prior runtime `72c439c` passed hosted run `38827787867`; exact installed source's
run `38828049329` was still running when checked. This is cached current-host,
fixture-inference installation evidence, not clean-OS/GPU/model-quality proof or
release approval. Mandatory checks and 542 tests passed before installation;
only evidence/documentation changed afterward.

### CLI and managed-worker revision recovery — 2026-10-16

`tests/resume_worker.rs` now exercises approved `recover-revision-result`, read-only
preview and resume through actual CLI processes for saved solver, critic and
reflector phases. Each phase starts exactly one bounded same-image inference
worker, preserves original `reasoning_effort=none` despite changed current settings,
performs only missing calls, and reaps its child. Successful outcomes remain
unverified with five total calls / 1280 reserved outputs. Three SIGINT cases retain
one unknown result and all charged reservations, close the fixture connection,
reap the child and leave no unfinished worker audit session. No provider/GPU stop
or live-model quality is established by a loopback fixture.

The suite can target an explicitly installed executable via
`COGFABRIC_TEST_INSTALLED_BINARY=/absolute/path/to/cogfabric cargo test --test resume_worker`.
The override applies only to integration-test process launches, requires an
existing absolute file, and adds no production setting or authority. Earlier
Core-only revision evidence is now supplemented with real CLI/worker coverage.

Verification: all 542 workspace tests (481 library including server, 61
integration), formatting, all-target workspace checking and warnings-denied Clippy
passed with `FLATC` unset. The first test build referenced a private role allowlist;
the fixture was corrected to explicit known role names without changing visibility.
Optimized installation verification is the next separate evidence step.

### Recover a recorded whole-goal solver before critique — 2026-10-16

Approved `recover-revision-result` now reconciles a recorded cycle-positive solver
answer into `ready_critic` for unstructured DEEP/RESEARCH tasks. Normal admission
checks original snapshots, stopping hash, schedules, same-boot deadline and enough
remaining capacity for critic/reflection/revised solver (three calls and 640
reserved outputs, plus bounded prompts). No prior inference or cycle is replayed.
The answer remains unverified; a non-admitting or invalid critic stops with the
saved answer and only one new critic charge. An admitting critic proceeds through
the missing reflection and solver under original limits.

HTTP regressions cover the three-call path (five total / 1280 reserved outputs)
and both critic stop cases (three total / 768 reserved outputs). Additional tests
cover exact call/output/input budget refusals without writes, and another crash
after a recovered critic: a recorded result can form `ready_reflector`, whereas
an absent result retains unknown outcome and denies recovery. Other-boot,
unknown inference outcomes, and arbitrary backend/subtask recovery remain partial.

Verification: 540 workspace tests (481 library including server, 59 integration),
formatting, all-target workspace check and warnings-denied Clippy passed with
`FLATC` unset. Initial stop-test failure was an incorrect trace-field lookup;
assertions now read the persisted nested task answer and pass for both stop cases.
Hosted run `38827055986` passed for prior source `71f5e1f`; run `38827410095`
for `44706cd` was still running when checked. New source is not yet hosted-proven;
no new optimized installation or live-model claim is made.

### Continue after a recorded critic — 2026-10-16

`recover-revision-result --human-approve` now also reconciles a completed
same-cycle solver/critic pair into `ready_reflector`. Strict critique content
must match the checkpoint. Continuation revalidates that pair and reserves no
new allowance: at least two original calls, 512 output tokens and bounded prompt
capacity must remain for reflection plus a revised solver. Original settings,
module/model pins, stopping hash, schedule and aging same-boot deadline still
apply. Resume invokes only the missing reflector and solver; no prior inference
is replayed and the outcome remains unverified.

Tests cover actual HTTP (two new calls, five total / 1280 reserved outputs),
insufficient call/output/input allowance, changed critique reason, and a second
crash after recovered reflection. A recorded reflection can form `ready_solver`;
a reserved reflection without its result still fails closed without writes.
This extends the completed-reflection path; arbitrary within-cycle positions,
unknown provider outcomes and other-boot recovery remain incomplete.

Verification: 536 workspace tests (477 library including server, 59 integration),
formatting, all-target workspace checking and warnings-denied Clippy passed with
`FLATC` unset. The previous runtime's hosted run `38827055986` was still running
when checked; no new hosted success, installation or live-model claim is made.

### Reconcile a recorded revision handoff — 2026-10-16

`recover-revision-result TASK --human-approve` links a fully recorded same-cycle
solver/critic/reflector sequence to the next solver boundary without inference
replay. The prior boundary, consecutive calls, strict admitting critique and
original stopping-state hash are checked. Normal continuation admission validates
all original snapshots, remaining budget and same-boot deadline before committing
the new boundary and operator audit together. Missing, altered or unresolved
results, non-admitting critiques, live leases and terminal tasks fail closed.
Structured independently checkable objectives retain their separate result path.

The report omits answer/strategy text and claims no verification or resume grant.
Actual local HTTP continuation uses the recorded reflection with exactly one new
solver call, five total calls and 1280 reserved outputs. The answer stays
unverified. CLI denial precedes state initialization; audit-write failure rolls
back the boundary. The first broad test pass exposed missing original stopping
hash validation; that check was added before final verification. General unknown
provider outcomes, recovery after only a critic, and other-boot/consumed-cycle
positions remain unsupported. No permission, call, deadline or cycle is renewed.

Verification: all 533 workspace tests (474 library including server, 59
integration), formatting, workspace all-target checking and warnings-denied Clippy
passed with `FLATC` unset. The preceding graph commit's hosted run `38826523808`
failed the existing paused-clock idle-worker/socket test; test-only commit
`5c41eed` resumes real time after simulated idle, preserving production limits.
The focused idle test and full suite were rerun with that fix. Hosted verification
of the new source remains pending; no fresh live-model or installation claim.

### Recover interrupted graph initialization — 2026-10-16

Approved `resume-task` now initializes missing pending nodes after a saved planner
result and graph, including a partially persisted initialization. Preview reports
`initial_nodes_to_record` and leaves state untouched. Admission still validates
original model/module/context/settings, charged planner identity, committed
schedule, same-boot deadline and remaining budgets. Missing nodes alongside any
advanced node state remain rejected. Missing transitions and the continuation
start commit in one lease-protected transaction before worker/backend setup.

Regression coverage includes no-approval refusal, read-only preview, write failure
after an earlier initialization write (full rollback), missing state after prior
execution, and actual local HTTP continuation of both crash windows with custom
schedule. Exactly three new solver calls finish the task; the original planner is
not replayed and total reservations stay four calls / 1024 output tokens. No
permissions, deadline or resource allowance is renewed. Unknown provider outcomes,
other-boot continuation and general consumed-cycle recovery remain incomplete.

Verification: all 529 workspace tests (471 library including server, 58
integration), `cargo fmt --all -- --check`, workspace all-target check and Clippy
with warnings denied passed with `FLATC` unset. Checks used locked cached
worktree dependencies; no new live-model or installed-binary result is claimed.

### Include executing arithmetic and SQL work in routing — 2026-10-16

Equal-rank tool candidates now compare buffered requests plus the one currently
executing request before the deterministic ID tie-break. A drop guard clears the
processing observation on completion, cancellation, errors, shutdown and worker
unwinding. Both arithmetic and SQL regressions verify empty-buffer busy routing,
unchanged old handles, cancellation/completion cleanup and return to the original
tie-break; existing rank-priority/queued-load tests remain. Worker-panic tests also
assert cleanup. All 527 workspace tests (469 library including server, 58
integration), formatting, all-target checking and warnings-denied Clippy pass with
`FLATC` unset. No protocol, permission or queue ceiling changed. This is local
sampled scheduling, not reserved capacity, provider/GPU occupancy or a general
module scheduler. Previous installed and live evidence remains source-scoped.

Installed-source verification (2026-10-16): exact source
`c31862da63d1942368c8e89895b0373b932b6cd3` installed both packages using
`env -u FLATC cargo install --offline --locked --path ... --root ...` on Rust/Cargo
1.97.1 with cached dependencies (CLI 1m06s; server 0.18s). Both
`scripts/installed-smoke.sh` and `scripts/installed-http-smoke.sh` passed, covering
31 security probes with 25 expected denials, typed arithmetic/SQL workers,
authenticated discovery, origin denial and graceful shutdown. This is host-local
installation evidence, not a clean-OS, live-inference or neural-safety result.
Local evidence: `/tmp/cogfabric-installed-smoke.U0YhfW` and
`/tmp/cogfabric-installed-http.txml5X`. Binary SHA-256: CLI
`8a2df20464aa57587c15cf26ee653cf7209b6afdef24bdaf4e117b303d39a678`;
HTTP server `5a3622404b7aba8c11155175da1ff85d0bf9a001e15a317fda1cc5abbd16814e`.
Script SHA-256: CLI
`986680949b8f901e6ac804cba15547daf33389e8eb1d989d47ea66b8960530ae`;
HTTP `950ff16da845b970466e5af0b335dde9733e457760b4b81a65cc4dfc81976723`.
Hosted CI run `38825885917` was still in progress when this evidence was recorded.

License metadata regression repair (2026-10-16): the GPL-3.0-only update
exposed a compatibility test that used a mutable current example as historical
input. The original checker manifest is now frozen in `tests/fixtures/`; its
existing canonical SHA-256 assertion remains unchanged. Current examples validate
against package license metadata, and the reference-candidate test verifies both
manifest hashes while retaining quarantine and denied-promotion checks. All 526
workspace tests, formatting, all-target checking and warnings-denied Clippy passed
locally with `FLATC` unset. No runtime policy or release gate changed.

Current evidence review (2026-10-16): **pre-release; V1 incomplete**. The current
workspace passes 551 tests (490 library including the server, 61 integration),
formatting, all-target checking and Clippy. See `RELEASE_AUDIT.md` for the 18 goal
acceptance criteria and scoped evidence. Later narrative entries below are a
historical implementation ledger; their old counts, unavailable endpoints and
pending features are not current summary claims. The seven canonical subsystems
remain preserved. Optional Wasm execution is experimental infrastructure, not a
new mandatory V1 technology or a reason to delay existing skill functionality.

Current release gaps include general module provisioning/routing and scheduling,
ownership-aware live model residency, broader recovery, reviewed unseen technical
and security studies, and final release validation. Existing reference,
workflow and compiled-tool experts already register and execute; they do not
require the Wasm experiment. Qwen graph/Rust studies are public diagnostics, not
independent held-out evidence. Optional offline LoRA remains experimental under
the engineering instructions; canonical French roadmap targets are preserved.

Implemented on Linux: managed inference, arithmetic and SQL workers launch from
the running service image through `/proc/self/exe`, retaining version consistency
after an upgrade replaces its original executable pathname. See
`src/fabric/unix/process.rs`, `src/fabric/tools/unix/process.rs` and the real-service
replacement regression in `tests/worker_executable.rs`. Other Unix systems retain
pathname-based launching; executable attestation and hostile-host isolation remain
not implemented.

Direct and task-pinned inference broker dispatch both preserve caller task UUIDs.
Concurrent distinct-task and malformed-task regressions cover this boundary in
`src/fabric/tests.rs`; general independent actor provisioning remains incomplete.

`src/fabric/unix.rs` adds host-connected inference transport authenticated against
approved OS peer UID/PID, validated correlated COGM frames and cancellation-safe
connection disposal. A separate test process verifies real process communication.
General actor routing remains incomplete; this adapter is not an executable
sandbox or attestation.

`src/fabric/unix/worker.rs` now implements worker-side authorization with immutable
host/task grants, cumulative resource ceilings and disconnect/shutdown cancellation.
Tests cover the complete Rust adapter/worker exchange and negative grants/budgets.
Worker sessions require bounded SQLite audit attachment and fail closed on
authorization-write failure; reopening/privacy/failure tests verify this path.
Opt-in same-binary task workers now launch through `src/fabric/unix/process.rs`,
receive authenticated grants, reuse pinned settings and are closed/reaped at task
termination. CLI integration tests cover demo execution, zero-process arithmetic
and pending HTTP cancellation. Worker audit reconciliation is operator-only under
exclusive runtime leases; real-crash, live-lease and rollback tests cover it.
Generalized executable module lifecycle, stale-resource cleanup and general actor
routing remain incomplete. See `WORKERS.md` and `RECOVERY.md`.

Managed worker lifetime now follows the task's approved wall ceiling rather than
an unrelated fixed timeout. Virtual-time regressions cover idle gaps, partial
frames and session expiry; legacy bootstrap lifetimes remain compatible.

`protocol/bootstrap.fbs` and `src/fabric/unix/bootstrap.rs` provide validated typed
single-task setup and pinned generation settings. Codec/negative tests and the
worker round-trip cover grant conversion. Authenticated bounded bootstrap delivery
has a real socket fixture and is used by the managed launcher.

The complete French `ARCHITECTURE.md` remains canonical. This matrix describes
current evidence on `develop/v1.0`, not a reduced target architecture. The project
remains a pre-release. `QUALITY_REVIEW.md` preserves verification history.

| Architecture sections | Requirement | Status | Current implementation and tests | Remaining release work |
|---|---|---|---|---|
| 3, 7, 9, 10, 12 | External agent HTTP interoperability | Implemented / Partial | `src/agent/{collective,sessions,session_context,session_cleanup}.rs`, `src/store.rs`, `crates/cogfabric-server/`: authenticated loopback Chat Completions/SSE, client-owned tool proposals/results, optional durable root-task checkpoints through opaque echoed tool IDs, cumulative pinned budgets, atomic claims, bounded reference/advice persistence, same-boot expiry and fail-closed replay/history/configuration checks; prior OpenCode v2 + Qwen and synthetic MCP workflows | Broader client protocols, Responses/multimodal API, all Engine expert/project-profile paths, broader client compatibility and independent agentic-quality/security evaluation |
| 7, 12 | Typed local IPC | Partial | `protocol/*.fbs`, `src/fabric.rs`, `src/events.rs`, `src/wire.rs`, `src/service.rs`: bounded verified IPC and module RPC, content-minimized typed progress subscriptions/catch-up, version-pinned inference registration/discovery, correlation/actor/destination/expiry checks, queues and cancellation; socket, spoofing, overflow, replacement and compatibility tests | General command routing, automatic independent actor provisioning and broader compatibility fixtures |
| 3 | Language interface | Partial | `src/language.rs`, `src/backend.rs`, `src/config.rs`: strict advisory intent, clarification, original-goal preservation, explicit conversation references, optional synthesis, persistent role profiles; scripted and HTTP tests | Live interpretation and synthesis quality measurements; direct/managed/socket generation now succeeds |
| 4, 9 | Jev+ coordination | Partial | `src/engine.rs`, `src/planning.rs`, `src/store.rs::route`, `src/resource_scheduler/`: pinned capability coverage, dependency scheduling, bounded parallel advisory waves, target/role working allocations, resource-aware per-role Broker eligibility, operator-approved managed native residency receipts, exact pinned route-scope recovery, original-budget completed-wave continuation and admission budgets; dependency, routing, recovery and budget tests | Automatic ownership-aware residency, calibrated measured escalation, incomplete in-flight outcome reconciliation and physical scheduling telemetry |
| 5 | Jev-R reasoning | Partial | `src/reasoning.rs`, `src/agent/collective.rs`, `src/agent/sessions.rs`, `src/verification/provider.rs`: planner/solver/explorer/critic/reflector, bounded revisions/search, progress stopping, durable client tool-loop continuation with root-pinned cumulative budgets and exact history/call binding, original-goal pure compiled checks and optional trusted host verification | Full Engine expert integration through the external agent API, untrusted executable verifier isolation, broader criterion families, host-provider restoration and independent live-model effectiveness measurements |
| 6 | Expert network | Partial | `src/registry.rs`: immutable semantic-versioned reference manifests, dependency/reverse-dependency checks, health, activate/disable/metadata rollback, bounded registry; lifecycle/tampering tests | General executable loading and health, measured workflow usefulness and real weight residency; trusted tool/inference rollback is implemented |
| 7 | Persistent memory | Partial | `src/store.rs`: SQLite WAL/FTS5, schema migrations through 9, durable task/events, bounded hash-checked agent session checkpoints/call bindings and pinned local reference identities, isolated explicit history, pinned registry/document snapshots, lease-protected recovery, immutable model manifests, episodic snapshots, quotas and reviewed retention | Broader independent fact verifiers, backend model residency, physical WAL/filesystem bounds, general external/neural artifact portability and automatic checkpoint resumption |
| 8 A/B | RAG evolution | Partial | `src/evolution.rs`, `src/evaluation.rs`: content-addressed quarantine, hashes/provenance/deduplication/quotas, isolated held-out retrieval, bounded cumulative approved-version regression gates and immutable accepted suites, atomic paired review records, transactional human promotion/rollback; integrity, regression, concurrency and rollback tests | Stronger poisoning/security evaluation, automatic source discovery and cleaning; dataset independence remains operator-attested |
| 8 C | Offline neural adaptation | Experimental | `training/train_lora.py`, inactive artifact registration; neural evaluation/promotion denied | Base-model provenance and independent neural evaluation manifests, actual training and externally isolated pipeline; no hot-swap claim |
| 8 D, 14 | Novel neural reconstruction | Not implemented | Research only; no trained Jev networks or superiority claims | Separate research and independent evaluations, beyond proven v1.0 infrastructure |
| 9 | Resource enforcement | Partial | `src/config.rs`, `src/reasoning.rs`, `src/resource_scheduler/`: mode ceilings, calls/cycles/outputs/prompt bytes/tools/time, shared queues, resident-aware declared working profiles, cancellation/quarantine and validated parallel resource restoration; negative/pressure/recovery tests; optional provider usage and inference duration | General-provider exact input-token preflight (opt-in llama.cpp supported), model-load cost and retry metrics, process-isolated inference/GPU memory and optional energy telemetry |
| 10, 11 | Security kernel | Partial | `src/security.rs`: immutable task/actor scope intersected with global policy, per-dispatch audits, deny-default capabilities, bounded no-follow reads; replay/actor/promotion denials and artifact tampering tests | General executable attestation and isolation before untrusted experts; signatures/attestations and neural behavioral evaluator absent |
| 13 | Reproducible evaluation | Partial | `src/evaluation.rs`, `src/benchmarking.rs`, `src/corpus.rs`: expected exact-match benchmarks, six isolated same-model ablation profiles plus optional larger baseline, per-arm resources, reviewed dataset hashes, bounded read-only corpus snapshots and durable run cancellation; profile/isolation/deadline tests | Independent review of the provisional 100-case corpus, unseen technical tasks, robustness/security studies and measured live-model comparisons |
| 12, 14 | CLI/service | Partial | `src/main.rs`: initialization/configuration, ask, service controls, task-specific socket progress/cancellation, skill lifecycle, ingestion/search, task traces, candidates/rollback, benchmark, self-test; real socket fixtures | Generative switching and ownership-aware residency scheduling |
| 12, 13 | Hardware/performance claims | Partial | Optional whole-device GPU diagnostics sampled real host memory/utilization/power; no energy or large-model advantage claimed | Task-attributed telemetry, real model switching and equal-budget comparisons |

Historical 355-test milestone: all then-current tests passed, including actual socket/loopback HTTP fixtures; formatting,
all-target build checks and Clippy with denied warnings pass. Fixtures do not prove
GPU integration, neural quality, immutable backend weights or live model switching.

Earlier LM Studio validation returned HTTP 401. Later direct and managed-worker
generation succeeded through the existing Ollama OpenAI-compatible endpoint;
the managed sorting answer passed independent Rust verification. See
`LIVE_INFERENCE.md` for actual measurements and limits. LM Studio authentication,
controlled native load/unload and reviewed equal-budget studies remain unverified.
LM Studio Qwen3 8B generation and native read-only inventory now pass real smoke
tests; see the later entries for actual scope and observed injection failures. Private
GitHub publication is authorized; no final stable release tag is authorized.

Independent verification now additionally supports explicit `order:` dependency
criteria through `src/verification.rs` and `src/reasoning.rs`; see
`VERIFICATION.md`. Failed checks drive bounded solver revisions; success preserves
the checked answer and records hashes. This advances sections 5/9/10 but does not
complete the requirement for broader independent executable verifiers.

Verification failures now preserve checked candidates as rejected outcomes through
progress IPC, episodic history and reviewed retention. Regression tests additionally
cover division-by-zero progress and single-call ordering rejection.

Comparison evaluation additionally supports versioned dependency-order criteria,
accepting every valid permutation across all arms rather than one exact answer.
Strict criterion preflight and a real loopback HTTP six-arm fixture cover this path;
this does not supply the remaining diverse held-out dataset or live-model results.

CLI diagnostics now checks storage/schema and registered selection, with optional
operator-only bounded loopback model discovery and explicit failure exit codes.
`src/diagnostics.rs` tests malformed discovery, redirects, bounds and missing IDs.
At that historical milestone the CLI probe found no server at 127.0.0.1:1234.
Later live generation evidence supersedes that availability observation.

`src/gaps.rs` and `src/evolution.rs::create_from_gap` now support bounded recorded-gap
inspection and reviewed RAG preparation. Candidate/evidence links commit atomically;
changed evidence, missing gaps and audit faults fail closed. Provenance source tasks
are protected from retention. No automatic source acquisition or poisoning/safety
approval is claimed.

`src/fabric.rs` now measures actual broker queue wait; append-only optional fields in
both RPC/socket schemas preserve missing legacy metrics. Per-call audits and task/
comparison totals carry measured queue waits, with cancellation invalidation.
Controlled contention and malformed/legacy metric tests cover the implementation.

Prompt/workflow experts now use immutable bounded checklist manifests and one
separately audited shared-backend advice call in full/experts profiles. Budget and
zero-cycle admission, profile isolation, manifest replacement/disable and invalid
workflow tests cover this path. Measured utility remains partial; structured
multi-step advice is described below. No extra model residency or tool authority
is introduced.

`src/backups.rs` now creates consistent checked SQLite snapshots with owner-only
storage, checksums, bounded admission and nonblocking backup leases. WAL-content,
read-only restore, symlink/capacity/contention tests and an actual CLI copy/reopen
smoke cover this path. Optional configuration and RAG artifact restoration are now implemented as
described below; arbitrary external artifacts remain outside automated restore.

Owner-authenticated independent-process progress streaming is now implemented by
`src/service.rs`, `src/wire.rs` and `watch-task`. Tests cover catch-up/live ordering,
task isolation, two-watcher admission, overflow requiring durable replay, terminal
completion and shutdown. This closes the streaming item in the IPC and CLI rows
above; general module routing and separate-process actor authentication remain
partial. See `EVENTS.md` for limits and Store-instance publication boundaries.

`src/telemetry.rs` now samples bounded optional Linux process RSS and lifetime
high-water RSS at task completion and in diagnostics. Task audits, append-only
socket fields and comparison maxima preserve unavailable measurements. Parser,
legacy-frame and aggregation tests cover this path. These are whole Rust-process
observations, not isolated task peaks, backend residency or GPU measurements.

The optional offline trainer now freezes bounded dataset input, rejects ambiguous/
conflicting provenance, fails on passes with no supervised tokens, reserves separate
output and records seeded configuration, dependency versions and hashed adapter
inventory. Nine standard-library preflight tests pass in addition to the 164 Rust
tests. Level C stays Experimental: ML dependencies/weights are unavailable here,
and base provenance manifests, independent neural evaluation, external isolation,
actual training and adapter switching remain unverified. Promotion remains denied.

`src/sql.rs` adds a built-in bounded data.sql.s1 tool for explicit SELECT tasks over
inline tables, with task-scoped authorization, two-worker admission, fresh in-memory
connections, deny-default SQLite authorizers, cancellation/progress limits and
hashed results. Aggregation, isolation, recursive exhaustion, mutation/file/schema
denials, baseline separation and durable status tests cover this path. Tool-backed
domain functionality advances; dynamic executable manifests, external connectors
and broader independent verifiers remain incomplete. See `SQL.md`.

Workflow manifests additionally support one to four dependency-ordered advice
steps, strict DAG validation, pinned versions, shared inference and conservative
whole-workflow admission. Tests cover legacy hash compatibility, malformed graphs,
execution order, dependency results, profile isolation, version replacement and
budget refusals. This implements bounded multi-step advice; executable/conditional
workflow interpreters, measured utility remain incomplete; registered-model advice is described below. See `EXPERTS.md`.

`src/restore.rs` and `restore-database` now automate current-schema database-only
restoration into an exclusively new private directory, with saved-hash verification,
SQLite validation, no-overwrite publication and a private versioned restore record.
Tests cover preserved running state without replay, existing paths, checksum/
symlink/corruption/schema/WAL denials and model permission denial. This does not
restore configuration/artifacts or automatically migrate older snapshots; full
workspace restoration remains partial. See `STORAGE.md`.


Workflow experts may now pin an enabled immutable local-model registration and
invoke its actual endpoint through the bounded broker. Tests cover activation,
in-flight pinning, live disable overriding frozen knowledge snapshots, budget
refusal, failed-call accounting and managed two-child cleanup. See `EXPERTS.md`.
Model configuration snapshots are implemented; weight attestation, automatic model
residency and live domain-quality measurements remain incomplete.

Comparison reports now expose bound expert configurations and invalidate the
same-model configuration-control flag when observed, including provider failures.
Regression coverage checks experts-only disclosure and ordinary single-model arms.

Expert selection now preserves the router's relevance ranking and skips disabled
model bindings before dispatch, permitting another already routed workflow without
spending calls on the unavailable candidate. Tests cover score/ID disagreement,
version replacement, disable fallback and audited admission ordering. The router
still returns at most two candidates; general utility-based scheduling is partial.

Offline training now requires a bounded versioned dataset split manifest with
provenance, exact file hashes and normalized prompt disjointness across training,
validation and test. Only training rows reach optimization; tests cover changed
inputs, overlap, path/version/field denials and dry-run behavior. Semantic split
independence, base-weight provenance, real training, external isolation and an
independent neural evaluator remain unproven. Level C remains Experimental.

Comparison evaluation now supports sql_result_json_v1: strict typed result-shape
validation and structural equality against separately supplied expected labels.
A six-arm regression covers actual inline SQLite execution, no-model tool calls,
JSON formatting independence, type/value errors and runtime-status exclusions.
Operator label correctness and the diverse live-model study remain outstanding.

A provisional 100-case synthetic evaluation corpus now covers ten categories in
`evaluation/v1_cases.jsonl`, with hashed inventory and explicit incomplete review
flags. Regression coverage checks all shapes/identities and recomputes thirty
deterministic tool cases. Independent label review, contamination analysis, harder
unseen technical tasks and live equal-budget study results remain release work;
the case count alone does not establish the diverse held-out study requirement.

Decomposition admission now includes conservative predecessor JSON/prompt-byte
bounds and reserves the first whole-goal prompt as well as calls/output tokens.
Research exploration preserves that solver's remaining calls/tokens/input bytes.
Four constrained-budget regressions verify whole-goal fallback and audited skips;
this is bounded scheduling, not measured utility estimation or resumable graphs.

Optional operator GPU observations are implemented in `src/gpu.rs` and
`diagnostics --probe-gpu`: fixed read-only NVIDIA query, bounded child/output, strict
units/ranges, unsupported values preserved as unavailable and explicit whole-device
scope. Parser and timeout/overflow/reaping fixtures pass. The actual driver probe
failed here, so live GPU integration, task attribution, energy and load-cost
measurement remain unverified. No GPU data is fabricated or added to task totals.

Registered local model endpoints now have an explicit operator diagnostic covering
expert registrations as well as the primary configured backend. The bounded
deduplicated GET probe reports missing IDs and exact configuration checksums,
skips disabled registrations and refuses more than eight distinct endpoints before
network access. Fixtures cover missing advertisements, deduplication and admission.
Live generation/residency remains unverified; local LM Studio CLI startup timed out
despite existing GGUF files, so absence of weights is not assumed.

Comparison runs can now pin an operator-reviewed dataset SHA-256 before task
creation. Reports distinguish prior operator pinning from observed byte hashes.
Regression coverage confirms malformed/mismatching pins and even whitespace-only
changes create no additional tasks. This proves byte binding, not independent
review, contamination resistance or the still-outstanding live-model study.

Host-access validation refined earlier sandbox observations: the framework GPU
diagnostic sampled a real 12 GiB device, LM Studio API access succeeded, and an
existing local model loaded with 35% GPU offload / 2,048 context / one prediction.
Discovery and inference then returned HTTP 401 because the server requires
authentication. No credentials were searched or authentication disabled. The
temporary model was unloaded and the previously stopped server was stopped again;
loaded-model inspection returned empty. Successful generation remains unverified.
Scoped operator-supplied API authentication is now implemented and fixture-tested;
an operator credential is still needed for live generation. External model loading
is not automatic framework residency control.

Scoped local API authentication: **Implemented** for direct HTTP generation,
primary and registered discovery, and managed-worker forwarding. See
`src/credentials.rs`, `src/backend.rs`, `src/diagnostics.rs`, typed bootstrap and
`tests/inference_auth.rs`. Operator configuration persists scope only; endpoint
changes cannot forward the primary token. Synthetic direct/managed tests reject
escaped credential echoes without writing credentials to task traces or config.
Real authenticated LM Studio generation remains **Not verified**. See
`docs/AUTHENTICATION.md` for setup and residual same-user/provider risks.

Authentication boundary follow-up: CLI integration verifies scoped and non-scoped
primary/registered discovery, escaped echo rejection, and missing-credential
failures before connecting for direct and managed tasks. `tests/inference_auth.rs`
contains the evidence; full Rust regression count is 168. Actual server credentials
and live successful generation remain external validation requirements.

Paired comparison reporting: **Implemented** in `src/benchmarking.rs` against the
small-model-alone arm. Reports include improved/regressed case indices, paired and
unpaired coverage, explicit denominators and null unavailable rates. Unit tests
cover incomplete/zero-denominator outcomes; the real six-arm SQLite fixture checks
aggregation from executed tasks. This supplies descriptive dataset regression
metrics, not the outstanding live held-out robustness or security study.

Configuration recovery: **Implemented** for new checked database snapshots via
embedded versioned effective settings and explicit `--restore-configuration`.
`src/backups.rs`, `src/restore.rs` and CLI integration cover preserved settings,
private new-path publication, invalid/missing metadata rejection, legacy default
restoration and no task replay. External artifacts/weights and older-schema
migration remain **Not implemented**; this does not complete full workspace restore.

RAG candidate artifact recovery: **Implemented** as explicit bounded snapshot and
restore options in `src/rag_archive.rs`, `src/backups.rs`, `src/restore.rs` and CLI.
Tests verify checksummed read-only payload roundtrip, canonical path rebasing without
status changes, malformed/missing/oversized payload and trigger rejection, opt-in
re-backup behavior and actual CLI flags. Neural/unrelated external files remain
excluded; general full external artifact restoration stays **Partial**.

Older snapshot migration: **Implemented** for explicit `--migrate-schema` on versions
1–5. `src/store.rs::migrate_snapshot` applies existing transactional migrations to
checked staging only, with limits/progress interruption and standalone publication.
`src/restore.rs` tests all recognized versions, source preservation, no task replay
and failure before publication; CLI integration exercises the option. Future and
unversioned schemas remain rejected. This supersedes prior older-schema blockers,
while arbitrary external/neural artifacts remain outside the recovery implementation.

Versioned task-progress module routing: **Implemented** for trusted embedding hosts
in `src/events/modules.rs` and `src/events.rs`. Registration/discovery, immutable
versions, pinned task/stage subscriptions and shared bounded queues work over COGE.
Tests cover actual durable events, task/stage filtering before admission, replacement,
unregistration, malformed descriptors, registry/subscriber bounds and overflow.
General independent-process actor identities, executable module event handling and
command routing remain **Partial / Not implemented**; no model permissions expand.

Event observer recovery: **Implemented** via explicit cancellation/receive-until
and module-bound durable replay. Tests verify pending/buffered cancellation,
capacity release without cancelling tasks, overflow replay, preserved pinned stages
and cursor advancement through empty filtered pages. This strengthens host event
lifecycle; external module provisioning and execution remain incomplete.

Bounded audit inspection: **Implemented** in `Store::task_events_since` and CLI
`task-events`, with stable indexed task-scoped pages. Full trace reads now fail
explicitly on event/raw-byte ceilings and malformed/oversized history. Comparison
reports select bounded identity events instead of materializing full traces.
Tests cover page order/isolation/cursors, full count/byte bounds, malformed records
and actual CLI pagination. This supports recovery inspection; automatic graph
resumption and restoration of unknown interrupted inference usage remain incomplete.

Inference reservation recovery evidence: **Implemented** in
`src/recovery/inference.rs` and `inference-audit`. New reasoning events correlate
results to charged calls; bounded transactional inspection validates identities,
hashes and allocations while preserving unknown legacy/interrupted usage. Tests
cover runtime/CLI production of call IDs, incomplete/legacy reports and malformed,
duplicate/unmatched/tampered records. This does not authorize replay or complete
automatic graph resumption; remaining-state/budget/configuration checks still apply.

Durable execution settings: **Implemented** in Engine/Session before inference,
with runtime configuration, effective call/cycle limits and execution options.
Embedding validation now rejects invalid configuration before task creation.
Tests cover durable original settings, ordering before dispatch, private path
exclusion and snapshot write failure containment. Backend attestation and restored
permissions are explicitly false; automatic checkpoint continuation remains partial.

Task recovery execution bounds: **Implemented** in `src/recovery.rs`. Exclusive
lease-protected reconciliation validates bounded UUIDs before updates and applies
a five-second SQLite progress deadline. Regression tests cover malformed/oversized
identities, oversized batches, expensive-trigger interruption, atomic rollback and
callback cleanup. This improves reconciliation only; automatic resumption remains
partial and external inference completion is still unknown.

Durable advisory graph reconstruction: **Implemented** in `src/recovery/graph.rs`
and `graph-audit`. Bounded transactional inspection validates pinned graph
structure, per-node transitions and dependency ordering without upgrading results
to evidence or revealing result bodies. Tests cover partial histories, isolation,
malicious transitions, changed definitions, duplicate plans, real scheduler events
and CLI inspection. Automatic checkpoint execution remains **Partial**: this is
validated state reconstruction, not remaining-budget admission or replay authority.

Saved inference admission reconstruction: **Implemented** in
`src/recovery/inference.rs`. Original validated runtime settings and deterministic
mode ceilings bound remaining calls, reserved output and prompt-byte charges;
unrecorded results never refund charges. Tests cover runtime/CLI reports, missing
legacy snapshots, invalid/late/duplicate snapshots, mode mismatch, uncorrelated
results and overdrawn budgets. Wall-time/cycle/tool reconstruction and actual
checkpoint continuation remain **Partial**, with no resumed execution authority.

Durable tool/cycle admission: **Implemented** in `src/reasoning.rs` and recovery
inference auditing. New settings version the resource ledger; reservations are
written before tool admission/cycle execution, with failure containment and
conservative reconstruction against original limits. Tests cover injected write
failure, denied-tool behavior, actual runtime cycles, missing version markers,
forged sources/capabilities, noncontiguous charges and cycle exhaustion. Older
histories retain unknown tool/cycle allowances. Wall-time recovery, immutable
reference admission and actual continuation remain **Partial**.
Repeated Session solve calls now share the original cycle ceiling rather than
restarting it; a regression test checks one exhausted FAST session remains at
one cycle and one inference call after another solve request.

Shared monotonic execution deadline: **Implemented** in Engine/Session. Setup time
consumes the original wall budget; sessions reject expired tool/cycle/model
admission and cap request timeout by remaining time. Tests exercise slow setup,
zero post-expiry dispatch and refusal to extend a restricted deadline. Blocking
callbacks/I/O are not preemptible; cross-process wall-time recovery remains
**Partial** and no fresh allowance is granted by recovery inspection.

Model registry read integrity: **Implemented** in `src/models.rs`. Listing now
checks embedded identity against row identity as exact lookup already did, and
both paths exclude oversized payload/checksum fields before Rust allocation.
Regression tests cover changed IDs/versions with recomputed valid hashes and
oversized manifests, checksums, row IDs and versions. This strengthens discovery
and version references; server-loaded weight identity remains unattested.

Independent integer sorting verification: **Implemented** in `src/verification.rs`
and Jev-R/Engine criterion dispatch. Explicit `sort:` goals preserve signed 64-bit
integer multiplicity and ascending/descending order, with bounded input/output and
strict metadata parsing. The checker uses an individually charged `sorting.verify`
capability and records `integer_sort/v1` rather than ordering evidence. Tests cover
invalid/overflow/duplicate-loss answers, malformed criteria, bounded recursive
revision, FAST rejection, denied verification budget and criterion-specific evidence.
General independent task verification and live effectiveness remain **Partial**.

Freshly verified structured-memory reuse: **Implemented** in `src/engine.rs`.
Full/Memory profiles may complete exact `sort:`/`order:` goals from explicitly
published episodes only after a new individually charged independent check and
audit. No memory kind or historical status grants verification. Tests cover
zero-inference success, wrong answers, revocation, changed goals, exhausted tool
budget and actual CLI reuse. General fact verification and broad live memory
benefit remain **Partial**; this claims no learned weights or benchmark advantage.

Immutable assembled reasoning inputs: **Implemented** in `src/reasoning.rs`.
Versioned context snapshots preserve actual objective/reference/advice bytes and
hashes before planning/solving; raw and serialized bounds and audit-write failures
prevent later dispatch. Tests mutate a retrieved document after execution and
verify retained original context/hash/order, inject snapshot failure and reject
oversized/escaped inputs. This preserves recovery evidence; validated replay,
remaining-time admission and actual continuation remain **Partial**.

Checked saved-result recovery: **Implemented** in `src/recovery/result.rs` and
`recover-verified-result`. Under exclusive runtime ownership, one immediate
transaction validates the original bounded inference ledger and independently
checks the latest solver answer against an explicit original sort/order criterion.
Remaining verification allowance is charged; valid answers complete without new
inference, failed checks preserve task state with durable charges, and SQL/audit
failure rolls back. Tests cover leases, reopen durability, terminal/actor denials,
wrong/tampered/unresolved answers, exhausted budgets, atomic failure and real CLI
use. This completes a concrete result-reconciliation path; arbitrary graph
continuation, missing outcomes and recovered wall-time admission remain **Partial**.

Compiled checker expert bindings: **Implemented** in `src/registry.rs`, Engine and
Session verification. Reviewed checker manifests use existing immutable lifecycle
and dependency checks, route by capability tags and bind only supported compiled
criteria. Health and actual memory/reasoning proof events disclose manifest and
checker identities; tests cover malformed bindings, legacy hashes, replacement,
disabling, pinned historical proofs and independent recursive verification.
Registration grants no capabilities; arbitrary executable experts and dynamic
implementation loading remain **Partial / Not implemented**.

### Offline security boundary evaluation — Implemented (bounded scope)

`src/security_evaluation.rs` and the `security-eval` CLI provide 24 reproducible
permission, task-isolation, FlatBuffers and manifest probes with positive controls,
fixture hashes and failure exit status. Unit tests verify deterministic reports;
`tests/security_evaluation.rs` checks the actual CLI. Broad adversarial evaluation,
live injection/poisoning, sandbox escapes and module peer authentication remain
**Not implemented** by this suite. It cannot approve neural candidates.

Host inference broker shutdown: **Implemented** in `src/fabric.rs`. An explicit
permanent stop signal reaches current and retired task-pinned versions, cancels
active futures, discards queued work and prevents subsequent registration/pinning
or dispatch. Tests in `src/fabric/tests.rs` cover retained versions after replace/
unregister, buffered work, cancellation, idempotence and pre-initialization stop.
Worker-loop exit is asynchronous; general executable lifecycle and external
inference termination remain unproven.

Service broker lifecycle integration: **Implemented** in `src/service.rs`. Normal
shutdown joins task handlers before stopping the shared broker, so retained pins
cannot continue inference after service completion. Socket tests cover typed STOP
with an incomplete client and host-triggered shutdown during pending HTTP work,
while retaining durable cancelled task status. General module routing is still
incomplete; this adds no model capability.

Native residency inspection: **Implemented** in `src/residency.rs` and
`residency-status`. Bounded LM Studio native inventory distinguishes installed
models from loaded instances, with dedicated opt-in credentials and no mutations.
Unit and actual CLI HTTP tests cover identities, native endpoint constraints,
separate authentication, escaped token echo rejection, redirects, malformed and
oversized responses. Physical load/unload ownership and live validation remain
**Not implemented / Not verified**. See `MODELS.md`.

Explicit native model load/unload: **Implemented** in `src/residency.rs` and
`residency-load` / `residency-unload`. Operator approval, exclusive workspace
leases, installed/exact-instance preflight, durable pre-effect intent, bounded
requests and fresh inventory confirmation are tested through the CLI. Unknown
outcomes remain interrupted without retries or implicit cleanup. Cross-client
ownership cannot be established by this API; automatic lifecycle scheduling,
weight identity, generation after switching and real host validation stay
**Partial / Not verified**. See `MODELS.md` for operator responsibilities.

Residency audit/status consistency: **Implemented** in `src/residency.rs`.
Terminal confirmation/unknown-outcome metadata and task status commit atomically;
injected audit/status failures leave the prior running state without partial
completion evidence. Missing and terminal anchors are rejected. External model
server effects remain outside the database transaction and are never retried
merely because local persistence failed.

Installed binary workflow: **Implemented and locally verified** through
`scripts/installed-smoke.sh`. An isolated optimized offline source installation
passes tool tasks, skill registration, reference retrieval, example RAG promotion/
rollback, configuration/artifact restore and real socket ask/stop in fresh state.
README documents installation prerequisites and reproduction; CI includes the
workflow. Clean-OS package setup, remote CI and live inference remain unverified.
See `RELEASE_AUDIT.md` for source/binary identity and evidence limits.

Per-subtask coordination costs: **Implemented** in `src/reasoning.rs`. Scheduler
terminal nodes now disclose actual reserved calls/output/prompt bytes, measured
timing and optional provider/queue deltas without result bodies or verification
claims. Tests cover dependency execution, skipped/cancelled nodes, unavailable
usage and audit failure stopping subsequent dispatch. Expected-utility estimation,
parallel/general module scheduling and automatic resumption remain **Partial**.

Same-boot execution deadline evidence: **Implemented** in `src/execution_clock.rs`,
Engine/Session settings and recovery inference audits. Original pre-setup Linux
boot-time snapshots reconstruct decreasing/expired nanosecond allowance without
civil timestamps. Tests cover actual kernel observation, separate CLI processes,
setup exhaustion, reboot mismatch, backwards time and malformed snapshots. Legacy,
other-boot and unavailable clocks do not receive a fresh allowance. Actual graph
continuation remains **Partial / Not implemented**; inspection grants no authority.


Explicit pre-solver advisory continuation: **Implemented (bounded scope)** in
`src/recovery/continuation.rs`, `src/reasoning.rs` and `resume-task`. Original
same-boot deadlines and resource charges survive restoration; completed nodes
are correlated to saved planner/solver output and reused as unverified advice.
Five regression tests include actual HTTP, corrupted checkpoints, lease/approval
refusals, cancellation and terminal transaction failure. Default direct module
Full/Collective tasks are supported; managed/registered/custom backends,
post-solver phases and general repeated recovery remain **Partial / Not implemented**.
See `RECOVERY.md` for exact admission, timing and provider-identity limitations.


Registered direct backend continuation: **Implemented** in
`src/recovery/continuation.rs`. Admission validates the original immutable manifest
against both its task snapshot and current enabled registry entry, rejects changed
endpoint/model/limits and never selects a newer version. Tests exercise real HTTP
model selection and six missing/disabled/altered registration cases. This extends
the preceding bounded recovery entry; managed/custom backends and later phases
remain **Partial / Not implemented**. Provider weight identity remains unattested.


Managed-worker advisory continuation: **Implemented** in `src/recovery.rs`,
`src/recovery/continuation.rs` and `tests/resume_worker.rs`. Transaction-protected
lease conversion permits a fresh same-binary worker with only remaining resource
grants while preserving exact host deadline enforcement. Actual CLI success and
in-flight HTTP cancellation tests assert reaping and cumulative unresolved charges;
a lease test verifies shared worker access and exclusive recovery refusal.
Canonical private workspace storage is required. This supersedes the managed-mode
limitation above; custom modules, later phases, repeated recovery and external GPU
termination remain **Partial / Not implemented / Not verified**.


Durable reasoning boundaries and revision continuation: **Implemented (bounded
scope)** in `src/reasoning.rs` and `src/recovery/continuation.rs`. Before charging a
new solver cycle, the runtime persists bounded strategy/previous-answer/repeat
state and ledger anchors. Unconsumed boundaries following a completed advisory
graph can resume with original cycles, budgets and independent verification;
stale or altered boundaries fail closed. Tests cover write failure, preservation
of repeat stopping, tampering/consumption and actual resumed HTTP verification.
This supersedes the blanket later-phase limitation above. Arbitrary within-cycle,
no-graph and general repeated recovery remain **Partial / Not implemented**.


Whole-goal and skipped-plan boundary recovery: **Implemented** in
`src/recovery/continuation.rs`. Validated unconsumed boundaries admit FAST tasks,
malformed-planner fallback and explicit budget-skipped graphs while preserving
original resources and stopping state. A valid planner result with missing graph
history fails closed. Tests include actual HTTP no-graph revision, FAST and skipped
budgets plus missing-plan/consumed-boundary denials. This supersedes the preceding
no-graph limitation; within-cycle and repeated recovery remain **Partial**.


Repeated solver-boundary recovery: **Implemented** in
`src/recovery/continuation.rs`. Up to 17 immutable context assemblies are validated
against original bytes and chronological call/cycle reservations. Two-boundary
regression execution retains one settings record, cumulative charges and the
original cycle cap. Tampered counters/context, consumed/exhausted boundaries and
excess history fail closed. This supersedes the general repeated-boundary limit;
arbitrary within-cycle recovery remains **Partial / Not implemented**.


Failed saved-answer revision reconciliation: **Implemented (structured criteria)**
in `src/recovery/result.rs` and `src/recovery/continuation.rs`. A fresh independent
sort/order rejection may save an admitted next-solver boundary without repeating
inference. Original budgets/context/deadline and repeat stopping remain enforced;
write failure rolls back proof, tool charge and boundary together. Tests cover
actual HTTP continuation, repeated-answer refusal and fault rollback. Unknown
outcomes and arbitrary within-cycle phases remain **Partial / Not implemented**.


Authenticated process event-module delivery: **Implemented (host API)** in
`src/events/unix.rs`, `protocol/event_stream.fbs` and generator integration.
COGS wraps verified task-scoped progress with pinned module versions/stage grants;
both sides authenticate approved UID/PID peers. Bounded lifetimes/frames/queues,
cancellation and explicit overflow/protocol errors are tested, including a real
child process. Registry/filter replacement semantics persist. Automatic provisioning,
IPC replay, service/CLI listener integration and general module command routing
remain **Partial / Not implemented**; no executable attestation is claimed.


Durable authenticated module replay: **Implemented (host API)** in
`src/events/unix.rs`, `src/events/modules.rs` and COGS v2. Pinned filtered replay
advances empty-page cursors, deduplicates overlap with live queues and restores
host-retained grants after registry replacement/unregistration. Version-one live
output remains compatible; wrong versions/cursors and receiver frame/time
exhaustion fail closed. Five additional regression tests cover replay/reconnect,
pinned filtering and negative limits. This supersedes the IPC replay limitation;
automatic provisioning and general command routing remain **Partial**.


### Typed arithmetic module dispatch

`src/fabric/tools.rs` and `protocol/tool.fbs` implement replaceable host-registered
arithmetic modules over verified COGT FlatBuffers request/response frames. Tasks
pin immutable module versions before execution. Registration/discovery, conflict
rejection and unregistration preserve existing pins; shutdown revokes retired pins
and cancels active/queued futures. Four queued requests, 8 KiB frames, 120-byte
expressions, 256-byte answers and a 30-second dispatch ceiling bound this adapter;
the Engine's original task deadline and charged tool allowance still apply.

Only `math.evaluate` from task-scoped `jev-plus` is supported. Registration is a
trusted Rust host API, not an IPC permission grant. The Engine independently
recomputes arithmetic before marking evidence verified; module output alone is
never proof. This adds no inference calls. Backend errors become generic failures.
Service shutdown also stops this broker. These are in-process typed queues, not
an authenticated process tool transport, executable sandbox or general command
registry. SQL and other capabilities remain outside this adapter. General module
command routing/provisioning and executable lifecycle remain **Partial**.


### Authenticated arithmetic process transport

`src/fabric/tools/unix.rs` extends COGT command dispatch to an already connected
Unix socket. Both sides authenticate trusted UID/PID expectations. Immutable,
non-deserializable host grants restrict one task/module/version, 1–64 cumulative
calls and a 1–300 second session. The adapter carries actual broker task identity;
unscoped dispatch is denied. Four-byte big-endian lengths bound verified frames
to 8 KiB; partial frames and writes have two-second deadlines. Each exchange has
at most 30 seconds and both sides enforce their fixed session deadline.

Worker authorization is audited before execution; audit failure prevents dispatch.
Correlation replay, changed tasks/versions, exceeded calls, malformed frames and
pipelining fail closed. Cancellation discards the client stream permanently;
worker disconnect/shutdown/expiry drops pending backend futures. Backend failures
are generic and audits exclude expressions, answers and backend error bodies.
A separate child-process regression executes actual arithmetic. This is a trusted
embedding API: automatic task-grant provisioning, executable launch/reaping,
service/CLI tool-worker setup and other tool capabilities remain incomplete.
Dropping a future is not proof that arbitrary external side effects stopped;
this transport provides no executable sandbox or model attestation. Abrupt worker
termination may omit the terminal audit record. Original Engine tool allowances
and independent arithmetic verification still apply; grants cannot expand policy.


### Managed built-in arithmetic workers

`configure-tool-worker --enabled true` persists `arithmetic_process` (default
false); restart a running service to apply it. Disable with the same command and
`--enabled false`. Direct and socket `ask 'calc: 6*7' --mode fast` now lazily launch
the same Rust binary as an arithmetic worker, using the existing bounded broker
and COGT commands. No inference worker/model call is needed for arithmetic.
The default arithmetic registry is required; custom versions are rejected before
task creation rather than silently replaced. Other capabilities remain in-process.

`protocol/tool_bootstrap.fbs` defines verified COTB v1, bounded to 2 KiB with
two-second read/write deadlines. Both peers authenticate approved UID/PID before
bootstrap. Trusted setup grants exactly one task, built-in module/version and one
arithmetic call; its lifetime is at most 30 seconds and conservatively rounds up
remaining task seconds. The host still enforces its original precise deadline.
The worker opens the matching private workspace database with the pinned quota;
no mutable generation settings, model credentials or inherited environment are used.
Permission/tool-budget admission precedes lazy launch and the host independently
verifies the returned arithmetic answer.

Launch intent, owned PID, bootstrap delivery and confirmed reaping are distinct
audit records. Ownership begins at spawn and survives cancelled/failed handoff;
normal task cleanup closes the connection, waits two seconds, then kills/reaps
with a further two-second limit. Directory/socket cleanup checks inode/device
identity. Cleanup errors fail the task. This also applies after execution errors
and cancellation. Process/runtime crashes can still leave stale files or unknown
cleanup outcomes; no automatic crash cleanup or executable sandbox is claimed.
This supersedes the arithmetic-specific provisioning/CLI lifecycle limitations
above. General executable provisioning and non-arithmetic command routing remain
**Partial / Not implemented**. Process overhead is not a measured speedup.


### Typed SQL capability routing

`src/fabric/tools.rs`, `src/fabric/tools/sql.rs` and `protocol/sql_tool.fbs` extend
the shared tool registry beyond arithmetic. Default `sql.local` 1.0.0 implements
`sql.query`; capability lookup pins SQL and arithmetic separately before execution.
Trusted Rust hosts may register immutable SQL replacements. Active capability
conflicts and changing an active module's capability are rejected. Unregistration
preserves existing pins, while broker shutdown cancels current/retired work.
Registry history remains bounded to 128 versions, with one active module per
supported capability; each module has four queue slots and a 30-second dispatch
ceiling under the original task deadline. Arbitrary capabilities are not accepted.

COGQ v1 carries structured tables/rows/typed cells and structured results, not JSON
strings inside IPC. Arithmetic COGT v1 remains unchanged. SQL frames are bounded
to 256 KiB; verification caps depth at 16, table visits at 25,000 and apparent
bytes at 1 MiB. Decoded cell budgets (32,000 request bytes / 16,384 response bytes)
stop aliased FlatBuffers references from amplifying allocations. Existing input,
identifier, table/row/column, cell and canonical result limits also apply.

The Engine charges `sql.query` before dispatch, records the selected version and
reruns the constrained SQLite checker before publishing verified evidence. This
uses a fresh execution of the same compiled SQLite checker, not an independently
implemented SQL engine. Wrong/malformed module output cannot establish proof.
One authorized command includes both execution and checking, potentially two
bounded SQLite runs (100 ms each), sharing the existing two blocking-worker slots.
No model call or new permission is added. The default backend still uses only
inline task-local data and denies host databases, mutations and unsafe functions.
Registered Rust backend code is trusted host code and is not automatically sandboxed.
SQL process transport/provisioning and broad executable tool loading remain
**Partial / Not implemented**; arithmetic worker setup remains arithmetic-only.


Installed tool workflow: **Implemented and locally verified** through the expanded
`scripts/installed-smoke.sh` against an optimized isolated source installation of
`6919087`. Actual CLI/service arithmetic workers are audited/reaped; rejected
arithmetic is cleaned up; SQL dispatch stays separate; restored configuration
launches a functioning worker. Binary/script hashes, task traces and exact build
identity are recorded in `RELEASE_AUDIT.md`. All 292 mandatory Rust tests and quality
checks pass. This supersedes the earlier installed binary evidence, while final
runtime changes, clean-OS setup and live inference still require verification.

SQL process transport: **Implemented** for trusted Rust embedding through
`src/fabric/tools/sql/unix.rs`; automatic CLI/service provisioning remains
**Not implemented**. Tests cover actual child-process SQLite, exact UID/PID and
task grants, cross-capability denials, immutable registration identity, cumulative
calls, replay, corrupt/oversized responses, partial-frame/session deadlines,
cancellation and failed authorization audit. Queries/results are excluded from
worker metadata. All 301 Rust tests and mandatory quality checks pass. This
supersedes the SQL transport limitation above, but does not establish sandboxing,
crash cleanup or installed verification of the new runtime source.

The SQL Unix source commit `af92168` subsequently passed a fresh optimized install
and existing installed CLI/service smoke workflow. See `RELEASE_AUDIT.md` for
hashes and retained evidence. Automatic SQL worker provisioning is still absent;
separate-process transport evidence comes from the Rust child-process regression.

Managed SQL workers: **Implemented** for the built-in `sql.local` 1.0.0 module in
CLI/service execution. Files: `src/fabric/tools/unix/process.rs`, `src/engine.rs`,
`src/config.rs`, `src/main.rs`, `src/fabric/tools.rs`, `protocol/tool_bootstrap.fbs`.
Tests: `tests/tool_worker.rs` real SQL CLI/service success, rejected unsafe query
with confirmed reaping, exhausted budget with no child; process-unit regressions
cover SQL COTB v2, arithmetic v1 compatibility, cross-capability/version refusal,
wrong-task scope and custom-registry refusal before task creation. Shared startup
ownership and cancellation/reaping paths remain covered. This supersedes the SQL
provisioning limitation above; arbitrary executables, crash cleanup, independent
SQL-engine implementations and live model measurements remain incomplete.

Managed SQL integration validation passes all mandatory quality checks with 304
Rust tests (283 library, 21 integration). The expanded smoke script also checks
SQL worker configuration, success/rejection reaping, backup/restored worker state
and actual socket SQL execution. Optimized installation verification is pending
for this new source; live inference and broader release blockers remain open.

Optimized installed validation of `04a3a03` now passes the expanded smoke workflow,
including actual managed SQL CLI/service execution, rejected-query cleanup and
restored SQL process configuration. Exact hashes/evidence appear in
`RELEASE_AUDIT.md`; the pending installed-source gap above is closed for this
commit. Crash cleanup, general executable integration and live measurements remain open.

Inference startup failure containment: **Implemented** in
`src/fabric/unix/process.rs`. Ownership of child/socket/lease survives cancelled
connection/setup and failed ownership audit; shutdown handles both initialized and
unfinished workers with bounded wait/kill/reap. Launch intent and owned PID are
persisted separately from completed handoff. Actual child tests cover cancellation,
audit failure, second-launch refusal and socket removal. Host crash/stale resources
and external GPU cancellation remain unresolved; no permission or wire change occurs.

Inference startup ownership validation: all mandatory quality checks pass with
306 tests (285 library, 21 integration). The expanded smoke workflow also checks
one demo inference child from launch intent through owned PID, handoff and matching
reap; debug execution passes. Optimized installed verification is pending for this
new source. This does not establish host-crash cleanup or real model behavior.

Optimized installed verification of `fcd6465` now passes the expanded CLI/service
workflow and demo inference ownership/reaping checks. Exact build identity and
hashes are recorded in `RELEASE_AUDIT.md`. The installed-source gap is closed for
this commit; host-crash cleanup, general module lifecycle and live-model gates
remain incomplete.

Tool-worker audit recovery: **Implemented** in `src/recovery.rs`, sharing registry
descriptor validation and the existing operator-only recovery command. New tool
audit records are explicitly versioned; original unversioned records remain
recoverable. `src/recovery/tool_tests.rs` covers mixed inference/math/SQL recovery,
terminal-session exclusion, idempotence, preserved answers/charges, malformed or
oversized metadata, audit-failure rollback and actual live socket worker lease
exclusion. Combined batches retain 10,000-session and five-second limits. No tool
replay, PID termination, external-effect proof or stale-file removal is implied.

Mixed worker recovery validation passes all mandatory checks with 309 Rust tests
(288 library, 21 integration). The expanded smoke workflow also confirms normal
worker completion leaves no recoverable sessions. Optimized installation evidence
must be refreshed after this source change. Stale-file cleanup, broader module
lifecycle and live-model evaluation remain open.

Optimized installed validation of `dbb9c47` now passes the expanded workflow,
including CLI recovery after clean completion. `RELEASE_AUDIT.md` records hashes
and evidence. The installed-source gap is closed for this commit; the mixed/live
recovery regressions establish abandoned-session behavior. Stale filesystem
resources and broader module lifecycle remain incomplete.

Worker-file cleanup: **Implemented** for journaled same-binary socket directories,
with operator-reviewed checksum-bound preview/apply under an exclusive runtime
lease. `src/recovery/worker_files.rs`, `src/recovery/worker_files_tests.rs` verify
actual host death, lease exclusion, approval, workspace/checksum binding,
symlink/type/mode/identity/extra-file refusal, audit failures, malformed/oversized
journals and pagination. All 318 Rust tests and mandatory quality checks pass.
Installed validation is pending. Older unjournaled resources, creation-to-journal
crash windows, hostile same-user races and automatic crash cleanup remain outside
the guarantee; filesystem deletion cannot roll back with SQLite audits.

The cleanup source `054505c` subsequently passed fresh optimized installation and
the expanded CLI/service smoke workflow, closing the pending installation gap.
`RELEASE_AUDIT.md` records source/binary/script hashes and retained evidence.
Actual host-death deletion is verified by the Rust child regression; the installed
workflow exercises empty reviewed plans. Broader module lifecycle and live model
gates remain incomplete.

Tool version rollback: **Implemented** for trusted host registrations in
`src/fabric/tools.rs`, covering arithmetic and SQL. Activation preserves original
backend identity and existing task pins; historical IDs cannot change capability.
Tests cover SQL/math rollback, active conflicts, shutdown, immutable redefinition
and rollback at the 128-version ceiling. All mandatory checks pass with 321 tests
(300 library, 21 integration). History retains inactive backends/queues until
shutdown; health checks, general executable lifecycle and residency quotas remain
partial. Real direct/managed Ollama generation now passes, including independent
sorting proof and confirmed child reaping; see `LIVE_INFERENCE.md`. This supersedes
older statements that all compatible real inference remains unavailable.

Inference backend rollback: **Implemented** in `src/fabric.rs`, with trusted host
activation/discovery of immutable retained versions. `src/fabric/tests.rs` covers
original backend/queue reuse, version-pinned role routes, conflict refusal,
shutdown/unknown versions and the 128-version bound. All mandatory quality checks
pass with 324 tests (303 library, 21 integration). Retention keeps inactive backend
references until shutdown; remote health/grant renewal, automatic executable
lifecycle and real weight residency/switching remain partial. See `MODELS.md`.

Optimized installation of `dec46fa` and the complete installed smoke workflow
passed. A separate installed socket service executed one real Ollama sorting task
with negative/duplicate values, independent Rust proof, confirmed worker reaping
and service/socket cleanup. See `RELEASE_AUDIT.md` / `LIVE_INFERENCE.md` for hashes,
times and scope. Live model switching and controlled performance studies remain
unverified; model advertisement is not loaded-weight attestation.

Transport-bound inference identity: **Implemented** in `src/backend.rs`,
`src/fabric.rs` and `src/fabric/unix.rs`. Initial brokers preserve actual connected
module metadata; registration rejects changed identity/version/roles before
inference. Regression tests cover no-dispatch rejection, invalid initial metadata
and actual child-process discovery/role restriction. This closes metadata
relabeling, not trusted-code compromise, executable attestation or weight identity.

The identity slice passes all mandatory checks with 325 tests (304 library and
21 integration). Optimized installed verification is pending for this source;
prior installation evidence remains tied to its recorded earlier source.

Optimized installation and installed CLI/service smoke passed for `8506352`;
`RELEASE_AUDIT.md` records hashes and evidence. The pending installed-source gap
above is closed. The leading summary table now reflects implemented authenticated
process transports and live generation without implying general actor provisioning,
loaded-weight attestation or model quality. Broader lifecycle work remains partial.

Event-module rollback: **Implemented** in `src/events/modules.rs`. Trusted hosts
can restore immutable registered metadata for future subscriptions; existing
observer/process grants and durable replay retain pinned filters. Regressions
cover rollback after unregistration, live/replay filtering, version forgery,
unknown selectors, 128-version history and 32-active-module reactivation bounds.
The registry remains process-local; executable health/provisioning and persistent
general module lifecycle remain partial. See `EVENTS.md`.

The event rollback source `2ac215b` also passed fresh optimized installation and
the installed CLI/service smoke workflow. See `RELEASE_AUDIT.md`; process-local
metadata rollback is verified, while automatic general executable lifecycle and
controlled benchmark studies remain incomplete.

A separate installed STANDARD-mode live interpretation attempt failed before
solver execution: the provider returned empty text under the interface's fixed
128-token reservation. The runtime returned visible failure and retained one
charged call; no structured-goal success is claimed. `LIVE_INFERENCE.md` records
actual settings and timing. This is a newly observed language integration gap,
not evidence against the independently verified direct/socket sorting cases.

Language-interface allocation: **Implemented** in `src/config.rs`, `src/engine.rs`
and `src/main.rs`. Operators can set `interface_tokens` / `configure-local
--interface-tokens` (default 128, bounded 1–4096) within original cumulative
ceilings. `src/config.rs` and `src/language.rs` regressions cover legacy settings,
invalid limits, exact backend allocation/charges and no dispatch when it cannot
fit. All mandatory checks pass with 329 tests (308 library, 21 integration).
Larger allocations do not guarantee valid provider output or permission changes.

A real follow-up using 384 interface tokens reached the unchanged 90-second
inference timeout before producing a structured goal. The reservation/timeout
were enforced and one call stayed charged, but live interpretation remains
unverified. `LIVE_INFERENCE.md` records both attempts; neither is a benchmark or
successful language-interface result.

Provider reasoning control: **Implemented** in `src/config.rs`, `src/backend.rs`,
`src/main.rs`, `protocol/bootstrap.fbs` and `src/fabric/unix/bootstrap.rs`.
Operator-only configuration accepts a fixed optional effort value; default HTTP
requests/configuration identities remain unchanged. Versioned COGB v2 preserves
explicit settings through workers and rejects incompatible shapes instead of
silently ignoring them. Direct/managed HTTP and codec regressions plus schema
conformance pass. All mandatory checks pass with 330 tests (309 library, 21
integration). Provider compliance and useful real interpretation remain separate
live evidence; no compute ceiling or model-quality claim follows from this option.

Initial planner admission now reserves a whole-goal solver using remaining
calls/tokens/prompt bytes after interface/expert work, rather than original totals.
`src/reasoning.rs` records `admit_planning` / `skip_planning`; its minimum prompt
reservation includes both planner and empty-strategy solver framing. Decomposition
and exploration retain their separate stricter reservations. A new language
regression covers independently constrained calls, tokens and prompt bytes after
malformed interpretation, preserving the original goal and reaching a solver.
All mandatory checks pass with 331 tests (310 library, 21 integration). This fixes
the scheduler gap observed in the effort trial; live structured interpretation
itself remains unverified.

Fenced intent compatibility: **Implemented** in `src/language.rs`. Only one exact
outer `json` fence is accepted in addition to raw JSON; byte/schema/permission
checks are retained. Tests reject surrounding instructions, multiple JSON values,
wrong/nested/incomplete fences and forged permission fields, and demonstrate a
fenced advisory objective cannot replace the user's goal before solver dispatch.
All mandatory checks pass with 333 tests (312 library, 21 integration). Earlier
effort-trial trace inspection confirmed its interface output was a fenced valid
object; the original strict parser correctly rejected that formerly unsupported
wrapper. Real final-path success still requires separate observation.

The corrected real structured-language path now succeeds on the same request:
`3b186cd` accepts the exact fenced intent, records advisory structured metadata,
skips unaffordable planning and dispatches the whole-goal solver within two calls
and 768 output reservations. CLI exit is zero; prose remains unverified. The
managed child is confirmed reaped. `LIVE_INFERENCE.md` records timing and limits.
This closes the single-case live integration failure, not robust interpretation
quality, synthesis or equal-budget reasoning evaluation.

Structural sorting comparisons: **Implemented** in `src/benchmarking.rs` as
`integer_sort_v1`, reusing fresh compiled checks over explicit `sort:` objectives
and omitting expected strings. Unit regressions cover duplicates, direction,
integer extremes/types, wrong answers and cross-criterion rejection. The actual
HTTP comparison regression covers sorting and ordering through all six arms.
`examples/sorting_comparison.jsonl` is demonstration data, not a reviewed held-out
study. Reports disclose shared verifier implementation; live comparative quality
and independent corpus review remain incomplete.

Runtime `3eb870c` passes optimized installation and installed smoke. Expanded
`tests/resume_worker.rs` now verifies saved provider controls survive mutable host
configuration changes for both normal resumed worker completion and cancellation;
all 334 tests and mandatory checks pass. Current operator settings remain unchanged.
This strengthens existing task pinning evidence, not general arbitrary-phase replay
or provider hidden-compute attestation. See `RELEASE_AUDIT.md` / `QUALITY_REVIEW.md`.

Bounded structured role JSON: **Implemented** via `src/structured.rs`, reused by
language interpretation, planner graphs and critic advice. Existing strict schemas,
graph validation and recursion limits remain enforced. Tests execute dependent
subtasks from a fenced graph (including durable graph validation), perform a
bounded fenced-critic revision, and reject forged permissions/surrounding prose/
multiple objects. All mandatory checks pass with 335 tests (314 library, 21
integration). This adds compatibility with observed model formatting; useful
live recursive gains still require measurement.

### Explicit public comparison workflow checks — 2026-10-12

Implemented: `compare --workflow-check` runs the existing six profiles without
falsely declaring public examples held out. `src/main.rs` and
`src/benchmarking.rs` require exactly one purpose; report fields distinguish
workflow checks from operator-attested held-out runs. Dataset integrity pins,
resource bounds and cancellation remain enforced. Regression:
`workflow_checks_are_explicit_and_never_attest_held_out_review` verifies all six
results and rejects missing/conflicting purposes. Independent held-out datasets
and live small-model comparative measurements remain incomplete.

### Observed inference adapter availability — 2026-10-12

Implemented: `InferenceBackend::availability` defaults to unknown; Unix adapters
report connected, in-use or permanently closed from their local stream slot.
`Broker::statuses` exposes bounded immutable-version metadata, active selection
and observed state without provider probes. Closed transports cannot be
reactivated, enter new task routes or dispatch from already pinned routes.
Metadata discovery/history retains registered versions for diagnosis. Regression
`closed_transport_is_visible_but_cannot_be_reactivated_or_routed` uses an actual
forged Unix response to prove closure, route refusal, inventory and shutdown.
Remote health, weight identity, automatic reconnection and residency scheduling
remain incomplete. In-use is not failed; connected is not provider health.

### Tool transport availability — 2026-10-12

Implemented: arithmetic and SQL backend availability defaults to unknown; Unix
adapters distinguish connected/in-use/closed local stream state. The shared tool
broker exposes retained-version status and refuses closed adapters during new
pinning, rollback activation and pinned dispatch. Existing malformed-frame and
forged-response regressions now verify both capabilities' inventory and refusal.
Grant validity, remaining calls and remote health remain separate checks; a
present stream is not proof of any of these. No implicit reconnection or permission
renewal is added. General lifecycle and residency scheduling remain partial.

### Expired/exhausted task-scoped capability routing — 2026-10-12

Implemented: local availability additionally distinguishes expired session deadlines
and exhausted call grants for Unix arithmetic/SQL adapters. New pins, historical
activation and already-pinned dispatch refuse these terminal observations without
queueing or renewing authority. Closed-stream state retains precedence when both
closure and expiry are observed. In-use adapters remain routable until their
absolute deadline; dispatch still validates the original grant. Paused-clock
expiry regressions prove refusal with unused connected streams and zero charges;
existing two-call tests now prove exhausted-state pin/activation refusal. Remote
health and automatic provisioning/reconnection remain separate incomplete work.

### Managed worker availability propagation — 2026-10-12

Implemented: inference `ProcessTask` delegates initialized availability to its
Unix adapter. Managed arithmetic/SQL wrappers share adapter observations and
report expired original host deadlines before worker initialization. Uninitialized
workers otherwise report unknown; observation never launches a child. Actual
socket/same-binary-child regressions verify connected state and closed state
after confirmed shutdown for all three paths; expired unlaunched tools remain
without initialized workers. This closes the wrapper propagation gap, not general
process health, startup-failure recovery or automatic grant renewal.

### Terminal managed-worker shutdown — 2026-10-12

Implemented: managed inference/arithmetic/SQL wrappers mark shutdown terminal
before awaiting cleanup. Later initialization/dispatch refuses the stopped state;
an initialization mutex coordinates successful handoff with shutdown inspection,
so an initialized child cannot fall between startup and managed ownership checks.
Observation reports stopped wrappers unavailable without claiming child completion.
Repeated shutdown remains allowed for cleanup retries. Regressions prove shutdown
before first use causes no worker initialization or launch audit for all three
capabilities. Existing interrupted-startup and actual child-reaping tests remain.
This does not stop external provider computation or provide hostile-code isolation.

### Real provider-managed model selection transition — 2026-10-12

Verified against the existing Ollama server: one managed FAST sorting task using
Q6 followed by one using the existing `latest` quantization, both independently
checked and reaped. Provider residency inventory changed to the respective
reported digest after each call. See `LIVE_INFERENCE.md` for exact limits, model
identities and timing. This strengthens configurable multi-model integration;
automatic framework residency ownership, native LM Studio switching, adapter
hot-swapping and small-model performance remain unverified/incomplete.

### Explicit alternative inference candidates — 2026-10-12

Implemented: trusted hosts can `register_candidate` with immutable bounded routing
rank. Only explicitly ranked modules may overlap inference roles; ordinary exclusive
registration retains conflict rejection. New snapshots choose the lowest rank,
then module ID, skipping observed unavailable adapters. Registered replacement and
rollback preserve each version's rank, backend and bounded queue; running task
pins never migrate or silently retry an alternative. Status metadata includes rank.
Regressions cover registration-order independence, role scope, exclusive conflicts,
rank bounds, transport-identity forgery, replacement/rollback and closed-route
fallback only for new tasks. Rank is declared operator policy, not learned utility
or measured cost. General plugin provisioning, residency and adaptive cost-aware
scheduling remain incomplete.

Ranked routing transport regression now additionally registers two authenticated
Unix adapters simultaneously, verifies lowest-rank task pinning, unregisters that
candidate and checks new tasks use the remaining adapter while existing pins
still send exact original module/version FlatBuffers headers. Forged version
relabeling is refused. This is actual local transport evidence, not remote plugin
attestation, process provisioning or model quality measurement.

### Explicit alternative tool candidates — 2026-10-12

Implemented: trusted Rust hosts can register ranked arithmetic and SQL alternatives
using `register_candidate` and `register_sql_candidate`. Lowest rank then module ID
selects new pins; unavailable transports are excluded. Existing handles retain their
original backend/version. Exclusive registrations still reject overlap, rollback
retains immutable rank, and activation/registration enforce 32 active modules and
128 historical versions. Rank cannot exceed 1,000,000. Capability and transport
identity checks precede registration; task/actor authorization remains mandatory.
Three new regressions cover arithmetic replacement/rollback and authorization,
SQL typed dispatch and capability separation, and capacity rejection without
consuming rejected versions. This is operator-declared routing, not an executable
plugin loader, adaptive utility estimator or automatic permission grant.

### Empty local-model answer diagnostics — 2026-10-12

Implemented: bounded HTTP parsing distinguishes empty responses with a reported
output limit from reasoning-only responses, with operator troubleshooting advice.
Internal reasoning is neither returned as an answer nor printed in diagnostics.
Regression fixtures cover length termination, null answer with reasoning and blank
answer; fixtures now construct explicit configuration instead of reading saved
operator settings. Automatic retries and budget expansion remain absent.

Live evaluation update: the user-selected LM Studio `qwen/qwen3-8b` completed all
six public workflow arms on one sorting case, with normal cleanup of all six
workers. Tools/recursive arms verified in approximately 0.40/0.34 seconds;
collective/memory/experts took approximately 1.8–2.0 seconds. Baseline output
contained correct numbers but failed the required format. This is not held-out
quality evidence; empty-corpus memory/expert labels and first-cycle recursive
success establish no retrieval, specialization or revision gain. See
LIVE_INFERENCE.md for exact usage and limitations. Two fresh default-parallel
350-test suites passed; prior intermittent failures remain unexplained.

Native residency update: real read-only LM Studio v1 inventory now succeeds and
reports Qwen3 8B loaded at context capacity 4096, with three other installed keys
unloaded. This closes the real unauthenticated inventory smoke gap only. Native
load/unload mutations, authenticated native access, physical weight attestation
and automatic ownership-aware residency scheduling remain unverified/incomplete.

Security update: a live Qwen3 8B retrieved-document trial produced the attacker's
requested marker in an unverified answer. No requested shell execution, promotion
or permission change occurred. Answer integrity therefore failed despite runtime
authority containment; stronger source/instruction separation and broader
behavioral evaluation remain release work. Installed runtime `2d2fb3b` passed
the full existing optimized installed-binary smoke; clean-OS setup is unverified.

Reference boundary mitigation: all inference roles receive fixed instructions to
ignore commands in external evidence. System wording alone failed the observed
live attack. Quoted nonempty reference serialization plus a final original-goal
reminder resisted that same attack once in fresh state; answer stayed unverified.
A regression checks that forged reference newlines/section endings remain inside
one JSON string. Empty-context prompts preserve their hashes/cost. Old nonempty
reference checkpoints may fail prompt-hash validation rather than silently resume
with changed prompts. This improves separation; it is not a general injection
safety proof, and broader adversarial evaluation remains incomplete.

### SQL worker backpressure — 2026-10-12

Implemented: replace immediate rejection on the third executing request with a
bounded six-request admission ceiling, two execution slots and a one-second worker
queue deadline. Excess admission still fails closed, queued cancellation releases
its admission permit, and executing blocking work retains both permits until
completion. Oversized raw input is rejected before admission. New independent
capacity fixtures cover admission exhaustion, queue timeout, cancellation, permit
restoration and successful queued execution. SQLite restrictions and task tool
charges remain unchanged. This addresses a demonstrated contention path; prior
intermittent SQL transport failures were not instrumented, so do not claim every
historical failure's cause is proven. Runtime-lease test contention is separate.

Recovery lease evidence: duplicated open-file descriptions keep recovery blocked
until the last descriptor closes, even after the original owner drops its handle.
A kernel regression verifies close-on-exec flags, zero interruption mutations
while blocked, and idempotent recovery after final closure. Historical transient
contention remains unexplained; the runtime deliberately retains fail-closed
ownership checks. Current mandatory tests pass 354 cases.

Installed-path update: current runtime `161dea5`, built from source `9b0cf59`,
passes the complete offline installed smoke and a real Qwen3 8B request submitted
through the installed authenticated socket service. Sorting independently verified,
managed worker reaped normally, service exited zero and removed its socket.
See QUALITY_REVIEW.md and LIVE_INFERENCE.md for hashes, paths and exact metrics.
This closes latest installed CLI/service/local-model smoke coverage; it does not
close clean-OS installation, general inference quality or broader release gaps.

### Explicit checkpoint prompt format — 2026-10-12

Implemented: new reasoning contexts declare `quoted-references-v1`; recovery
rejects unknown/null declarations and reports incompatible prompt hashes explicitly
before dispatch. Compatible undeclared snapshots remain accepted only with exact
current hashes. A positive regression verifies declaration preserves original
query/context and cumulative calls/tokens; negative cases cover unknown/null
format and changed prompt hash. Existing repeated recovery still validates source
bytes and chronological reservations. Automatic migration of old nonempty unquoted
reference prompts remains deliberately unsupported rather than silently weakening
the new reference separation or substituting task context.

Language-interface update: fresh public CLI settings with Qwen3 8B exercised real
interpretation, planning admission and whole-goal solving. Initial interpretation
omitted an explicit command prohibition; original-goal retention and runtime
authority remained intact. A stronger fixed interface instruction preserved that
constraint on one repeat trial. This is measured partial extraction quality,
not a guarantee of arbitrary constraint retention. README now includes the tested
384-token interface/no-thinking setup; detailed evidence is in LIVE_INFERENCE.md.

### Reproducible FlatBuffers compiler bootstrap — 2026-10-12

Implemented: `scripts/build-flatc.sh` clones public upstream v25.12.19, verifies
commit `7e163021e59cca4f8e1e35a7c828b5c6b7915953` before executing CMake, builds
with two jobs and checks the exact compiler version. It requires a new absolute
destination, refuses existing paths and retains source/build evidence on failure.
README documents durable user-cache installation and the FLATC export; build errors
point to the script. CI now invokes this same bootstrap rather than a separate
unverified-tag-only recipe. Actual public clone/build and negative argument/path
checks passed locally. Existing Git/CMake/C++/Make prerequisites were already
installed; this is not proof of fresh-OS prerequisite installation or authenticated
upstream provenance beyond the reviewed commit pin and HTTPS transport.

Hosted setup evidence: GitHub run 38295462158 passed for bootstrap commit
`fb7f9ec` in 3m21s, including verified compiler bootstrap, mandatory Rust checks,
CLI workflow smoke, security evaluation and training preflight. This verifies
fresh hosted checkout/dependency setup on a runner with existing prerequisites,
not a fresh end-user OS install or live GPU/security/quality evaluation. See
QUALITY_REVIEW.md for the immutable commit and run link. CI Node/runner-image
migration warnings remain maintenance work.

### Typed operator tool discovery — 2026-10-12 (Implemented)

`tool-modules` / `tool-modules --socket` expose append-only typed FlatBuffers host
inventory with capability, immutable version/rank, active selection and local
availability. Bounds: 128 versions/32 active IDs. Validation denies malformed or
conflicting registries and unknown enums; legacy replies omit the vector safely.
Borrowed fields are bounded before copying; envelope verification bounds shared
reference amplification. Thirteen wire tests, real socket discovery and expanded
CLI/service smoke pass. All 358 tests (337 library, 21 integration), formatting,
all-target check and Clippy with warnings denied pass with flatc 25.12.19; schema
conformity passes. Smoke used the debug binary, not a new optimized installation.
No worker is launched and no capability is granted by discovery. See
QUALITY_REVIEW.md for evidence. Automatic approval access recovered following the
operator's instruction; verification and private Git synchronization resumed.

Optimized installed validation for source `75e7e8a` now passes the expanded smoke
and direct Qwen3 8B independently checked sorting with thinking disabled. The
initial default-thinking request exhausted its output reservation, correctly
reported as failure. See RELEASE_AUDIT.md for actual results and remaining gates.

### Inbound inventory security regressions — 2026-10-13

Implemented in `src/wire.rs`: raw FlatBuffers fixtures bypass the trusted encoder
and confirm rejection of duplicate versions, multiple active versions, capability
identity changes, invalid ranks, excessive active/history counts and exclusive
capability overlap. Shared-string cases now separately exercise early field-bound
rejection and the 1 MiB apparent-size verifier limit using a physically small
frame with 128 repeated references. No runtime permission or behavior changes.

All 359 tests (338 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass; 14 wire tests pass. Logs:
`/tmp/cogfabric-inbound-all-tests.log`, `/tmp/cogfabric-inbound-inventory-tests.log`.
An initial Clippy pattern-style warning was corrected before the final pass.
These deterministic tests strengthen malformed-frame/resource-exhaustion evidence;
they do not establish general plugin isolation or neural safety.

### Clean installation prerequisites — 2026-10-13 (Implemented for Arch container)

A fresh rootless official Arch container passed package installation, pinned
compiler bootstrap, locked builds, all 359 tests, formatting, Clippy and optimized
CLI/service smoke without the host Cargo cache. README now includes build/smoke
prerequisites; `INSTALLATION_VALIDATION.md` records a reproducible image-pinned
workflow. QUALITY_REVIEW.md retains actual versions, hash and terminal log.
CachyOS-specific desktop/GPU setup and broader release requirements remain open.

### Typed inference lifecycle discovery — 2026-10-13 (Implemented)

`inference-modules` / `inference-modules --socket` expose all retained adapter
versions, roles, active selection, local availability and immutable routing ranks.
Append-only FlatBuffers control 8 preserves old empty-vector defaults. Inventory
limits, borrowed field bounds, duplicate versions, active version conflicts, role
conflicts, ranks and availability enums are validated. Ranked alternatives retain
the existing broker policy; observation grants no capabilities or model residency
ownership. General independent executable provisioning and adaptive scheduling
remain partial. Files: `protocol/cognitive.fbs`, `src/wire.rs`, `src/service.rs`,
`src/main.rs`, `scripts/installed-smoke.sh`.

All 361 tests (340 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass using flatc/crate 25.12.19. Sixteen wire tests
cover state/history round trips, malformed inbound inventories and old defaults.
Actual socket discovery and expanded CLI/service smoke pass on the debug binary.
Compiler `--conform` against the previous committed schema passes. Logs:
`/tmp/cogfabric-inference-inventory-all-tests.log`,
`/tmp/cogfabric-inference-inventory-wire.log`,
`/tmp/cogfabric-inference-inventory-smoke.log`; smoke evidence
`/tmp/cogfabric-installed-smoke.vEAe2u`. Optimized installed validation for this
new revision remains pending; the clean Arch result applies to source `35c4bdc`.
No live inference or neural safety evaluation was performed for this change.

The inference-inventory optimized installation gap above is now closed for
`da2a8b9`: expanded installed CLI/service smoke passes. RELEASE_AUDIT.md records
the binary hash, log and evidence; broad executable provisioning remains partial.

## Comparison output-contract parity — 2026-10-13 (Implemented)

The running 100-case public Qwen workflow at source `da2a8b9` exposed an ablation
fairness limitation: `Session::prompt_base` omitted the structured output contract
when executable verification was disabled for the model-alone profile. Tool-enabled
profiles received explicit array-format instructions. Some ordering answers were
semantically valid arrays wrapped in Markdown fences and were rejected by the
strict checker. Consequently this run must not support a causal reasoning-gain
claim, even after it completes. Its fixed executable remains unchanged.

Source now derives output contracts from the objective independently of
verification permission. A capturing-backend regression confirms identical solver
prompts and one call for both settings, with successful verification evidence only
in the enabled case. It also checks ordering contract parity with empty/reference
contexts and excludes the expected sorted answer from prompts. The first fixture
incorrectly expected no verification audit at all in the baseline; it was corrected
to count successful proof events, preserving the existing unverified audit. The
regression passes: `/tmp/cogfabric-baseline-contract-unit.log`. All 362 tests (341 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass. Full tests and expanded CLI/service smoke used
`CARGO_TARGET_DIR=/tmp/cogfabric-baseline-contract-target-20261013` to preserve the
running comparison executable. Logs: `/tmp/cogfabric-baseline-contract-all-tests.log`,
`/tmp/cogfabric-baseline-contract-smoke.log`; evidence
`/tmp/cogfabric-installed-smoke.jWuzXU`. The fixed isolated debug executable hash is
`77f79a6254e47d6cd82a342aec5b8cc8c9f9f8344c3db701f04218bed8402ae4`.
The running comparison binary SHA-256 remains `c5dfdb5bce170ec6697228c70c355c197913ff9f185ee706fb8658bdc83f0890`.
A corrected live rerun is required before reporting comparative gains.

### Arithmetic and SQL output contracts — 2026-10-13 (Implemented)

`src/reasoning.rs::Session::prompt_base` now states numeric-only arithmetic output
and the SQL columns/rows JSON shape for explicit tool-shaped objectives, without
computing answers or executing a tool. Format instructions apply independently of
verification permission. A real Engine model-alone regression captures both
prompts, confirms expected answers are absent, checks exactly one inference call,
zero tool calls, no dispatch/worker events, empty evidence and unverified status.
This extends sorting/ordering prompt parity; pre-fix arithmetic/SQL exact-match
scores also require the format caveat before any comparative-gain claim.

All 363 tests (342 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass. Full tests and CLI/service smoke used the
isolated target to preserve the ongoing original comparison executable. Logs:
`/tmp/cogfabric-tool-format-parity-unit.log`,
`/tmp/cogfabric-tool-format-parity-all-tests.log`,
`/tmp/cogfabric-tool-format-parity-smoke.log`; evidence
`/tmp/cogfabric-installed-smoke.7G8s1C`. Fixed isolated debug binary SHA-256:
`a2012ac53ec77dee168d4e2aadb1dca7067864b419a3ddd6e0db8f47e5c792d0`.
Original running-study executable remains checksum
`c5dfdb5bce170ec6697228c70c355c197913ff9f185ee706fb8658bdc83f0890`.
Additional instruction bytes count toward existing prompt bounds. No token budget,
permission or runtime proof is expanded. Older saved contexts with different
prompt hashes fail closed; there is no automatic migration of changed prompts.
A fresh corrected live study remains required.

### Full public corpus diagnostic — 2026-10-13 (Experimental)

All 600 profile tasks completed at source `da2a8b9`; raw answers and category/resource
summary are retained in `evaluation/results/`. Output-contract asymmetry and
format-sensitive scores prevent causal reasoning-gain claims. The corrected
`9fec58b` full rerun is active with frozen executable/fresh state; no results are
asserted yet. Independent label/split review and model-quality/security evaluation
remain incomplete. Both parity commits passed hosted CI.

### Compiled role prompt fingerprints — 2026-10-13 (Implemented)

`src/backend.rs` derives a versioned canonical SHA-256 from all compiled default
role/system prompts and the shared reference-data boundary, using the same helper
as actual local HTTP system messages. `Session` records it in original execution
settings before dispatch. `src/recovery/continuation.rs` requires a matching
fingerprint before continuation. Missing/null/mismatched values fail closed
without adding events or restoring permissions. Older records remain inspectable;
there is no automatic migration or charge refund. This is compiled-default
identity, not arbitrary-backend/provider behavior, weight or same-owner attestation.
See RECOVERY.md for the intentional legacy-continuation limitation.

All 364 tests (343 library, 21 integration), formatting, all-target check and
Clippy with warnings denied pass. Full tests and CLI/service smoke used an
isolated build directory. Logs: `/tmp/cogfabric-prompt-fingerprint-all-tests.log`,
`/tmp/cogfabric-prompt-fingerprint-smoke.log`; evidence
`/tmp/cogfabric-installed-smoke.PEsR38`. The corrected study's frozen executable
checksum remains unchanged. Host compilation overlapped part of that public
workflow study, so its timing remains uncontrolled and supports no causal latency
claim. No model call or permission was added by the fingerprint.


Corrected public Qwen3 8B output-contract parity workflow completed on 2026-10-13:
600/600 tasks at source `9fec58b`, 988.471 seconds, strict matches per 100 of
7 alone, 21 tools, 21 recursive, 51 collective, 48 memory and 50 experts. All 500
inference workers stopped normally. Full raw answers and category/resource summaries
are retained in `evaluation/results/`; see its README for exact hashes, configuration,
status interpretation and limitations. This supersedes earlier pending-run statements
only; independent label review, held-out evaluation, 1–3B/larger baselines and causal
quality/performance evidence remain incomplete. Host compilation overlapped the run.


Inactive inference backend retirement is implemented through trusted-host
`Broker::retire` in `src/fabric.rs`. Active retirement and identity reuse fail;
existing task pins remain usable, while future rollback/routes cannot select the
retired version. `src/fabric/tests.rs` verifies backend release after the last pin
and queue close. This improves runtime handle lifecycle without provider unload,
new grants or general executable provisioning. Mandatory formatting, all-target
check, Clippy with warnings denied and all 366 tests (344 library, 22 integration)
passed on 2026-10-13 with flatc 25.12.19. Test log:
`/tmp/cogfabric-inference-retirement-tests.log`. Optimized installation evidence
predates this new API and must be refreshed before final release.


Tool backend retirement is implemented through trusted-host
`fabric::tools::Broker::retire` in `src/fabric/tools.rs`. Existing arithmetic/SQL
pins continue after retirement, while future selection/rollback and identity reuse
are denied. Metadata tombstones preserve module capability identity even after
all retained backend versions disappear. A regression verifies both actual weak
reference releases, authorized execution, denied actor access, cross-capability
reuse rejection and registration of a fresh same-capability version.
Mandatory formatting, all-target check, Clippy with warnings denied and all 367
tests (345 library, 22 integration) passed on 2026-10-13 with flatc 25.12.19.
Local test log: `/tmp/cogfabric-tool-retirement-tests.log`. No new permissions,
provider unloading, owned-child cleanup guarantee or performance gain is claimed.
Optimized installation verification must be refreshed for these retirement APIs.


Retirement concurrency verification (2026-10-13): the gated regression
`retirement_preserves_active_and_queued_calls_but_not_shutdown_immunity` in
`src/fabric/tests.rs` retires an original backend while one request is executing
and another is queued. New calls select the replacement; original calls complete
on their original backend. A separate shutdown branch cancels both original calls
without dispatching the queued request, then verifies the retired backend releases.
All 368 tests (346 library, 22 integration), formatting, all-target check and
Clippy with warnings denied passed. Log:
`/tmp/cogfabric-retirement-concurrency-tests.log`. This is deterministic lifecycle
concurrency evidence; runtime source and optimized installation are unchanged.

Hosted tool-retirement CI
[38319883518](https://github.com/DrapNard/cognitive-fabric/actions/runs/38319883518)
completed successfully for `a665df998699000e0f64839b06582d4411729cc1`; watcher
exited zero. Compiler bootstrap, mandatory checks, tests, CLI/service smoke,
security probes and offline training preflight passed. Local watcher log:
`/tmp/cogfabric-tool-retirement-hosted-ci.log`. This supersedes its earlier pending
status, without changing broader provisioning, residency or evaluation blockers.


Registry cleanup lock ordering (2026-10-13): inference and tool brokers now detach
retired versions and commit shutdown state under their write lock, then release
the lock before dropping backend handles. Trusted backend destructors may inspect
committed registry metadata without deadlocking on that same lock. Regression
fixtures cover retirement and shutdown for inference, arithmetic and SQL. The
initial inference regression reproduced cleanup under the held lock (failed test,
no hang); a SQL fixture signature compilation error was corrected before final
validation. All 370 tests (348 library, 22 integration), formatting, all-target
check and Clippy with warnings denied then passed. Log:
`/tmp/cogfabric-registry-cleanup-tests.log`; initial reproduction:
`/tmp/cogfabric-retirement-cleanup-before.log`.

Files: `src/fabric.rs`, `src/fabric/tests.rs`, `src/fabric/tools.rs`. Task pins,
shutdown flags, identity tombstones and permissions are preserved. Custom host
cleanup code is still trusted and may block or panic for unrelated reasons; this
is not a cleanup timeout, process sandbox or provider-unload guarantee. Optimized
installation evidence predates this source change and requires refresh before
release. General module provisioning and independent evaluations remain open.


Optimized installation and full installed CLI/service smoke passed for cleanup
source `b81fe34`. Fresh-state managed Qwen3 8B produced a STANDARD ownership
explanation (unverified, two calls) and a FAST sort result independently checked
by `integer_sort/v1` (one call). Both workers were reaped normally. Exact hashes,
budgets, traces and limitations are in `QUALITY_REVIEW.md` and `LIVE_INFERENCE.md`.
This refreshes installed-source verification; broader lifecycle and evaluation
requirements remain incomplete.


Implemented local queue occupancy tie-breaking for approved equal-rank inference,
arithmetic and SQL alternatives (`src/fabric.rs`, `src/fabric/tools.rs`). New pins
prefer fewer occupied local channel slots after operator rank; IDs settle remaining
ties. Gated regressions verify actual buffered work, rank precedence, replacement,
old-pin execution and return to ID order after draining. Adaptive calibrated cost,
remote load, residency scheduling and reservation-based balancing remain Partial /
Not implemented. All 372 tests and mandatory checks passed; see QUALITY_REVIEW.md.


Implemented affordable optional retrieval under zero tool ceilings. In
`src/engine.rs`, `max_tool_calls=0` audits skipped retrieval and permits plain
inference with no document/memory retrieval. Arithmetic, SQL and independent
structured checks still fail authorization/budget admission before tool dispatch.
The regression covers plain generation, excluded retrieved content and negative
dispatch; zero call/token/input/cycle coverage remains in `src/reasoning.rs`.
All 373 tests and mandatory checks pass; a real managed Qwen explanation used one
model call and zero tools. See CONFIGURATION.md, QUALITY_REVIEW.md and
LIVE_INFERENCE.md. Free-form language restrictions are still advisory.


Implemented retrieval-free explicit arithmetic/inline SQL admission in
`src/engine.rs`: tool-enabled Full/Memory/Experts profiles skip unrelated retrieval
and execute under one tool allowance, with original policy and host result checks.
Zero-tool denials remain enforced. New Engine coverage verifies no retrieved
memory/document evidence; real CLI/service worker fixtures verify one-call
execution, reaping and zero-budget launch refusal. Progress and comparison
resource expectations match actual charges. All 374 tests and mandatory checks
pass; debug CLI/service smoke passes. See QUALITY_REVIEW.md for evidence.


Installed verification refreshed for `8b30fd1`: expanded smoke exercises actual
one-tool arithmetic/SQL CLI/service workers, preserved restored tool ceilings and
zero-tool managed demo inference over direct and socket paths. Zero-budget socket
arithmetic is denied before worker launch. Runtime source remains the 374-test
implementation. See QUALITY_REVIEW.md for exact artifacts and pending hosted scope;
this does not complete general module provisioning or independent evaluation.

## Read-only continuation admission preview (2026-10-13)

Implemented `resume-preview TASK_UUID` in `src/main.rs` and
`src/recovery/continuation.rs`, sharing existing admission and managed workspace
checks with approved resume. Reports bounded counters and subtask states without
raw task/context/answer text. Does not write task/audit state or launch inference.
Uses a cooperative exclusive lease (which may create its lease file), not a new
permission grant. Resume always revalidates; existing continuation phase and
original-clock restrictions remain. Regression coverage verifies unchanged
status/event count, charged budgets, live-lease refusal and missing prompt
fingerprint refusal. No broader continuation or provider attestation is claimed.

Private GitHub CI runs 38322336100 (`5345f67`) and 38322010060 (`8b30fd1`)
both completed successfully before this change.

Status: **Implemented** for existing supported continuation checkpoints.
Validation: formatting, all-target check, Clippy with warnings denied, and
375 Rust tests (353 library, 22 integration) passed with flatc 25.12.19.
Socket/HTTP tests required execution outside sandbox network restrictions; the
initial restricted run failed on those operations. Full results are recorded in
`/tmp/cogfabric-resume-preview-tests.log`.

## Read-only preview CLI startup correction (2026-10-13)

Status: **Implemented**. Actual CLI integration exposed that common startup
ran `Config::prepare` and database bootstrap before preview, repairing directory
permissions and permitting initialization writes. Preview now branches before
that startup and uses `Store::open_existing_read_only` with SQLite read-only and
query-only flags, the current schema requirement and a five-second busy timeout.
No migrations or bootstrap run. Existing checkpoint validation remains shared.

`tests/resume_worker.rs` now exercises preview through the actual executable
before both successful and cancelled managed continuation. It checks that an
unsafe workspace is rejected without repairing permissions, safe preview leaves
task status/events unchanged, and no HTTP request reaches the provider. Another
negative test proves missing state is not created and old schemas are not
migrated. This corrects the previous CLI claim; its direct library tests alone
did not cover startup behavior. SQLite may use normal read-side WAL shared-memory
coordination; the cooperative lease file may still be created.

Validation: formatting, all-target check, Clippy with warnings denied and all
376 Rust tests (353 library, 23 integration) passed with flatc 25.12.19.
Actual socket/HTTP integration ran outside network-restricted sandboxing.
Full test log: `/tmp/cogfabric-preview-readonly-tests.log`.

## Module inventory callback isolation (2026-10-13)

Status: **Implemented** for inference, arithmetic and SQL inventory observations.
`src/fabric.rs` and `src/fabric/tools.rs` now snapshot bounded retained handles
and activation flags before calling backend availability methods. Those callbacks
run after releasing the registry lock, preventing registry inspection inside an
inventory callback from deadlocking against lifecycle writes. Regression tests
in `src/fabric/tests.rs` and `src/fabric/tools.rs` assert write-lock availability
and inspect version metadata from actual observation callbacks. No model or tool
execution occurs. Snapshot references temporarily keep observed versions alive.

Membership and activation flags are sampled together; availability is observed
afterwards and can change independently. This is not an atomic remote-health
report, a provider probe or permission grant. Backend code remains trusted and
can block or panic. Other routing/activation callbacks have their existing
contracts; this change does not establish general callback reentrancy or
independent executable provisioning/isolation.

Validation: formatting, all-target check, Clippy with warnings denied and
378 Rust tests (355 library, 23 integration) passed with flatc 25.12.19.
Full socket/HTTP tests ran outside network restrictions; log:
`/tmp/cogfabric-inventory-observation-all-tests.log`. Inventory snapshots
clone at most 128 retained handles; no additional model or tool calls occur.

## Modular OpenAI-compatible agent serving (2026-10-13)

Status: **Implemented** for authenticated loopback Chat Completions, native
text/function turns and the tested OpenCode v2 workflow; broader API compatibility
and native conversation worker/registered-model support remain **Partial**.

The `cogfabric` library remains transport independent. `src/agent/` adds a typed,
replaceable native conversation backend and bounded FAST solver-turn lifecycle,
using the existing security kernel, global SQLite admission and execution leases.
Only `llm.generate` is granted. Tool calls are validated proposals returned to the
client, never executed or marked verified. Native audit records exclude raw
conversation, tool argument and answer bodies. Existing collective modes remain
available through separate aliases without accepting client tool definitions.

`crates/cogfabric-server/` adds HTTP/SSE mapping, dedicated bearer authentication,
loopback binding, browser-origin refusal, bounded connections/request bodies/turns,
timeouts and shutdown/disconnect cancellation. Its thin worker entry points reuse
core FlatBuffers workers, including actual math/SQL subprocess reaping tests.
No internal IPC schema or canonical French architecture was replaced.

Regression coverage: `src/agent/tests.rs`, server listener unit tests and
`crates/cogfabric-server/tests/http.rs` cover protocol correlation, unknown tools,
budget denial, durable cancellation/deadlines, authenticated two-turn tool/SSE
exchange, malformed/oversized requests, slow-body admission, connection capacity
and expiry, eight-turn saturation, streaming/non-streaming disconnects, collective
verification and server-image worker relaunch. CI and default workspace checks
now include the server crate; root `cargo run` retains the CLI default.

Formatting, workspace all-target check, Clippy with warnings denied, and all
391 Rust tests passed (360 library including the server listener; 31 integration).
Test log: `/tmp/cogfabric-agent-final-tests.log`. `docs/HTTP_API.md` documents
setup, limits, profile differences and unsupported capabilities.

Actual OpenCode v2.0.26 + LM Studio `qwen/qwen3-8b` passed read/edit/read on a
disposable fixture, with all other client actions denied. The first attempt
identified OpenCode's `store:false` field, now supported and regression-covered.
The final `scripts/opencode-smoke.sh` passed directly against the server: evidence
`/tmp/cogfabric-opencode-smoke.fuLeiy`, runner log
`/tmp/cogfabric-agent-opencode-smoke.log`. Five native requests (including client
metadata/title work) reported 4,893 input and 103 output tokens, three tool
proposals, 7,165.843 ms summed turn duration. All five persisted unverified;
server tool dispatch count was zero. OpenCode performed the three approved
client actions and the actual file contained `Status: after`. This is integration
evidence, not a general coding-quality or performance benchmark.

Debug server SHA-256:
`51425d19c325aec5602d5475f38d42b4d67a97cabb3c98ad48a2d9d828311fc0`.
OpenCode JSONL SHA-256:
`3bdc2b3136128e68d29748739774b0217f0145e2cd2ecf23b65bc7c6d61a8edc`.
SSE currently frames buffered upstream completions. Responses API, multimodal
input, remote/multi-tenant serving, native conversation worker RPC and full
collective reasoning over client tool loops remain unsupported. Existing
managed requirements are rejected rather than silently bypassed. Full v1.0
release gates and independent quality/security evaluations remain open.

Existing root CLI self-test and full installed-style CLI/socket smoke also
passed after the workspace split (evidence `/tmp/cogfabric-installed-smoke.IQKfcx`,
logs `/tmp/cogfabric-agent-existing-cli-smoke.log` and
`/tmp/cogfabric-agent-existing-socket-smoke.log`). Final protocol review additionally
rejects provider reuse of historical tool-call IDs; the regression and full
391-test workspace suite pass. Updated final live evidence follows below.

Final historical-call-ID fix also passed the complete scripted real-client run:
`/tmp/cogfabric-agent-opencode-final-smoke.log`, evidence
`/tmp/cogfabric-opencode-smoke.mCoC2U`. Five requests reported 4,891 input
and 98 output tokens, three client tool proposals and 7,213.491 ms summed turn
duration; all five remained unverified and no server tool dispatch occurred.
Final debug server SHA-256:
`a74c5a13e1954899944028769cc09cb4ee2ac2fd3d30eef2888422146900d890`.
Final OpenCode JSONL SHA-256:
`791c5b92a37b5258f554b073f2b8ab835cded56cbe54280c9640ac04106f285f`.
Earlier hashes/measurements in this entry refer to the preceding verified revision.
Client/model variations and startup work prevent treating these timings as a
controlled performance benchmark. Optimized/clean-OS installation of the new
server remains separate release validation work.

## Native conversation model-registration pinning (2026-10-13)

Status: **Implemented**. `AgentRuntime::local` now supports enabled immutable
model registrations through the existing selected model configuration. Startup
and each admission validate endpoint, model, role-profile absence and output
ceiling against the exact stored version. `run_until` snapshots registration
metadata before creating the task and persists the same `model_registration`
event contract as Engine, with weight identity explicitly unverified. Admitted
turns retain that snapshot if the operator later disables the model; subsequent
admissions reject before creating another task or calling inference.

The actual HTTP regression in `src/agent/tests.rs` confirms the registered model
and allocation in the provider request, pauses the response, disables the
registration, then verifies the original completed-turn metadata and rejection
of later turns. Mismatched configured model and excessive output ceilings are
also denied. This is metadata consistency and lifecycle evidence, not weight
attestation, automatic loading/unloading or exact context-token enforcement.
Native managed-worker RPC and role overrides remain unsupported. This
supersedes the earlier blanket native-registration limitation only.

Formatting, workspace all-target check, Clippy with warnings denied, and all
392 Rust tests (361 library, 31 integration) pass with flatc 25.12.19. Log:
`/tmp/cogfabric-native-registration-all-tests.log`. HTTP_API.md and THREAT_MODEL.md
now describe the supported selection boundary. No additional model calls or
permissions are added per turn; one bounded registration read/snapshot is added.

Hosted CI for the original modular agent API commit `f6ef304` completed
successfully: [38366331807](https://github.com/DrapNard/cognitive-fabric/actions/runs/38366331807).
This confirms that revision's workspace checks and existing CLI/socket workflow;
it does not establish new server optimized installation or general agent quality.

## Typed native conversation worker packets (2026-10-13)

Status: **Implemented** codec, **Partial** managed native execution.
`protocol/agent.fbs`, `src/agent/protocol.rs` and build integration add a dedicated
verified FlatBuffers native conversation format. This preserves history, tool
proposals and results without JSON envelopes or flattening into existing role
prompts. It is the prerequisite for the next managed-worker transport slice.
Existing schemas and native worker admission behavior remain unchanged.

The codec bounds physical/verifier/copy resources, validates fixed identity and
UUID fields, exclusive payloads, typed enums, allocations and history, and
preserves null text/unknown usage for tool-only completions. Four regression tests
in `src/agent/protocol/tests.rs` cover all choice modes and conversation roles,
malformed packets and identities, invalid enums/allocations, duplicate and
malformed output calls, and borrowed shared-string amplification. The API only
returns data; it grants no permissions, checks no live peer/deadline and launches
no worker. HTTP native managed mode remains explicitly disabled until those
transport/lifecycle checks exist. PROTOCOL.md records the integration contract.

Formatting, workspace all-target check, Clippy with warnings denied and all
396 Rust tests (365 library, 31 integration) passed using exact flatc 25.12.19.
Full log: `/tmp/cogfabric-native-protocol-all-tests.log`. No inference or new
permissions were added; the previously verified OpenCode workflow is unchanged.
Next milestone: authenticated one-task Unix exchange with trusted bootstrap
settings, bounded cancellation and audited owned-child reaping.

Hosted native registration CI [38367824575](https://github.com/DrapNard/cognitive-fabric/actions/runs/38367824575)
completed successfully for `4cf64ed`. Final codec formatting passes after
removing a test-file leading blank line; all workspace checks and 396 tests pass.

## Managed native conversation inference (2026-10-13)

Status: **Implemented** for same-image owner-local native workers, including
registered model configurations. `src/agent/unix.rs` adds UID/PID-authenticated
one-task/one-call RPC over the native FlatBuffers codec. Trusted non-deserializable
grants enforce task, input/output/proposal budgets and lifetime; replies match
original correlation/task/expiry and are validated against the original tools.
Connections are consumed on failure/cancellation rather than reused. A partial
frame has a ten-second timeout after its first byte; generation may use its full
granted deadline. Invalid peers/tasks, audit failures and pipelined data prevent
backend dispatch. Native backend construction is now lazy under cancellation,
so an already-cancelled turn invokes no backend or worker.

`src/agent/process.rs` owns the same-running-image child, private journaled socket,
execution lease and native process-capacity slot through reaping. Eight children
per host process may remain owned, including background cleanup after an entire
runtime future drops. Normal completion/cancellation waits for cleanup; failed
cleanup never produces a successful result or a false reap claim. Without a live
runtime/after host death, cleanup remains best effort and recovery preserves
unknown outcomes. These workers are coordination, not a hostile-code sandbox.

Trusted bootstrap appends `max_tool_calls` with legacy default four, tested absent
from default-valued frames and round-tripped at 0/1/4/64. `flatc --conform` against
the previous committed schema passes. Existing role-worker fields/versions and
permissions remain unchanged. Both supplied binaries delegate `agent-worker` to
the core implementation. Only explicitly scoped upstream inference credentials
are forwarded into otherwise cleared environments; server API keys are excluded.

Unit and actual-process regressions cover one-use proposals, forged identities
and outputs, unauthorized tasks/ceilings, audit-before-dispatch, partial frames,
provider-future cancellation, eight-worker capacity, live-lease recovery refusal,
registered native model selection, independent upstream/server credentials,
normal shutdown, streaming disconnect, whole-future non-stream disconnect and
ownership-audit failure with confirmed child reaping. No client tool executes
inside the worker or server. Files: `src/agent/`, `protocol/bootstrap.fbs`,
`src/fabric/unix/bootstrap.rs`, shared socket helper, binary entry points and
`crates/cogfabric-server/tests/native_worker.rs`.

Mandatory formatting, workspace all-target check, Clippy with warnings denied and
all **414 Rust tests** (376 library, 38 integration) passed with flatc 25.12.19.
Full log: `/tmp/cogfabric-native-managed-all-tests.log`. Existing CLI self-test
and complete installed-style typed socket smoke pass; socket evidence
`/tmp/cogfabric-installed-smoke.wLpdmV`, logs
`/tmp/cogfabric-native-managed-cli-smoke.log` and
`/tmp/cogfabric-native-managed-socket-smoke.log`.

Actual OpenCode v2.0.26 + LM Studio `qwen/qwen3-8b` passed the final managed
read/edit/read smoke with only that disposable file allowed. Evidence:
`/tmp/cogfabric-opencode-smoke.TnXEVJ`; runner log
`/tmp/cogfabric-native-managed-final-opencode.log`. The script now asserts each
admitted turn launched and reaped a worker. Five requests (including client
metadata work) reported 4,901 input/103 output tokens, three tool proposals and
7,549.771 ms summed turn duration. All five tasks remained unverified; all five
children were reaped with no forced kills and exit code zero; server tool dispatch
count was zero. Actual fixture contents matched `Status: after`. These are
integration observations, not controlled quality or latency benchmark claims.

Final debug server SHA-256:
`a6371603c478bd6b23f6dc50e3bba129fe4ad6e52cda6d7a1b09f4e803181fde`.
Final OpenCode JSONL SHA-256:
`3eaad52ab1a5f54ad37067abb29ef887575c1074a77d596f1563f632e188306f`.
Updated smoke script SHA-256:
`bc4fe9a14427c9f10138ec82b5a33fa837213d0a75b291da9e6a4654743f1485`.

This supersedes earlier native-worker-disabled statements. HTTP_API.md,
PROTOCOL.md, WORKERS.md and THREAT_MODEL.md describe the supported boundary.
Native role overrides, arbitrary executable provisioning/attestation, stronger
isolation, independent agentic quality/security evaluation and optimized/clean-OS
installation of the expanded server remain incomplete. External GPU completion
is not established by local cancellation or reaping. Full v1 readiness is unproven.

Shutdown additionally drains native ownership for up to ten seconds, including
cleanup scheduled after a request future already disappeared. A cleanup/audit
error disables new native worker admissions for that host lifetime and makes
shutdown fail visibly. Actual-process regressions cover immediate shutdown after
non-streaming disconnect and injected stop-audit failure with no further child
launch; provider disconnect is observed asynchronously rather than inferred
from the reaping timestamp alone.

Managed frame encoding is also validated before task/child admission, so a
request within a raised HTTP/input ceiling but beyond the 256 KiB native frame
limit is rejected without launching a worker. The regression verifies no task
row or child intent is created. Final mandatory workspace checks and 414 tests
(376 library, 38 integration) pass.

Hosted codec CI [38369517206](https://github.com/DrapNard/cognitive-fabric/actions/runs/38369517206)
completed successfully for `e04fcd6`. The final managed-worker revision has local
414-test and live-client verification above; its hosted run follows private sync.

Update 2026-10-13: managed-worker source `d807e9e` also passed hosted CI
[38377800359](https://github.com/DrapNard/cognitive-fabric/actions/runs/38377800359),
locked offline optimized workspace binary builds, the core CLI/socket smoke and
the managed OpenCode/Qwen read/edit/read smoke using the optimized server.
RELEASE_AUDIT.md records binary hashes and fresh evidence directories. Optimized
workflow verification is implemented and tested; new-server clean-OS/package
installation and independent agentic-quality evaluation remain incomplete.

### Collective worker ownership on dropped futures

**Implemented:** `src/fabric/worker_cleanup.rs`, collective inference/tool process
Drop implementations and HTTP shutdown integration retain sockets and execution
leases until child reaping. Shared ownership also includes expert inference
processes. Failed cleanup/audit blocks subsequent collective process owners and
fails drain; startup ownership remains retained after explicit cleanup errors.
**Tests:** dropped startup subprocess owners for inference/tools;
`crates/cogfabric-server/tests/collective_cleanup.rs` exercises real managed
inference on HTTP disconnect, immediate shutdown and stop-audit failure with
blocked readmission. No new external tools or untrusted plugin sandbox. General
provisioning, residency scheduling and independent reasoning evaluation remain
incomplete. WORKERS.md records bounded waits and forced-host-termination limits.

Final verification: formatting, offline workspace all-target compilation and
Clippy with warnings denied pass. All 417 Rust tests pass (378 library, 39
integration). Full log: `/tmp/cogfabric-collective-cleanup-tests.log`. The first
sandboxed socket test attempt was denied by local sandbox policy; the complete
socket/subprocess suite passed with authorized local execution. No new live
model study or optimized installation was run for this cleanup revision.

### Installed HTTP interoperability verification

**Implemented and verified:** separate optimized core/server package installation
from runtime source `38948b1`, installed CLI/socket smoke, new offline installed
HTTP smoke and installed managed OpenCode/Qwen read/edit/read. HTTP_API.md records
installation commands; INSTALLATION_VALIDATION.md extends the reproducible clean
Arch workflow to both packages (that updated clean-OS workflow is not yet run).
The offline HTTP smoke now runs in CI; this revision's hosted result remains
pending. RELEASE_AUDIT.md records exact runtime source, binary/script hashes and
evidence. Rust runtime/tests are unchanged: the previous 417-test quality gate
and hosted runtime CI pass. Independent expert quality and general provisioning
remain partial.

### Clean-OS installation refresh

**Implemented and verified:** source `a9933b3` passes fresh Arch prerequisite
setup/dependency download, pinned FlatBuffers, all mandatory workspace checks and
417 tests, separate optimized core/server installs, and both installed offline
workflow smokes. Hosted CI `38381768218` also passed the new HTTP smoke step.
INSTALLATION_VALIDATION.md gives reproduction; RELEASE_AUDIT.md records immutable
image/source references, binary/log hashes and limits. This supersedes the prior
new-server clean-OS and pending HTTP CI gaps. Native GPU/model setup in Arch,
CachyOS-specific desktop setup, independent expert quality and general module
provisioning remain unverified or partial. No runtime changes or new test count.

### Read-only residency operation inspection

**Implemented:** `residency::inspect_operation` and CLI `residency-inspect` compare
one bounded recorded operator intent against a fresh inventory GET. Early
read-only startup avoids migrations/bootstrap. Exact endpoint matching, strict
intent/action parsing, original bounds and duplicate detection precede HTTP.
**Tests:** `tests/residency.rs` covers load/unload present/absent observations,
one GET without retry/POST, unchanged tasks/config/skill registry, and malformed,
oversized, duplicate, missing or wrong-endpoint intent before network.
**Partial:** observing desired state proves neither operation causality nor
instance ownership. Tasks are never finalized or granted cleanup authority;
automatic scheduling and live authenticated mutation validation remain open.

Final inspection checks: formatting, workspace all-target check, Clippy with
warnings denied and 419 Rust tests pass (378 library, 41 integration). Test log:
`/tmp/cogfabric-residency-inspection-tests.log`. No live residency mutation or
source-install refresh is claimed for the new command.

Inspection credential regression: **Implemented and tested** in `tests/residency.rs`.
Separate fixture residency/inference tokens verify scoped forwarding and encoded
provider-echo rejection without task changes or response metadata disclosure.
Current workspace gates pass with 420 tests (378 library, 42 integration).
This proves the local CLI credential path, not real server ownership/authentication.
MODELS.md records why documented native REST instance IDs do not supply the
coordination contract needed for safe automatic unloading.

Hosted runtime CI [38384114873](https://github.com/DrapNard/cognitive-fabric/actions/runs/38384114873)
passed for `d4b9cf5` (419 tests). The added credential regression is verified
locally with the 420-test gate above; its own hosted run follows private sync.

### External compiler oracle for public Rust diagnostics

**Implemented:** 20 balanced ownership/borrowing cases and a versioned hash/label
manifest under `evaluation/rust_ownership_*`; fixture regression compiles fixed
trusted snippets as Rust 2021 metadata and checks specific negative diagnostics.
Compiler subprocess waits and cleanup are bounded; snippets/model output never
execute. This adds an oracle outside the model and built-in framework checkers,
without adding runtime permissions. **Partial:** public authoring is not unseen
or independently reviewed data, and this test establishes no live-model result.

Compiler corpus gate: all 421 workspace tests, formatting, all-target check and
Clippy pass. Runtime library count stays 378; integration count is now 43.

### Solver output-format instruction consistency

**Implemented:** `src/backend.rs` conditions explanatory fact/guess separation on
the original user's output contract. Untrusted-reference boundaries, scorer and
resource/security limits are unchanged. The compiled role-prompt hash changes;
existing continuation identity checks remain authoritative. All 421 workspace
tests and mandatory checks pass (`/tmp/cogfabric-solver-format-tests.log`).
The public compiler-backed Qwen diagnostic before this change matched 6/20
strict labels; raw reports include format failures and incorrect answers.
This does not establish causal gains, held-out robustness or expert quality.

Frozen public Rust rerun: strict matches changed from 6/20 to 8/20, with three
improvements and one regression, unchanged labels/scorer/configuration and zero
tools. Both raw reports and metadata are committed. Extra prose and incorrect
judgments remain; this does not establish reliable expert behavior or causal
reasoning gains. BENCHMARKING.md records actual latency and reported usage.

### Complete basic-benchmark preflight and execution disclosure

**Implemented:** `evaluation::benchmark` strictly parses and validates the whole
bounded dataset before creating tasks; empty/oversized/unknown-field late cases
and excessive count fail before inference or tool dispatch. Reports add dataset
checksum, mode, charged model/tool task counts and runtime status counts without
changing scoring or missing-usage semantics. Five regressions in
`src/evaluation/benchmark_tests.rs` cover no-side-effect rejection, label isolation,
exhausted zero-call runs and legitimate deterministic-tool success. No new model
or tool capability. General independent benchmark review remains incomplete.

Final preflight gate: 426 tests, formatting, workspace all-target check and
Clippy pass. CLI arithmetic report smoke verifies ten tool successes, no model
calls and status/hash disclosure. QUALITY_REVIEW.md records logs and scope.

### Bounded basic-benchmark cancellation

**Implemented:** `evaluation::benchmark_until` and CLI `bench --max-run-seconds`
apply a 1–86400-second run ceiling (default 1800) alongside task limits. Ctrl-C
and deadline expiry retain partial results and durable cancelled active tasks,
preserve charged calls/unknown usage and prevent later admission. Ready
cancellation creates no task; partial accuracy excludes unexecuted cases.
Four new unit regressions cover invalid bounds/mode, pre-admission cancellation,
active cancellation and virtual-time expiry with retained completed prefixes.
Two real CLI/stalled-provider tests verify partial JSON, nonzero exits, SIGINT
and durable task status. Cleanup grace and external GPU-stop limits remain
explicit in BENCHMARKING.md; no permissions or model-quality claims expand.

Final benchmark cancellation gate: 432 tests, formatting, workspace all-target
check and Clippy pass. Actual successful CLI arithmetic also passes under an
explicit five-second run ceiling. QUALITY_REVIEW.md records logs and scope.
Hosted preflight CI `38398499992` passed for `a4ca9b8`; the new cancellation
revision's hosted result follows private sync.

### Bounded benchmark model/configuration disclosure

**Implemented:** `bench` records declared configuration/checksum, package version
and compiled local-role prompt hash with explicit non-attestation flags. Basic
and comparison cases share `evaluation::model_metadata` for content-minimized
backend labels, expert bindings and primary registration snapshots. SQL avoids
materializing reasoning bodies; label/event/snapshot/aggregate bounds fail closed.
Tests cover large excluded bodies, malformed/oversized/duplicate metadata, count
and aggregate limits, configuration hashing/home omission, distinct requested vs
reported labels, tool-only empty labels and invalid configuration before even
pre-cancelled reporting. No permissions or scoring changes; weight/backend
attestation and independent quality remain unproven.

Final identity-disclosure gate: 434 tests, format, workspace check and Clippy pass.
Actual CLI arithmetic confirms configuration/prompt hashes, omitted home paths
and no inferred model labels. QUALITY_REVIEW.md records evidence and scope.

### Replaceable executive expert router

**Implemented:** `src/routing.rs`, `Engine::with_expert_router` and audited cached
logical policy identity. The default tag selector preserves behavior; accepted
results are bounded to two known enabled snapshot IDs. Callbacks run outside
SQLite locks, audit failure prevents invocation, and ablation exclusions persist.
Tests cover decision/identity bounds, re-entry, in-flight snapshot disable, old
Engine clone preservation, policy replacement and audit denial before callbacks.
Bench/compare disclose configured router metadata without attesting implementation.
**Partial:** this is a trusted Rust metadata callback, not untrusted executable
provisioning, learned utility, persisted policy rollback or general scheduler
replacement. ROUTING.md records contracts and resource/isolation limits.

Final router gate: fresh-target verification passes 438 tests, formatting,
workspace check and Clippy. Default CLI policy metadata/audit and zero extra
model calls also pass. QUALITY_REVIEW.md records new evidence paths after the
previous temporary artifacts disappeared. Hosted identity CI `38402184154` passed
for `929c6af`; the router revision's hosted result remains pending private sync.

### Bounded skill inventory storage boundary

**Implemented:** `Store::skills_from` uses row/SQL byte bounds before owned copies,
strict tag parsing and field/version/enable validation. Legacy empty tag arrays
remain readable. `ensure_skill` validates new writes and shares immediate-
transaction ID admission with `registry::register`; retained legacy plus manifest
IDs cannot bypass the common 4,096 ceiling. Router input uses that same constant.
Four regressions cover corrupt/oversized metadata without data mutation, boundary
read/write behavior and existing-ID versions, competing connection admission,
and no routing/inference dispatch from invalid snapshots. No schema migration,
data deletion, model load or new permissions. Existing invalid data fails closed
and needs operator-reviewed repair; signatures and physical quotas remain separate.

Health uses the same bounded inventory reader and one consistent read transaction
for retained metadata, active graph, model references and document counts. Invalid
legacy metadata fails health rather than being hidden behind an empty active graph.

Final skill-inventory gate: 442 tests, formatting, workspace check and Clippy pass.
CLI valid registration, duplicate-tag denial and consistent health count pass.
QUALITY_REVIEW.md records evidence and the initial corrected CLI syntax attempt.
Hosted router CI `38407375428` passed for `b3e85c3`; storage-hardening CI follows
private sync. Independent evaluation and general lifecycle requirements stay open.

### Bounded manifest reads and unambiguous active identities

**Implemented:** `registry::active_graph`, activation and existing-version
registration use SQL byte guards before owned manifest/hash/identity copies.
Rows are verified while streaming, with active count and duplicate-ID rejection.
Idempotent registration verifies the stored body, preventing a matching old
checksum from hiding body corruption. Activation input identity/version is
validated before database lookup. Three regressions cover body corruption, field
limits across read paths and ambiguous/excessive active sets without implicit
repair or promotion. No data migration, deletion, model load or new permissions.

Final manifest-bound gate: 445 tests, formatting, workspace check and Clippy pass.
Valid CLI registration/idempotency/activation/health also pass. QUALITY_REVIEW.md
records actual evidence and unchanged corruption/audit state on denial. Broader
independent evaluation, module lifecycle and code authenticity remain incomplete.

Active routing description/tags are now checked against the verified manifest
body. Valid-sized projection drift fails snapshots/health without automatic
repair. Reviewed version activation updates the projection and preserves prior
task snapshots. Two regressions plus the 447-test workspace gate pass;
QUALITY_REVIEW.md records scope and actual CLI evidence. Owner-resistant
authenticity and broader independent quality evaluation remain unproven.

Explicit `skill-disable` can now exclude a damaged target while validating every
remaining active expert and reverse dependency. Only its enable flag and audit
change; corrupted body/hash data is retained. Other active corruption or required
dependents still deny the operation. A verified older version may be activated
after successful containment. This is operator-directed, not automatic repair
or a bypass for activation/promotion. Three regressions and all 450 workspace
tests pass; QUALITY_REVIEW.md records CLI evidence and remaining data-repair limits.


### Explicit atomic expert disable sets

`skill-disable-set ID...` / `registry::disable_set` accepts 1–256 distinct
canonical IDs under the existing human-only `skill.disable` permission. The
operator explicitly selects the damaged expert and any dependents to contain.
Remaining enabled manifests, projections and dependencies must validate. No
cascade is inferred; unknown/already-disabled targets and late audit failures
roll back every enable change and audit in the immediate transaction. Bodies,
hashes and retained inventory capacity are preserved. Normal activation stays
strict; this is not executable isolation or automatic recovery. Invalid retained
inventory metadata can still require reviewed repair/restoration.

Validation: all 453 workspace tests and mandatory checks pass; actual CLI
containment passes. See QUALITY_REVIEW.md for evidence and unchanged release gaps.


### Bounded shortest-path evidence

**Implemented:** `src/verification/path.rs` independently checks shortest directed
unweighted paths from original `path:` objectives. `src/reasoning.rs` uses existing
budgeted verification/revision and preserved checked results; checker registry
bindings, recovery reservation validation and comparison scoring support the new
version. Exhaustive three-node graph tests, malformed/bounds cases, actor/task
denials and failed-then-correct reasoning cover the slice. This expands executable
criteria without claiming general semantic verification or measured model gains.

Shortest-path gate: all 459 workspace tests and mandatory checks pass; actual
CLI/HTTP fixture passes. See QUALITY_REVIEW.md for evidence scope and limitations.


### Public shortest-path diagnostic corpus

`evaluation/shortest_path_cases.jsonl` contains 16 original CC0-1.0 directed,
unweighted graph objectives: direct/chain paths, alternative optima, directed
reverse traps, disconnected graphs, cycles, unreachable goals, identical endpoints,
dead ends, nonlexical labels and branch merging. Inputs contain only criteria and
`verification: shortest_path_v1`, with no expected path fed to inference. The
separate `evaluation/shortest_path_manifest.json` pins dataset/objective hashes,
minimum distances and one accepted optimum per case. Dataset SHA-256:
`0fedaaf6bef27f45ae0a13debcd5c6e391a32637c84891b0f38a19d9cba93edc`.

`tests/shortest_path_corpus.rs` independently enumerates all simple paths in these
small fixed graphs to check labels, alternative optima and provenance metadata.
The runtime checker uses breadth-first distance computation instead. Comparison
regressions prepare every frozen row and recheck reference outputs while rejecting
forged verification objects and rejected runtime states. This is public diagnostic
data, not a held-out or independently human-reviewed study. Contamination has not
been assessed; no live model result is recorded. On 2026-10-13 the configured
LM Studio endpoint `127.0.0.1:1234` refused the read-only model-list connection
(curl exit 7). No provider was started, no weights loaded/unloaded and no model
quality score is inferred from that failure.

When the authorized Qwen endpoint is available, use a fresh workspace configured
for `qwen/qwen3-8b` and run a bounded `compare` with this dataset, its pinned hash,
`--source "Public synthetic shortest-path diagnostics" --license CC0-1.0`,
`--code-revision` set to the tested revision, and `--workflow-check`. Use the
same configuration and budgets across the six profiles, retain raw reports and
usage, and report incomplete/failed runs. Do not label this public corpus held out.
Independent unseen tasks and equal-budget model studies remain release work.


### Revision-chain resource admission

Before unverified deep/research answers enter critique, Jev+ now checks that an
additional cycle, three calls and 640 output-token reservations remain for the
critic (128), reflector (128) and revised solver (384). It also checks cumulative
prompt-byte upper bounds and each backend prompt's 32,000-byte ceiling. Bounds
use the exact candidate truncations, the validated 1,000-byte critique reason,
and a 4,000-character strategy bound of 16,000 UTF-8 bytes. No new allowance is
granted or charged by this advisory admission; each subsequent call still uses
normal durable reservations, authorization, deadline and cancellation checks.
Elapsed provider time can still prevent later stages after initial admission.

The durable `schedule` event precedes critique and records `admit_revision` or
`skip_revision`, required costs, uncharged prompt bounds and
`expected_utility_measured: false`. A denied chain preserves the current
unverified candidate with `insufficient_revision_budget`, avoiding critique or
reflection that cannot support another answer. Independent executable-criterion
revision retains its direct checker-to-solver path. This is feasibility-based
resource control, not calibrated expected utility, general scheduling, semantic
verification or evidence of improved model accuracy.

Revision-chain gate: 462 workspace tests and mandatory checks pass. Actual CLI
fixture confirms avoided critique dispatch; see QUALITY_REVIEW.md for scope.


Current host optimized packaging: exact source `840e63d` installed both CLI and
server offline with pinned FlatBuffers; complete installed CLI/socket and HTTP
workflow scripts pass. Hosted CI `38420337608` succeeded. See
INSTALLATION_VALIDATION.md for hashes, evidence and scope; current clean-OS, live
model-quality and broader lifecycle gates remain unproven.


### Replaceable, recovery-pinned dependency scheduling

**Implemented (trusted Rust host scope):** `src/scheduling.rs` provides a versioned
`SubtaskScheduler` and default stable topological order. Engine installation
captures policy metadata; sessions validate full dependency-respecting selections,
audit intent before callback and persist order/hash before any subtask events.
`src/recovery/graph.rs` validates the committed schedule and sequential execution;
continuation prefix checks and actual graph continuation use that order without
reinvoking a policy. Legacy unmarked graphs retain default behavior; marked graphs
cannot silently fall back. Benchmark metadata discloses policy and no attestation.
See SCHEDULING.md. General parallel/module/GPU scheduling, untrusted policy
provisioning and measured expected utility remain incomplete. No grants, budgets,
protocol schemas or model-loading authority expand.

Scheduler gate: all 466 tests and mandatory checks pass. Default CLI/HTTP graph
audit passes; custom-order execution/reconstruction and tamper denials are covered.
See QUALITY_REVIEW.md for recovery proof scope and unchanged release blockers.


Custom-order HTTP continuation: **Verified (supported pre-solver graph scope)**
in `src/recovery/continuation.rs` regressions. A recorded `[1,0]` order resumes
only its pending node and final solver through real local HTTP, retaining original
charges and settings. Changed prefix/descriptor or missing decision denies preview
and approved resume before network/state change. SCHEDULING.md records exact
assertions and limits; arbitrary within-call recovery and independent model quality
remain incomplete.


Real Qwen shortest-path deep diagnostics: **Partial / Measured public workflow**.
Source `22dbc5a` produced 56/96 outcomes before its unchanged 1,200-second cap.
Nine common cases scored alone 6, tools 6, recursive 7, collective 6, memory 6,
experts 2. Six expert timeouts/one transport failure remain counted. Raw reports,
load context, advisory manifest and hashes are committed; a separate exhaustive
oracle validates every pass bit and totals. No expert/collective advantage or
held-out independence is established. BENCHMARKING.md records conditions and
limits; broad release blockers remain open.


### Optional advice wall-time containment and shared timeout fallback

Optional legacy/structured workflow advice now has one cooperative phase deadline:
one third of the task's remaining original wall time, capped by the configured
inference timeout. The `expert_phase_budget` event records that ceiling before
advice execution. This is a deterministic feasibility heuristic, not measured
expected utility or a guarantee that later inference fits. Structured steps share
one phase allowance rather than resetting it per step. Original task deadline,
call/output/prompt/tool ceilings and individual authorizations remain in force.

A typed runtime inference timeout permits shared-backend advice to return control
to whole-goal reasoning, matching the existing bound-model fallback intent.
The spent call/output reservation remains charged; partial advice is discarded,
reported usage/queue totals become unavailable, and `expert_failure` must persist
before any main-solver work. Failure-audit errors stop execution. Other shared
backend errors, malformed results and kernel/audit denials remain fatal. Existing
bound-model failure fallback semantics are unchanged. Provider computation may
continue after client cancellation; `provider_stop_proven` remains false. This
adds no retries, new model loads, tool grants or fresh task allowances.

The real partial Qwen study at `22dbc5a` remains frozen and is not rescored or
rewritten. Its six expert timeouts motivated containment; this change has not
established improved Qwen accuracy or resolved its transport failure. Shared
backend recovery after a dropped managed exchange still depends on that adapter's
fail-closed session lifecycle; no automatic worker restart is introduced.


### Structured partial-advice containment proof

`structured_advice_uses_one_deadline_and_discards_partial_output` completes one
advisory step, stalls its dependent step and verifies that both share the original
phase deadline. The dependent receives its predecessor as unverified advice, but
after timeout neither main planner nor solver receives that partial output.
The final independently checked answer completes with four charged calls and
1,024 reserved output tokens; usage remains unavailable. Audit history retains
the completed advisory step, no fake completion for the stalled step, one phase
budget and `partial_advice_discarded: true`. Upstream stopping is not inferred.

The same behavior passed through actual CLI/HTTP in
`/tmp/cogfabric-partial-advice-smoke._4rwx3oj`: the first step returned after a
controlled delay, the second stalled, and main reasoning completed within original
limits without the partial sentinel. This adds regression/integration evidence,
not a new live Qwen accuracy result or an arbitrary managed-provider recovery claim.


### Bound-advice failures remain fail-closed

Fallback now accepts only the private typed runtime timeout for shared and bound
advice. Previously the bound-model path caught every error, allowing policy/audit
or result-integrity failures to be treated as optional provider unavailability.
Unknown bound-provider errors now visibly fail the task; no primary inference
follows. Error text is never used as a classifier. Missing/disabled bindings still
skip before dispatch, and typed timeout fallback retains original spent charges,
unknown usage, mandatory failure auditing and remaining task limits.

This intentionally narrows availability fallback until a trustworthy typed error
contract can distinguish provider faults from kernel/transport integrity failures.
There is no retry, permission expansion, new process or model-loading authority.
Operator-trusted Rust remains outside an untrusted-code sandbox guarantee.


### Durable inference reservation accounting

`Session::call_with_backend` now records `model_dispatch` before mutating call,
output-reservation or prompt-byte counters and before taking prior usage metrics.
A rejected database insert therefore leaves existing session charges and reported
usage intact and cannot invoke the backend. A committed reservation still counts
if progress publication fails: `Store::event` marks that post-insert failure with
a private `PersistedEventError`, allowing the session to retain the durable charge
while returning the error before inference. Error text is not the classifier.

No committed reservation is refunded or replayed. Terminal task failure and
recovery audits retain their conservative unknown-usage semantics; preserving
prior in-session metrics does not fabricate a complete failed-task usage report.
The event schema, ceilings, authorization and backend execution order are otherwise
unchanged. A test-only observer poison hook exercises the post-commit boundary;
it is absent from production builds. Observer failure is not provider availability
and cannot enter the typed-timeout advice fallback.


### Unambiguous JSON at agent boundaries

Agent HTTP request bodies, native provider response objects, decoded native tool
schemas and tool-argument objects now reject duplicate decoded JSON keys at every
nesting level. This includes escaped spellings of the same name and duplicate
objects inside arrays. Byte caps remain 512 KiB for HTTP request bodies, 256 KiB
for provider responses and 32,000 bytes for schemas/arguments; ordinary JSON
recursion limits and trailing-data rejection remain enforced. The reusable Rust
`strict_json::value` helper preserves accepted JSON values without echoing the
rejected key. Raw valid tool argument strings are not rewritten.

Duplicate HTTP bodies fail before native inference. Ambiguous provider objects
cannot supply a completion; ambiguous tool proposals and correlated history are
also rejected. This eliminates a class of parser disagreement, not all prompt
injection or schema violations. Tool JSON Schema semantics and action permissions
remain the client's responsibility; Fabric proposes tools and executes none through
this API. No remote reference resolution, host capability, new tool or model
permission is introduced. Internal FlatBuffers framing/schema is unchanged.


### Strict provider and advisory JSON

Ordinary OpenAI-compatible reasoning responses now use the bounded duplicate-key
parser before selecting answer/model/usage fields. Structured advisory parsing
applies the same rule after validating its optional JSON fence, including nested
objects inside arrays. Native residency responses and inventory decoding also
reject duplicate decoded names before their values can be normalized or observed.
Existing response/role byte caps, JSON recursion limits, trailing-data checks,
credential echo checks, loopback-only HTTP and redirect/proxy denials remain.
Unique unknown native metadata stays forward-compatible; duplicate metadata fails.

Ambiguous initial inventory cannot authorize a residency POST. If an approved
POST has already occurred and its reply is ambiguous, the task remains interrupted
with `outcome_unknown`; no confirmation request, retry, inferred ownership or
cleanup follows. Model/provider content still confers no permissions or factual
attestation. Internal FlatBuffers and stored benchmark results are unchanged.


### Versioned declared capabilities and discovery

Expert manifests may now declare up to 16 distinct capability identities with
independent canonical semantic versions:

```json
"capabilities": [{"id":"domain.sort","version":"2.1.0"}]
```

This field is optional and omitted from canonical serialization when empty,
preserving legacy manifest hashes. Each expert's own ID implicitly provides its
manifest version; redeclaring that same ID is rejected. Capability identities use
the existing bounded project identifier syntax. Duplicate IDs, invalid versions,
unknown capability fields and excessive entries fail manifest validation. Aliases
are immutable with their manifest version and copied into task snapshots. Expert
dependencies still refer to concrete expert IDs/versions; aliases do not rewrite
or relax dependency constraints.

`skill-lookup domain.sort --requirement '^2'` lists active manifest providers whose
declared capability version matches the requested semantic range. The command
uses human-only `skill.inspect`; it grants no runtime authority. Results explicitly
say `declaration_only`, `permissions_granted: false` and `provider_health_proven:
false`. Unversioned legacy categories remain listed by existing skill inventory,
not fabricated as versioned lookup matches. Multiple compatible providers remain
visible rather than being silently activated or loaded.

Advisory language interpretation can name a declared capability separately from
an expert ID. The engine resolves it against the pinned enabled snapshot and
adds its provider ID within the existing four-refined-route ceiling. Resolution
prefers an exact active manifest, then the lexically first reviewed alias provider,
then legacy category metadata. This prevents a legacy placeholder from hiding a
reviewed provider. Current gap preparation reads enabled declarations and manifests
in one transaction, so aliases count as available metadata without creating a
candidate or grant. Capability versions are exposed by lookup; language intent
still names capabilities without version requirements.

Large interface prompts exceeding 32,000 bytes explicitly audit
`skip_interpretation` and preserve whole-goal reasoning rather than dispatching an
invalid optional prompt. The catalog is not silently truncated. These changes do
not prove operational expertise, document retrieval quality, provider availability,
weights, executable isolation or calibrated expected utility. Declaring a sensitive
name such as `shell.execute` cannot grant that permission. No model/tool loader,
FlatBuffers schema change, database migration or neural promotion is introduced.
Public Rust struct-literal callers must provide the new `capabilities` vector;
legacy serialized manifests remain compatible.


### Pinned capability route bindings

`route_refined` now records one bounded binding for each requested advisory
capability: requested ID, resolved provider, provider manifest version, declared
capability version, route-selection state, declaration-only status and no granted
permissions. Values come exclusively from the task's pinned snapshot. Replacing
an active declaration during interpretation cannot change the recorded old
binding. Legacy unversioned declarations retain null versions; missing capabilities
have null providers/versions and remain explicitly unselected with a gap record.

A compatible provider can be available but unselected when the existing four-route
ceiling is reached. `selected` describes route membership, not executed advice,
provider health, factual correctness or granted authority. Existing dispatch,
budget and verification records remain authoritative for those separate claims.
No route ceiling, permission, model call or protocol schema changes.


Pinned compiler-free source builds: **Implemented** in `build.rs`,
`build_support/protocol.rs`, `protocol/generated/` and
`scripts/update-protocol-bindings.sh`. Exact runtime pin and schema/generated hashes
are required in default mode; explicit 25.12.19 regeneration must be byte-identical.
CI covers both modes. Bundle tampering/staleness and normalized package pins have
negative regressions; a locally archived/extracted core crate builds without FLATC.
No IPC schema or security-grant change; broader release blockers remain open.


Both optimized packages at `84e097c` install with FLATC unset and pass complete
installed CLI/socket and HTTP workflows. Hosted CI `38454460668` succeeded with
explicit regeneration and bundled checks. INSTALLATION_VALIDATION.md records
hashes, logs and host-cache scope; 490 source tests remain passing. This updates
installation evidence without claiming current clean-OS or live quality proof.


### Exact native tool-choice and text-part controls

Named `tool_choice` objects accept exactly
`{"type":"function","function":{"name":"declared_tool"}}`. Unknown outer or
nested fields, non-object function controls and undeclared names are rejected.
String choices `auto`, `none` and `required` retain existing semantics. This avoids
forwarding extra control fields in direct mode that native managed transport would
otherwise drop. Strict-schema declaration belongs to the tool definition, not the
named choice; the client still validates schemas and approves actions.

HTTP text parts accept exactly `type: text` and a string `text`. Extra image or
other fields are explicitly rejected rather than silently removed while flattening
text. These unsupported shapes return 400 before inference. Valid named choices
and ordinary text arrays remain supported. No tool execution, permission, budget,
conversation role or FlatBuffers schema is added.

### Optional expert cancellation regression

**Implemented and tested:** `src/reasoning.rs` verifies explicit task cancellation
while legacy advice is pending and while a dependent structured advice step is
pending after its predecessor completed. Both return cancelled, preserve every
dispatched call/output reservation, leave reported usage unknown, emit no timeout
fallback event and start no primary planner/solver work. The broker observes the
closed reply channel and drops the local pending backend future within a bounded
test wait. Partial advice never becomes verified evidence. This adds regression
coverage without changing production policy; dropping a local future does not
prove an external provider stopped GPU inference. Broader expert effectiveness,
independent verification and executable isolation remain Partial.

### Expert advice diagnostics in benchmark reports

Each benchmark/comparison detail now includes `expert_advice` audit counts:
`phases_started`, `fallback_events`, `inference_timeout_fallbacks`, and
`unclassified_fallbacks`. Current optional-advice timeouts persist the typed
`failure_kind: inference_timeout` classification. Historical events without that
classification and future unknown kinds remain unclassified; error text is never
used to infer a timeout. Counts are task-scoped and read through bounded SQLite
projections (128 events, 16 KiB per diagnostic event), without copying advice,
prompts or error messages into the report. Oversized audits fail reporting.

These are attempted-phase/fallback counts, not expert success rates. A fatal
provider/audit failure or cancellation may leave a started phase without a fallback
event; absence of fallback proves neither completion nor usefulness. Existing
reports remain frozen. Independent quality evaluation, broader expert lifecycle
and physical provider termination remain incomplete.

### Live four-case expert diagnostic follow-up

Source `b6b704323cac67b54935fd06397eeb66113be673`, Qwen3 8B through LM Studio,
completed 24/24 tasks in 282.33 seconds. The first four public graph cases were
run across all six profiles in fresh state with the committed advisory workflow,
empty document memory, 30-second task ceilings and 20-second inference ceilings.
Other declared settings and per-task resources are retained in
`evaluation/results/qwen3-8b-advice-diagnostics-four-cases.json`; hashed context and
summary are in its `-summary.json` companion. The CLI loaded the existing Qwen
weights with 8192 context, 0.35 requested GPU offload, one parallel prediction and
600-second idle TTL. No other model was unloaded; framework residency ownership
and immutable weights remain unproven.

Scores: alone 3/4, tools 3/4, recursive 3/4, collective 3/4, memory 2/4,
experts 2/4. All four expert advice phases emitted typed timeout fallback events;
main reasoning then completed two answers and exhausted two original task budgets.
The six arms charged respectively 4, 5, 5, 17, 16 and 13 model calls. Independent
simple-path enumeration agrees with every recorded pass bit; a frozen integration
regression checks scores, hashes, charged calls and diagnostic totals.

This demonstrates live bounded advice failure containment and report diagnostics,
not expert usefulness. There is no expert gain in this run. The corpus is public,
small and authored in this project; no held-out or independent human review,
training-contamination control, larger baseline or causal quality study is claimed.
Task ceilings differ from the older 90-second partial run, so these results must
not be treated as a controlled before/after comparison. Historical reports remain
unchanged. General expert lifecycle/isolation and independent quality remain open.

### Concise optional advice output contract

Legacy and structured advice prompts now request one sentence of at most 24
words naming the most important domain constraint/pitfall, without restating the
problem, enumerating steps or producing the final answer. This is a model output
request, not enforced linguistic correctness. Runtime token reservations, phase
ceilings, permissions, audit failure handling and independent verification remain
unchanged. Advice remains unverified; no new trusted evidence is created.

A direct Qwen probe on the first public graph used the same system/model settings:
192 output tokens exceeded a 30-second client deadline; 64 tokens returned in
15.78 seconds with length truncation and an incorrect undirected description;
the original 192-token allowance plus concise contract returned a complete
17-token directed-edge/cycle warning in 3.93 seconds. Full requests, responses,
order and limitations are retained in
`evaluation/results/qwen3-8b-advice-output-contract-probe.json`. This is one
sequential sample per setting, not a controlled latency study or independent
quality evaluation. The cancelled provider request's physical termination is
unproven. Full six-profile results from the preceding revision remain unchanged;
after-change collective effectiveness has not yet been measured.

### Concise advice measured through the collective runtime

Source `21970615d1b22439bec35e6082277f99f86b1a1d` completed the same four public
cases across all six profiles in fresh state: 24 tasks in 227.29 seconds. Operator
settings, input hash, workflow and empty document corpus match the preceding
four-case run; the existing loaded Qwen was reused. Results and a hashed summary
are retained in `evaluation/results/qwen3-8b-concise-advice-four-cases*.json`.
Independent exhaustive simple-path enumeration agrees with every recorded score.

Alone/tools/recursive scored 3/4 each; collective 2/4; memory 3/4; experts 4/4.
All four expert tasks completed with three charged calls each and no advice
fallback events. Expert totals: 12 calls, 8 checks, 43.67 seconds task latency,
3940 reported input tokens and 210 output tokens. Their elapsed times were
13.92, 12.24, 12.12 and 5.39 seconds, within unchanged 30-second task budgets.
The previous run's four advice timeouts are preserved in its historical report;
this revised run recorded zero. The frozen integration checks both reports with
one independent oracle and verifies hashes, scores, charges and diagnostic totals.

These are encouraging observations on one small public workflow. Two sequential
runs do not establish causality, stable latency, held-out robustness or a general
expert advantage. Sampling, provider state and whole-device activity were not
controlled; changing collective/memory scores illustrate that uncertainty.
Independent human review, unseen technical evaluation, broader expert provisioning
and physical model lifecycle remain incomplete. No neural training or permissions
were added.

### Expert advice cancellation through the socket service

The real Unix-socket service cancellation regression now covers all four paths
for pending optional workflow advice, as well as ordinary inference: request
client disconnect, service shutdown, cancel-active and task-specific cancel.
The fixture stalls a loopback HTTP provider during expert advice; each path
persists cancelled status and terminal progress where observed. It verifies one
durable call/192-token reservation, unknown provider usage, exactly one advice
phase, no timeout fallback event and no main planner dispatch. Existing busy,
wrong-task/prior-task cancellation and broker cleanup assertions remain intact.
This tests transport/lifecycle cancellation, not physical provider GPU stopping.
No production code, permission or inference budget changed.

### Complete public graph workflow measurement

Source `40e18992a13c9e18b036a21ea94ea03b98427e5c` completed all 96 tasks from
16 public cases across six profiles in 927.43 seconds. The input hash, declared
30-second task/20-second inference ceilings, immutable advisory workflow and
fresh empty document corpus are recorded in
`evaluation/results/qwen3-8b-concise-advice-full-corpus.json` and its hashed
summary. Every pass bit agrees with independent exhaustive simple-path enumeration.
The corpus manifest now marks this public live workflow complete; independent
study, human review, held-out and contamination-control flags remain false.

| Profile | Passed / executed | Charged model calls | Task latency total (seconds) |
|---|---|---|---|
| Alone | 12 / 16 | 16 | 32.12 |
| Tools | 12 / 16 | 22 | 38.79 |
| Recursive | 13 / 16 | 21 | 33.63 |
| Collective | 11 / 16 | 69 | 280.37 |
| Memory | 12 / 16 | 72 | 271.37 |
| Experts | 12 / 16 | 65 | 271.09 |

All 16 optional advice phases recorded zero fallback events, but experts tied
baseline accuracy with substantially more calls/time. The four-case 4/4 result
therefore must not be generalized to this larger corpus. In total: 265 charged
model calls, 128 checks; task outcomes include 60 completed, 16 unverified,
13 rejected, 6 exhausted and 1 failed (inference timeout). All remain counted.
No expert/collective benefit is demonstrated. The single extra recursive pass is
an observation in one run, not proof of causal or general reasoning improvement.

Model weights/provider state, sampling, device activity and host compilation were
not controlled; no 1–3B or larger baseline, unseen technical evaluation or neural
safety claim follows. The same existing Qwen load was reused; no automatic physical
residency ownership is proven. All earlier reports remain unchanged. Broader
module lifecycle, independent verifiers/evaluation and residency gates stay open.

### Intermediate subtask output scope

Subtask prompts now explicitly distinguish intermediate unverified results from
the whole-goal final answer. The final-answer output format applies to the final
solver; original non-format constraints remain applicable to each subtask. The
existing subgoal instruction and untrusted dependency boundary remain present.
A single prompt builder generates both dispatched prompts and admission bounds,
so the added clarification is charged and cannot evade per-prompt/input quotas.
Subtask calls still reserve 192 tokens; final calls and independent verification
retain their existing behavior. No new tool or authority is granted.

A scripted end-to-end regression plans two dependent sorting subgoals, checks the
scope/dependency prompts, then requires the original final output contract and a
compiled verified answer. It retains four calls and 1024 reserved output tokens.
This addresses conflicting instructions; live-model cost/quality benefit after
this change remains unmeasured. Earlier graph measurements remain frozen.

Current source optimized installation: **Implemented and verified** for core and
optional HTTP packages at fc83976 with FLATC unset, locked cached dependencies
and both installed workflow scripts. See INSTALLATION_VALIDATION.md for hashes,
toolchain, source and logs. This refresh does not supersede the old clean-OS scope
or prove live inference, residency ownership or independent reasoning quality.

Installed current real-provider socket execution: **Implemented / Verified** for
FAST sorting and DEEP path reasoning at optimized source fc83976. Both answers
passed compiled independent checks; DEEP executed four advisory subtasks within
original budgets. Service shutdown removed its socket. See LIVE_INFERENCE.md
and the raw installed socket smoke report. General quality/residency remain Partial.

Recovery lease test synchronization: the live arithmetic/SQL regression now waits
up to two seconds after cancellation for physical lease release. Only WouldBlock
errors are retryable; other errors fail immediately. It still proves recovery
is denied while the worker is live, recovers exactly one owned registration, then
returns an empty second recovery. This corrects an immediate-release assumption
without weakening runtime lease enforcement or expanding recovery authority.

### Fresh planning/decomposition phase ceiling

Fresh collective planning plus advisory subtask execution now share one phase
ceiling: one-third of the remaining original task wall time, capped by the
configured inference timeout. `planning_phase_budget` persists the ceiling before
planner dispatch. No subtask resets it. A typed timeout drops pending local work,
accounts elapsed inference, retains reservations, marks usage/queue totals unknown,
marks unfinished subtasks skipped and records `planning_failure` before proceeding
with the original objective and remaining budgets. Partial strategy is discarded;
completed subtask audit results remain explicitly unverified. Policy, scheduler,
provider and audit failures other than typed inference timeouts stay fatal.
Cancellation remains distinct and follows existing terminal task handling.

This leaves time for subsequent whole-goal work, not a guarantee of provider
completion, remote inference termination or measured expected utility. Trusted
synchronous callbacks cannot be preempted by this Tokio phase timeout. Saved graph
continuation keeps its existing original-task ceilings; this new fresh-phase
ceiling is not a general persistent phase-deadline recovery implementation.
Research exploration has its existing separate admission and is not covered by
this planning ceiling. General cost-calibrated scheduling remains Partial.

Paused-clock regressions cover a stalled planner, a completed subtask followed by
a stalled dependent step, retained calls/outputs, discarded partial strategy and
fatal failure-audit denial. Final integer-sort answers still require compiled
verification. Live quality/latency after this change is not yet measured; previous
installed binaries and graph reports retain their original source scope.

### Research exploration phase ceiling

The fresh RESEARCH explorer call now receives a separately audited ceiling of
one-third of remaining original task wall time, capped by configured inference
timeout. `exploration_phase_budget` is persisted before dispatch. Only a typed
inference timeout may become `exploration_failure` and continue the original
whole-goal solver within remaining budgets. Charged calls/output reservations are
retained, elapsed inference is accounted and reported usage/queue totals remain
unknown after interruption. No alternative is appended on timeout. Successful
exploration is still unverified strategy, not evidence or permissions.

Policy/provider/audit errors remain fatal, including a backend error whose text
says "Inference timeout" without the typed timeout condition. Failure auditing
must succeed before the final solver can start. This supersedes the earlier
fresh-planning note that excluded research exploration; it does not claim general
persistent phase-deadline recovery, synchronous-callback preemption, physical
provider cancellation or calibrated expected utility. Original task deadlines,
calls, tokens and permissions remain unchanged. Live effect and current optimized
installation after these phase changes are not yet verified.

Planning/exploration phase handoff: **Implemented / live smoke verified** at
18de498. Real Qwen planner timeout preserved original-budget final solving and
unknown usage; independently checked path completed in 14.48 seconds. Both
optimized packages and installed workflows passed with FLATC unset. See raw
phase smoke and INSTALLATION_VALIDATION.md. Broad effectiveness, synchronous
preemption, persistent phase recovery and residency remain Partial.

Rust reference candidate preparation: **Partial / inactive draft**. The original
source-linked ownership guide and hashed provenance inventory live in examples/.
Installed CLI import/integrity passed; unattested draft quality was denied.
`tests/rust_reference_candidate.rs` proves quarantined content is unavailable to
active retrieval, promotion without quality fails even with approval, and isolated
mechanical retrieval works. Independent acceptance, poisoning/security evaluation,
operator promotion and measured model benefit remain unimplemented/unverified.
Bootstrap rust.s1 therefore remains routing metadata in ordinary fresh workspaces.

### Planning/exploration diagnostics and comparison totals

Evaluation details now include `planning_advice` and `exploration_advice` alongside
existing `expert_advice`. Each contains phases_started, fallback_events,
inference_timeout_fallbacks and unclassified_fallbacks. Comparison arm resource
summaries additionally aggregate all three sets over executed tasks only. These
are audit event counts, not verified success, calibrated utility or remote-provider
termination. A task may fail or cancel without a fallback event; missing events
never establish completion. Historical untyped/unknown kinds remain unclassified,
not guessed from error text. Frozen reports are unchanged and may lack new fields.

One bounded reader projects fixed type labels through SQLite for each phase family:
128 relevant events, 16 KiB per event. It copies no strategy, prompt or error body
into these diagnostics and fails on excess audit size/count. Additional queries
use task scope and bound parameters. New tests distinguish phase families and
unrelated tasks, reject oversized data, verify unknown classifications, project a
real scripted explorer timeout and check arm totals against per-case counts.
No runtime execution policy, permissions or inference budgets change.

### Complete public RESEARCH phase comparison

Source `5a782a2e3a2fc8f7c0e933a9c2dfcb357c234166` completed all 96 tasks from
the sixteen public graph cases in RESEARCH mode, in 813.46 seconds. Each task
retained 30-second wall/20-second inference ceilings and declared shared call,
token and tool limits. Reports and hashed summary are retained in
`evaluation/results/qwen3-8b-research-phase-full-corpus*.json`. Independent exhaustive
simple-path enumeration agrees with every score; diagnostic arm sums agree with
per-task rows. No failed/exhausted task was omitted: outcomes were 64 completed,
16 unverified and 16 rejected. Maximum observed task latency was 21.26 seconds.

| Profile | Passed / executed | Charged model calls | Total task latency (seconds) | Planning timeout fallbacks |
|---|---|---|---|---|
| Alone | 11 / 16 | 16 | 34.38 | 0 |
| Tools | 12 / 16 | 23 | 44.52 | 0 |
| Recursive | 12 / 16 | 23 | 45.37 | 0 |
| Collective | 12 / 16 | 56 | 232.96 | 10 |
| Memory | 12 / 16 | 56 | 205.11 | 9 |
| Experts | 16 / 16 | 69 | 251.06 | 5 |

All 16 expert advice phases and 48 exploration phases recorded zero fallbacks.
The 48 planning phases recorded 24 typed timeouts, with no unclassified fallbacks.
Totals: 243 charged model calls and 147 checker calls. Experts improved five
paired cases over the small-model-alone arm with no observed regression, at more
than four times the calls and over seven times total task latency in this run.
The advice was the public BFS workflow, not a promoted Rust RAG candidate or
trained neural expert. Empty frozen document memory supplies no trained-memory
advantage. These results support bounded phase containment and useful outcomes on
this specific workflow, not a general or causal superiority claim.

This is one project-authored public corpus/run with no held-out independence,
human review, contamination control, 1–3B/larger baseline or independent neural
security study. Weights/provider state, device activity and sampling were not
controlled. Different mode/revision from the earlier DEEP run prevents treating
its score change as a controlled before/after effect. All historical reports
remain unchanged. General lifecycle, phase recovery, calibrated scheduling and
independent quality gates stay open.

Import-free WebAssembly expert foundation: **Experimental / not runtime integrated**.
`experiments/wasm-experts/` is a separate non-publishable Rust workspace pinned to
Wasmi 2.0.0 without WASI or host imports. Five tests exercise integer execution,
import denial, fuel exhaustion including start functions, memory/table ceilings
and malformed/oversized/signature rejection. It supplies no default CLI/API,
model permission, active expert, registry lifecycle or IPC replacement. Core
Cargo dependencies/lockfile remain unchanged. Compilation declaration/local-count
preflight, task/actor grants, version/artifact pinning, cancellation/ownership,
evaluation and rollback integration remain required before claiming executable
expert provisioning. Interpreter bugs/host heap remain independent review risks.

Wasm binary shape admission: **Implemented in the isolated experiment / Experimental
for runtime integration**. `experiments/wasm-experts/src/preflight.rs` bounds flat
integer types/functions/globals/exports, function parameters/results, locals,
control depth, function-body operators, memory declarations and data-segment count
before Wasmi translation. It rejects imports/tables/elements/GC/component shapes,
duplicates and the wrong scalar ABI before start execution. Nine experiment tests
include a tiny huge-local declaration and positive/negative boundaries; full
semantic validation is retained. Production dependencies/authority remain unchanged.
Host heap/time quotas, task grants, immutable artifact/version lifecycle, IPC and
independent security evaluation remain incomplete; no general sandbox claim.

Wasm artifact/invocation binding: **Implemented experimentally / not production
integrated** in `experiments/wasm-experts/src/artifact.rs`. Canonical IDs/versions
and expected hashes bind immutable validated compiled artifacts. Opaque non-cloneable
grants bind one task/input/artifact to the human-cli actor; mismatch/replay is denied
and admitted traps consume the grant. Each call has fresh instance state; compiled
old versions survive source/replacement changes. Thirteen tests pass. Authentication
and approval of grant creation remain the trusted embedding's responsibility;
these comparisons are not remote identity verification. Persistent conflict checks,
cumulative task budgets, audits, lifecycle/recovery and Core permissions/IPC remain
incomplete. No active module, registry or model grant changed.

Experimental Wasm grant lifetime: **Implemented experimentally / not production
integrated**. Grants now have immutable monotonic expiry (at most 30 seconds,
caller-narrowable), cloneable revoke-only handles and atomic admission/commit
ordering. Pre-admission cancellation prevents guest dispatch; cancellation or
expiry after admission suppresses results without refund/replay. Sixteen isolated
tests pass, including actual cross-thread queued cancellation and deterministic
terminal-state/expiry checks without sleeps. Interpreter execution is fuel-bounded,
not forcibly interrupted by revocation; parsing/compilation, authenticated task
budget integration, audits, persistence and general lifecycle remain incomplete.
No production actor or active expert changed.

Experimental Wasm cumulative authority: **Implemented experimentally / not Core
integrated**. One non-cloneable TaskAuthority pins task UUID, original deadline,
one exact version/content per approved artifact ID and 1–8 total allocations.
Only approved artifacts obtain grants; allocation is shared across inputs/artifacts
and never refunded after unused, revoked or trapped work. Authority revocation
cancels issued grants and prevents new issuance. Per-artifact grant constructors
are no longer publicly available in non-test builds. Twenty experiment tests pass.
The trusted embedding must authenticate approval and prevent duplicate independent
authorities per real task. Global/persistent task enforcement, audits, registry
lifecycle and runtime/IPC integration remain incomplete; production unchanged.

Experimental Wasm acknowledged auditing: **Implemented experimentally / durable
Core integration incomplete**. Authorities require a trusted sink for fixed metadata
records at allocation, dispatch reservation and terminal outcome. Allocation audit
failure consumes/revokes the attempted slot; dispatch failure prevents guest entry;
terminal failure withholds output and keeps the invocation spent. Cancellation and
expiry are rechecked after synchronous dispatch acknowledgement. No scalar values,
program/error text or guest-controlled audit callbacks are supplied. Twenty-three
tests include before/after-write failures, uncertainty, ordering and cancellation.
The sink's durability/authenticity and callback latency are trusted embedding
responsibilities, not attested by the prototype. Denial logging, crash recovery,
persistent registry and production actors/IPC remain unimplemented here.

Experimental Core module audit bridge: **Partial / operator embedding only**.
`src/module_tasks.rs` creates fresh tasks with immutable approved identities, a
1–8 invocation limit bounded by configuration, original 1–30 second clock, and
Core execution lease. SQLite immediate transactions validate at most 24 bounded
prior invocation records plus an overflow sentinel and enforce allocation,
dispatch and terminal ordering. Duplicate independent wrappers cannot reset a
recorded allocation. Cancellation closes the task transactionally; model actors
are denied `expert.module.evaluate`. `experiments/wasm-experts/src/core_audit.rs`
connects the existing executor through the optional `core-audit` feature; production
Core does not depend on Wasmi. Tests cover persisted records/reopen, recovery lease,
concurrent duplicate reservations, SQLite write failure, malformed history,
identity substitutions, expiry, inactive tasks and duplicate wrapper reset.
Trusted Rust callers still authenticate operator intent. No automatic resume,
registry installation, CLI/HTTP/model execution endpoint, domain verification or
independent sandbox audit is provided. Cancellation/expiry after guest entry
withholds results; fuel, rather than forced process termination, bounds the guest.

Recorded advisory-result reconciliation: **Implemented for the supported
pre-solver graph boundary / general recovery remains Partial**.
`src/recovery/advisory.rs` and `recover-advisory-result --human-approve` atomically
link a durable cycle-zero solver result to its running subtask after a crash.
The original continuation validator checks pinned context, dependency schedule,
models/modules, charges and same-boot deadline before commit. No inference is
replayed or advice verified. `src/recovery/continuation.rs` tests default/custom
schedules, no approval/live lease, unknown/tampered results, missing schedule,
invalid/expired clocks and audit rollback. `tests/resume_worker.rs` exercises
actual CLI reconciliation followed by real HTTP managed continuation and reaping;
completed advice is not repeated. Other phases and unknown outcomes remain denied.

Native operator lifecycle workflow: **Implemented and real embedding-cycle tested /
automatic ownership-aware residency remains Partial**.
`scripts/native-residency-smoke.sh` defaults to read-only planning, requires an
inactive installed key, and runs one explicit approved load/unload cycle while
checking other loaded IDs/contexts. It does not retry or clean up uncertain/drifted
state. `tests/native_residency_smoke.rs` tests the actual CLI against loopback
fixtures for planning, existing-instance refusal, an approved cycle and unrelated
inventory drift (no unload). One real user-approved Nomic embedding cycle completed
with requested/server-confirmed Core audits and the existing Qwen ID/context
preserved. Frozen evidence: `evaluation/results/lmstudio-native-embedding-lifecycle-smoke.json`.
Generative switching, exclusive ownership, attributed GPU metrics, native authenticated
live mutation and automatic loading decisions are not established by this result.

Ranked affordable expert admission: **Implemented / calibrated utility remains
Partial**. `src/engine.rs` and `src/reasoning.rs` distinguish pre-admission refusal
from an admitted attempt. A refused expensive workflow no longer hides a cheaper
already-routed candidate. At most one workflow is admitted; timeout, partial
advice and provider/audit failures do not buy another attempt. Five regressions
in `src/engine/expert_admission_tests.rs` cover structured call-budget refusal,
legacy prompt-budget refusal, one admitted timeout, fatal provider errors and
refusal-audit failure. Candidate versions, permissions and cumulative task budgets
remain pinned. General module/GPU scheduling and independently measured utility
are not established by these deterministic tests.

Expert backend setup admission: **Implemented** in `src/reasoning.rs`.
Legacy/structured prompt preparation and durable whole-workflow admission now
precede expert backend/worker-handle construction, model snapshots and phase
allocation. Refused candidates no longer look like admitted model bindings or
started advice phases in comparison metadata. Two additional regressions in
`src/engine/expert_admission_tests.rs` cover a refused registered managed backend
with no server/workspace requirement and all candidates refused with only main
reasoning calls. Event ordering verifies admission before phase before dispatch.
Model identity validation, call/token/prompt limits, prompt contents, timeout
fallback and one-admitted-workflow policy remain enforced. This is not a claim of
physical weight loading, exclusive GPU ownership or calibrated expert utility.

Active-aware inference routing: **Implemented for trusted registered inference
modules / general scheduling remains Partial**. `src/fabric.rs` samples queued
plus processing work for equal-rank new pins. Operator ranks still dominate;
existing pins never migrate. A scoped processing observation clears on normal
completion, caller cancellation and broker shutdown. `src/fabric/tests.rs` tests
an active empty queue versus an idle peer, post-completion tie restoration,
unchanged old pins and cancellation/shutdown cleanup; existing priority/queue/
retirement tests remain. `protocol/cognitive.fbs` and `src/wire.rs` add optional
bounded load observations with legacy unknown preservation, round-trip and forged
frame tests. Exact 25.12.19 bindings/manifest regenerated together. Observations
are local samples, not GPU budgets, provider health or capacity reservations.

Inference worker failure containment: **Implemented for local broker queues**.
`src/fabric.rs` combines initialized queue closure with backend observations for
inventory, routing, activation and pinned dispatch. New pins skip dead workers;
old pins fail without hidden retries or budget refunds. Trusted availability
callbacks now run outside registry locks; activation revalidates retained identity
and role/capacity/shutdown invariants afterward. `src/fabric/tests.rs` injects a
callback panic despite Connected advertisement, proves one failed invocation and
future-only healthy selection, and prevents reactivation/retirement resurrection.
The existing reentrant observer test now covers inventory, pinning and activation.
No process sandbox, provider stop proof, automatic reconnect or model mutation
is established. General independent executable isolation remains incomplete.

Tool worker failure containment: **Implemented for arithmetic/SQL broker queues**.
`src/fabric/tools.rs` now excludes closed local queues from inventory, new pinning,
activation and existing-handle dispatch even when the backend reports Connected.
Health callbacks execute outside registry locks; activation revalidates retained
identity and current capability/conflict/capacity/shutdown constraints. Regressions
cover both tool types with deliberate worker panic, no old-handle replay, future
healthy selection, metadata reentry and retirement during observation. Immutable
history remains reserved. No new tool permission, automatic retry, provider-stop
proof, native sandbox or residency operation is introduced.

### Managed unknown-operation resolution and receipt archives

**Partial — implementation integrated; full verification pending.**

- `src/residency/managed/resolution.rs`: original-intent-bound GET-only operator
  resolution, stop acknowledgement, immutable cohort retirement evidence and
  atomic local interruption; no replay, adoption or provider-success claim.
- `src/residency/managed/retirement.rs`: immutable terminal receipt compaction,
  audit validation and historical evidence lookup; archived rows grant no release.
- `src/main.rs`, `src/store.rs`, `src/storage.rs`: explicit CLI gates, schema 8,
  separate bounded quotas and retained original/resolver/archive task references.
- `tests/managed_residency.rs`: unknown load resolution, replay/approval denials,
  terminal archives, and unknown release resolution/archiving regression coverage.
- Remaining gate: complete negative/migration tests and mandatory frozen-source
  checks. Automatic task-to-native-instance lifecycle remains separate work.

### Optional provider-reported role inference timings

**Implemented — focused fixture verification; live accuracy unverified.**

- `src/backend/metrics.rs`, `src/backend/native.rs`, `src/token_admission.rs`:
  optional validated native timing/rate reports; missing remains absent.
- `protocol/module.fbs`, `src/fabric/protocol.rs`: optional typed metrics,
  version/source/range validation and malformed-frame regression coverage;
  reviewed bindings generated with flatc 25.12.19.
- `src/reasoning.rs`, `src/reasoning/parallel.rs`: separate per-call audit events;
  observations do not authorize tools, verify results or increase budgets.
- `docs/PROVIDER_METRICS.md`: supported fields and measurement limitations.
- Physical CPU/GPU/energy attribution and real timing accuracy remain unproven.

## Client-owned MCP, skills and web workflow — 2026-10-25

**Implemented:** `cogfabric-agent` preserves client function descriptions/schemas,
correlated tool results/errors, client skill text and same-model reasoning effort.
The shared native/FlatBuffers catalog limit is 128, with existing aggregate byte,
frame and output-proposal limits retained. `agent_client_capabilities` audits each
turn's names and description/schema hashes before inference; audit failure prevents
inference. Catalog metadata grants no kernel rights and client results remain
unverified. A removed historical tool may retain its correlated result; new calls
must target the current catalog.

The agentic client owns MCP, skill loading, web connections, credentials,
permissions and execution. OpenCode is one interoperability fixture, not a
required dependency or a special core code path. No native MCP client/server is
introduced. The shared API is documented in `docs/AGENT_CLIENTS.md`. Source: `src/agent/{types,
mod,protocol,local}.rs`; tests: agent validation/IPC/audit tests and opt-in
`crates/cogfabric-server/tests/opencode_mcp_client.rs`. See `docs/OPENCODE.md`.
Verification results are recorded below. **Partial:** broader
client APIs and multimodal results; full Jev-R orchestration through this endpoint
is not implemented. Synthetic MCP fixtures establish transport behavior, not real
REA/web capability or model quality.

Verification for this change:

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo +1.99.0 test --locked -p cogfabric agent:: --lib`: 30 passed.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  11 HTTP and 7 managed-worker tests passed.
- Socket-dependent tests required execution outside the restricted socket sandbox;
  the initial restricted run was not treated as a passing result. Two new fixture
  assertion errors were corrected before the successful rerun.
- The full all-target test suite was not run, preserving the operator's request to
  stop reflection tests. No live-model quality/performance benchmark was run.
- FlatBuffers schemas/generated bindings were unchanged. Typed IPC count,
  malformed-message and socket regressions passed; the exact generator pairing
  was not revalidated because `flatc` is absent in this environment (Rust runtime
  remains pinned to 25.12.19). Migration and rollback suites were not rerun.

Security/performance: catalogs remain bounded; expanding the count can increase
model context use, while audit hashing adds work bounded by existing request
limits. This does not broaden kernel capabilities or establish verified success.

Installed-client verification:
`cargo +1.99.0 test --locked -p cogfabric-server --test opencode_mcp_client -- --ignored --nocapture`
passed (1 test, installed OpenCode v2.0.32, 2.02 seconds). OpenCode loaded the
project skill with its actual `id` parameter, advertised `skill` and `execute`,
discovered the fixture MCP functions through Code Mode `search`, invoked both
synthetic REA/web tools and returned the results to CGF. Earlier fixture/config
probes failed and are not counted as passing evidence; direct MCP function
exposure with `codemode:false` was not established. This verifies a client-owned
Code Mode workflow, not live REA/web access or arbitrary client compatibility.
The total focused successful regression count is 49. Full Jev-R orchestration
through the model endpoint and additional client protocol adapters remain the
next separate engineering milestones.

## Client-conversation Jev-R first slice — 2026-10-25

**Implemented per request; full session orchestration remains Partial.** Canonical
architecture sections 4, 5, 7, 9 and 10: `src/agent/collective.rs` integrates a
bounded client-conversation role graph under the existing AgentRuntime task,
model pin, policy, deadline, resource pool and typed managed workers. Shared
`reasoning::mode_limits` supplies existing executive mode ceilings. The operator
enables `Config.agent_collective` with `configure-agent-collective --enabled true`
(default off); request effort selects FAST/STANDARD/DEEP/RESEARCH on the same
`cogfabric-agent` model. Planning, optional exploration, solving and budget-gated
critique/reflection/revision preserve client history and current function schemas.
Only solver proposals may return; client execution remains external. Structural
Verifier-R checks never claim factual verification. Each call is durably reserved
before dispatch; aggregate request token/input budgets, call/cycle caps, deadlines
and cancellation remain enforced. Hash-only traces exclude private message bodies.

**Not implemented:** persistent session identity, cumulative budgets/checkpoint
continuation across client tool turns, general expert-registry integration and
independent factual criteria for arbitrary conversations. Each request is a fresh
task, and client session limits remain necessary. This supersedes the earlier
one-call-only endpoint description only when the option is enabled, not the
remaining full-session gap. See `docs/CLIENT_COLLECTIVE.md`.

`docs/ROADMAP.md` now explicitly places optional image, audio and tentative video
evidence adapters at the end, after core/client completion; all remain planned,
not implemented. The canonical French architecture is preserved unchanged.

Verification for this slice:

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo +1.99.0 test --locked -p cogfabric agent:: --lib`: 41 passed,
  including role/effort preservation, aggregate budgets, input/call fallback,
  no-actionable-critique stopping, audit refusal, cancellation, privacy, typed IPC
  and deadline-before-dispatch denial.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  11 HTTP tests passed; the initial managed run found a new fixture assertion
  counting audit phases as calls. After correcting it, the exact native-worker
  rerun passed all 8 tests, including three distinct owned/reaped collective role
  children, same model/effort, aggregate usage and no tool execution.
- `cargo +1.99.0 test --locked -p cogfabric config::tests::client_collective_setting_defaults_off_and_roundtrips --lib`:
  1 passed.
- `cargo +1.99.0 test --locked -p cogfabric --test client_collective_config`:
  1 passed; enable/disable persistence, invalid input refusal and no database/task
  bootstrap are checked through the real executable.
- `cargo +1.99.0 test --locked -p cogfabric-server --test opencode_mcp_client installed_opencode_v2_jev_collective_owns_mcp_and_skill_workflow -- --ignored --exact`:
  1 passed (installed OpenCode, 3.47 seconds, synthetic MCP fixture/deterministic
  backend); planner/explorer/solver calls and client skill/MCP results coexist.

Total: 63 focused passing tests. Socket/process checks ran with loopback access.
The full all-target test suite and reflection test campaigns were not run, honoring
operator direction; the successful-reflection/revised-answer branch has no new
runtime test in this slice. No live-model quality or performance benchmark was
run. FlatBuffers schemas/bindings and database schema were unchanged; generator
pairing, migration and rollback suites were not revalidated for this change.
A review found and fixed a late-dispatch deadline race before final verification.

Security/performance: no new tool grants or execution were added. Optional roles
consume extra inference and repeat client context; the option defaults off and
charges all reservations within the existing request limits. Managed role workers
add startup/reaping overhead. This establishes mechanics, not a quality gain,
GPU stop guarantee or complete Jev-R session orchestration.


## Durable client tool-loop continuation — 2026-10-25

**Implemented for the bounded `cogfabric-agent` collective; broader Jev+/Jev-R
and seven-subsystem integration remains Partial.** This entry supersedes the
previous client-conversation first-slice entry's statements that session identity,
cumulative budgets and checkpoint continuation were not implemented. Its
verification counts apply only to that earlier code state.

`src/agent/sessions.rs` maps opaque `cgf_<uuid>` tool-call IDs to a durable root
task. Standard client tool-call/result history resumes the checkpoint; the client
does not need a custom session field. Immediate SQLite transactions pin model,
registration, configuration, effort, temperature and original deadline, and
persist session generation, pending exact proposal hashes and cumulative calls,
cycles, prompt bytes, output reservations, proposal counts and internal-tool
charges. Per-request `max_tokens` can bound that step but cannot raise the root
configured ceiling. Atomic single-claim semantics reject concurrent resume.
Changed/missing/replayed calls and results, unknown handles, altered history,
configuration drift and expired sessions fail closed without fresh task budget.

`src/agent/session_context.rs` captures bounded untrusted document, memory, skill
and manifest references once at the root; identities/checksums remain pinned.
`src/agent/session_cleanup.rs` expires at most the bounded waiting-session batch
during later admissions. The recovery path marks active/waiting collective sessions
failed without retry/refund. Only completed waiting-client checkpoints can resume
after process restart on the same Linux boot with unchanged model, registration,
effort and other pinned configuration. Reboot, expired/unavailable original
deadline or pinned-configuration drift invalidates restore; recovery fails closed
without a replacement task or fresh budget. In-flight calls interrupted by
cancellation/crash remain uncertain and cannot replay. Terminal-task retention
removes associated rows; clearing current checkpoint data does not securely erase
WAL history.

Client tools remain external. Optional internal knowledge search is bounded and
charged. Only applicable pure compiled criteria can independently complete their
narrow supported root-goal result; other answers and client results remain
unverified. The endpoint does not expose all Engine expert workflows, arbitrary
custom execution, project-profile/model selection or general `resume-task`.
Disabling the collective refuses its prefixed continuation IDs. See
`docs/CLIENT_COLLECTIVE.md`, `docs/AGENT_CLIENTS.md`, `docs/HTTP_API.md`,
`docs/RECOVERY.md` and `docs/THREAT_MODEL.md`.

Regression sources: `src/agent/sessions_tests.rs`,
`src/agent/session_context_tests.rs`, and `src/store/agent_sessions_migration_tests.rs`.

Focused verification passed:

- `cargo fmt --all -- --check`.
- `cargo +1.99.0 check --locked --workspace --all-targets`.
- `cargo +1.99.0 clippy --locked --workspace --all-targets -- -D warnings`.
- `cargo +1.99.0 test --locked -p cogfabric --lib agent::` (54 tests),
  `schema_` (11), `storage::` (10), `security::` (12), `recovery::` (145),
  `restore::` (5), and `models::tests::` (5).
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  11 HTTP tests passed. After correcting a native-worker fixture assertion,
  `cargo +1.99.0 test --locked -p cogfabric-server --test native_worker` passed
  all 8 tests.
- `cargo +1.99.0 test --locked -p cogfabric-server --test opencode_mcp_client
  installed_opencode_v2_jev_collective_owns_mcp_and_skill_workflow -- --ignored
  --exact` (1 test, synthetic MCP results).
- `cargo +1.99.0 test --locked --test restore_configuration` and
  `cargo +1.99.0 test --locked --test client_collective_config` (1 test each).

The broad recovery filter includes existing revision/reflection recovery checks.
No new successful-reflection branch test, dedicated reflection campaign, live
reasoning-quality benchmark or real REA/web connection was run. FlatBuffers files
were unchanged; the generator pairing could not be checked because `flatc` was
unavailable. This does not establish full seven-subsystem integration or general
agentic-client/protocol compatibility.

## Engine context and on-demand native GPU/provider lifecycle — 2026-10-25

**Implemented:** operator-selected client project context, pinned expert workflow
advice, approved Engine first-use activation, resource-route/restoration support,
and optional NVML package installation. **Partial:** full Engine client execution,
measured/physical GPU scheduling and live-provider validation.

- `src/config.rs`, `src/main.rs`, `src/store.rs`, `src/agent/{sessions,
  session_context}.rs`: immutable profile selection/capture, bounded matching
  expert advice, version/hash pins and configuration drift refusal. Tests in
  `session_context_tests.rs`, `sessions_tests.rs` and `tests/client_agent_project_config.rs`.
- `src/residency/lifecycle/{lazy,restoration}.rs`, `src/residency/lifecycle.rs`,
  `src/backend.rs`, `src/engine.rs`: policy-v3 activation, stable unloaded model
  identity, exact managed receipts, unknown-outcome gates, configured role and
  expert binding validation, same-boot cutoffs, safe release and approved resume.
  `tests/native_lifecycle.rs` covers actual CLI/service/worker and loopback native
  provider interactions; `tests/native_policy_inputs.rs` covers special-file,
  symlink, duplicate-field and policy-size refusal.
- `src/resource_scheduler/*`, `src/recovery/{continuation,
  continuation_resources,resource_routes}.rs`, `src/evaluation/model_metadata.rs`:
  managed targets retain policy scope through admission, evaluation projection and
  recovery without claiming a loaded instance. Exact expert Config identities
  reuse cached adapters under a bounded registry.
- `src/gpu/nvml.rs`, `tests/gpu_observation.rs`, `tests/setup_install.rs`,
  `crates/cogfabric-server/Cargo.toml`, `scripts/setup.sh`, CI: typed protocol
  negative tests, missing-driver diagnostics, feature propagation and GPU build checks.

Security/performance: first-use loading reduces unnecessary resident activation,
but load latency remains inside resource/time admission. Activation is serialized
under the approved coordinator and only confirmed newly loaded instances are
eligible for automatic release. Loading/inference errors can deliberately leave
capacity quarantined until operator reconciliation. Extra project/advisory context
uses the original prompt budget and may displace lower-priority references; it
creates no extra inference call by itself. Provider-reported placement/telemetry
is not a physical VRAM limit, task-attributed energy or proof of GPU termination.
The NVIDIA driver/device is unavailable in this environment; no live GPU speed,
quality or memory result is claimed. Optional image/audio/video work remains last
on the roadmap. The canonical French architecture is unchanged.

### Verification for the Engine/GPU milestone

All commands use Rust 1.99.0 and the existing lockfile. Successful focused library
checks used `cargo +1.99.0 test --locked -p cogfabric --lib FILTER`:

| Filter | Passed |
| --- | ---: |
| `agent::` | 57 |
| `resource_scheduler::` | 33 |
| `residency::` | 17 |
| `backend::` | 11 |
| `recovery::continuation::resource_routes::` | 7 |
| `managed_native` (overlaps the groups above) | 3 |
| `config::tests::client_project_profile_is_optional_canonical_and_roundtrips` | 1 |

`cargo +1.99.0 test --locked -p cogfabric --test native_lifecycle --test native_policy_inputs`
passed 20 lifecycle and 2 policy-input tests on the corrected implementation.
Earlier runs exposed and corrected native API versus inventory URL confusion,
restoration URL comparison, missing main-backend uncertainty after adapter-cache
refactoring, and a root-only unknown-outcome mutation gate. Fixture corrections
use a real managed receipt before testing reuse, distinguish the on-demand root
journal from eager continuation journals, and use the required native policy URL.
A new negative case still refuses unreceipted foreign instances; successful
newly-loaded cleanup has its own regression. No assertion was replaced with a
success stub, and uncertain provider work remains blocked.

No full all-target test campaign or reflection campaign was run, respecting the
operator's earlier direction. The focused native continuation tests exercise
continuation without launching a new reflection campaign. No live-provider quality,
speed/VRAM benchmark, GPU driver repair, new FlatBuffers schema or SQLite migration
was performed. The build validates the pinned reviewed binding bundle; `flatc` is
absent, so fresh compiler generation was not revalidated. Default and optional
NVML parser checks preserve unknown/unavailable measurements rather than zero.

Additional successful checks:

- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets --all-features`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  passed after correcting the lifecycle code and on the final test source.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker`:
  11 HTTP and 8 managed-worker tests passed.
- `cargo +1.99.0 test --locked --features nvml-gpu -p cogfabric --lib gpu::`:
  12 passed (also passed in the default build).
- `cargo +1.99.0 test --locked --features nvml-gpu --test gpu_observation --test setup_install --test client_agent_project_config`:
  2 observation, 1 installer and 1 project CLI tests passed.
- `sh -n scripts/setup.sh`: passed.

The GPU placement regression covers requested true and false values, rejected
response echoes, rejected fresh-inventory disagreement, version-2 receipts and
unknown-outcome mutation blocking. This verifies the adapter contract, not actual
GPU placement. No new live OpenCode run was made for this milestone; the previous
installed-client evidence remains recorded separately.

## Observed GPU admission and Engine HTTP project selection — 2026-10-25

**Implemented:** explicit immutable project selection on the one-goal Engine HTTP
aliases, plus optional observed-memory inference admission. **Partial:** physical
GPU quotas, model-weight/KV fit estimation, independent live GPU measurements and
full Engine expert execution through conversational client turns.

- `crates/cogfabric-server/src/lib.rs` accepts strict `cgf_project: "id@version"`
  only for Engine aliases. Registry preflight rejects invalid/missing versions;
  normal `TaskOptions` preserves exact context/version/checksum. The agent alias,
  multi-turn history, tools and generation overrides retain their existing
  restrictions. HTTP tests verify actual task snapshot evidence and early denials.
- `src/engine.rs` validates selected project context before native lifecycle can
  load weights. `src/residency/lifecycle.rs` and the Engine policy constructor
  refuse eager native policies when observed GPU admission is enabled; v3 remains
  supported. `tests/native_lifecycle.rs` checks both no-I/O boundaries.
- `src/resource_scheduler.rs`, `admission.rs` and `configured.rs` pin optional
  `gpu_observation.{device_uuid,sensor,headroom_bytes}`, preserve omitted defaults,
  expose the setting in snapshots, and check one selected-device observation per
  dispatch. Invalid/unavailable/missing/insufficient metrics refuse provider work.
  Declared memory plus headroom is the threshold; no model-load estimate is inferred.
- GPU evidence is recorded before dispatch. Query-completion age and task state
  are checked again after the synchronous dispatch audit. A late failure invokes
  no provider and conservatively quarantines the recorded dispatch. Ordinary
  preflight refusals release only unstarted logical allocations; original model
  reservations remain charged. `src/recovery/continuation_resources.rs` requires
  exact guard intent, observation values/order and dispatch-age evidence, while
  preserving unguarded legacy histories. Tests mutate/drop/invent that evidence.

Security/performance: the host chooses the device and sensor; models gain no
telemetry, model-change or execution capability. Every enabled dispatch adds a
bounded sensor query and audit work while holding its logical allocation. Sampling
and other processes' allocations can race, and no physical VRAM/energy isolation
is established. Unknown late boundaries may require operator reconciliation.
Defaults add no GPU query. Project context is bounded untrusted data and remains
inside existing inference/context budgets. Documentation updates: HTTP API,
resource scheduling, GPU observations, threat model, current snapshot and roadmap.

Verification (Rust 1.99.0, existing lockfile):

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets`: passed.
- `cargo +1.99.0 check --locked --workspace --all-targets --all-features`: passed.
- `cargo +1.99.0 clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  passed after correcting one new test initializer style issue.
- `cargo +1.99.0 test --locked -p cogfabric --lib resource_scheduler::`: 38 passed.
- `cargo +1.99.0 test --locked -p cogfabric --lib recovery::continuation::resource_history::`:
  3 passed, including guarded/legacy, missing/altered/unexpected observation, and
  required/altered/unexpected dispatch-age evidence.
- `cargo +1.99.0 test --locked -p cogfabric --test native_lifecycle`: 21 passed.
- `cargo +1.99.0 test --locked -p cogfabric-server --test http --test native_worker --test resource_scheduling`:
  13 HTTP, 8 worker and 1 shared-capacity test passed.
- `cargo +1.99.0 test --locked -p cogfabric --test resource_route_resume --test working_profile_resume --test agent_working_profiles`:
  3 tests in each target passed; original route/profile and legacy continuations
  remain verified. An earlier command used the server test target in the core
  package and an unmatched filter; those selections are not passing evidence.

The full all-target test/reflection campaign was not run, preserving operator
scope. New GPU admission tests use a private deterministic observer; this is not
live GPU validation. Read-only checks still found no working NVIDIA driver or
local model endpoint. No quality/speed benchmark, GPU driver modification,
FlatBuffers schema change, database migration or new rollback campaign was made.
Reviewed binding integrity is checked by the build; `flatc` is unavailable for
fresh generation. The next live milestone requires a usable driver/provider;
physical memory enforcement and model-aware capacity planning remain separate work.
