# v1.0 release acceptance audit

This checklist preserves the 18 acceptance criteria in the goal objective. It is
an evidence inventory, not a release approval. The canonical French architecture
and the broader implementation matrix remain authoritative. Earlier test counts
in the quality review are historical milestones. Current local verification is
526 Rust tests (468 library including the server, 58 integration), formatting, all-target checking and
Clippy with warnings denied. Default builds use bundled generated sources and
Rust FlatBuffers 25.12.19. Exact native generation/byte comparison was also
revalidated for the new optional load observations.

| Goal criterion | Evidence inspected | Assessment / remaining proof |
|---|---|---|
| 1. Workspace builds | `Cargo.toml`, all-target check | Passed locally; isolated optimized source installation also passed with cached dependencies. |
| 2. Formatting and Clippy | Mandatory checks in `QUALITY_REVIEW.md` | Passed locally. |
| 3. Mandatory tests | Workspace all-target test results, `tests/`, server integration tests | 526 passed; fixtures do not establish live model quality. |
| 4. FlatBuffers IPC | `protocol/`, `src/wire.rs`, `src/fabric/`, malformed/compatibility/socket tests | Implemented for present transports; authenticated process event delivery/replay is now tested; broader module compatibility remains partial. |
| 5. CLI/service communication | `src/service.rs`, socket tests, `tests/process_worker.rs`, `crates/cogfabric-server/tests/` | Tested typed local transport, progress, cancellation and optional HTTP client interoperability; native and collective ownership drains cover dropped request futures. No general independent plugin service is claimed. |
| 6. Compatible local LLM | `src/backend.rs`, `tests/inference_auth.rs`, `LIVE_INFERENCE.md` | Passed for Ollama and LM Studio Qwen3 8B compatible endpoints, including managed verified sorting. Native inventory and one operator-approved embedding load/unload passed; authenticated native access and generative switching remain unverified. |
| 7. Jev+ routes real tasks | `src/engine.rs`, registry/routing tests, built-in SQL execution | Deterministic tools, registered advice routing and trusted replaceable dependency ordering work; general module/GPU scheduling and independently established expert usefulness remain incomplete. |
| 8. Bounded Jev-R revision | `src/reasoning.rs`, scripted revision and budget tests | Enforced calls/cycles/tools/time and measured public Qwen graph/Rust workflows; broader independent effectiveness remains unverified. |
| 9. Skill registration/discovery | `src/registry.rs`, `EXPERTS.md` | Versioned RAG/workflow/compiled-checker registrations work; arbitrary executable loading is absent. |
| 10. Persistent memory | `src/store.rs`, migrations, retrieval/reopen/rollback tests | Implemented bounded persistence and fresh structured-answer checks; general fact verification and physical disk bounds remain partial. |
| 11. Budgets/cancellation | Session/broker/worker tests, `RECOVERY.md` | Current execution ceilings and approved recovery of pinned custom dependency orders tested; exact tokenizer/GPU ceilings and general automatic/within-call interrupted execution remain incomplete. |
| 12. Security regressions | `security-eval`, policy/transport/artifact tests, `THREAT_MODEL.md` | 31 fixed boundary probes (25 expected denials) plus regression tests. Live Qwen reference injection contaminated answers; quoted-reference mitigation resisted that attack once. Broader poisoning, sandbox escape and peer identity evaluation remain incomplete. |
| 13. RAG evaluation/promotion | `src/evaluation.rs`, `src/evolution.rs`, promotion/rollback tests | Transactional reviewed promotion with integrity/regression gates; dataset independence is operator-attested, not independently established. |
| 14. Lifecycle documented | `MODELS.md`, `EXPERTS.md`, `WORKERS.md` | Metadata lifecycle documented; explicit native load/unload is fixture-tested; real native inventory and one approved embedding load/unload passed; generative switching and ownership-aware automatic scheduling remain incomplete. |
| 15. Reproducible benchmarks | `src/benchmarking.rs`, `evaluation/`, `BENCHMARKING.md` | Six-arm runner and public live workflows exist for Ollama and Qwen3 8B; reviewed unseen dataset and held-out equal-budget model study missing. |
| 16. Accurate matrix | `IMPLEMENTATION_MATRIX.md` | Current summary reconciled with 526-test evidence, present live endpoint and public graph/Rust diagnostics; historical paragraphs are explicitly labeled as a ledger. |
| 17. Verified docs/installation | `README.md`, HTTP_API.md, CI workflow, INSTALLATION_VALIDATION.md | Optimized host installs and offline workflows passed at prior source `e1f3ffd` without flatc; the new recovery source needs a final install refresh; historical installed managed OpenCode/Qwen read/edit/read passed. Expanded clean Arch setup, all checks and installed core/server workflows passed at `a9933b3`. CachyOS desktop/GPU setup remains unverified. |
| 18. Honest experimental claims | `EVOLUTION.md`, `MODELS.md`, `THREAT_MODEL.md` | Neural adaptation remains experimental and neural promotion denied; no trained Jev or superiority claim. |

The current concise-advice runtime has a complete four-case live Qwen workflow
report: all 24 profile tasks ran, every score was independently recomputed, and
expert advice recorded zero fallbacks with four verified expert-arm answers.
This supersedes neither historical failed results nor the missing independent
held-out study. See `evaluation/results/qwen3-8b-concise-advice-four-cases.json`
and its hashed summary. The full 16-case run has now completed all 96 tasks with independently checked
scores: experts tied alone at 12/16 with 65 versus 16 charged calls, and no advice
fallbacks. This completes a public workflow measurement, not the missing
independent held-out study. See its `concise-advice-full-corpus` report.

## Arithmetic/SQL failure lifecycle gate — 2026-10-14

Closed tool worker queues now dominate optimistic backend health. Existing handles
fail without retry; future handles can select an approved healthy alternative.
Observation callbacks run outside registry locks, followed by activation identity
and constraint revalidation. Both arithmetic and SQL have worker-failure,
metadata-reentry and retirement-race regressions. All 526 workspace tests and
mandatory quality checks pass. Permissions, typed protocols and resource ceilings
are unchanged; native sandboxing and global scheduling remain incomplete.

## Failed inference worker lifecycle gate — 2026-10-14

Closed local queues override optimistic backend health for inventory, routing,
activation and pinned dispatch. Old pins fail without replay; new pins can select
an approved alternative. Health callbacks run outside registry locks, followed by
activation identity/conflict/capacity/shutdown revalidation. Tests cover a trusted
callback panic, no hidden retry, metadata reentry and retirement during observation
without resurrection. All 524 workspace tests and mandatory quality checks pass.
No OS sandbox, external provider stop, automatic reconnect or general model/GPU
scheduler is established by this local failure-containment change.

## Active-aware inference routing gate — 2026-10-14

New pins include processing work when comparing equal-rank candidates, preserving
operator ranks and existing pins. Inventory carries optional bounded queued/
processing observations through the typed socket protocol. Legacy absence stays
unknown; partial/out-of-bound observations are rejected. Broker lifecycle tests,
wire round-trip/forgery tests and a real socket assertion pass. Exact flatc 25.12.19
regeneration and byte comparison pass, as do compiler-free builds and all mandatory
quality checks (522 tests). Local samples do not attest provider execution or GPU
residency, reserve capacity or implement ownership-aware global scheduling.

## Expert backend setup gate — 2026-10-14

Whole-workflow admission now precedes backend/worker construction, expert model
snapshots and phase allocation. Refused model-bound candidates no longer affect
admitted-model comparison metadata or advice-phase counters. Two new tests verify
that boundary and the all-refused path; the existing injected audit-failure test
now checks the stronger stage-specific snapshot invariant. All 520 workspace tests,
formatting, check and Clippy pass. Prompts and resource ceilings remain unchanged;
this does not establish actual weight residency, calibrated utility or broader
module scheduling. Historical phase counts retain their source-scoped semantics.

## Ranked workflow admission gate — 2026-10-14

The runtime now considers another already-routed workflow after pre-admission
budget refusal, preserving router order and pinned versions. At most one workflow
is admitted: timeout, partial advice or provider failure never buys another attempt;
refusal-audit failure is fatal. Five regressions exercise structured call cost,
legacy prompt cost, timeout, provider failure and audit failure. All 518 workspace
tests and mandatory quality checks pass. This improves bounded selection; it does
not establish calibrated expected utility or general GPU/module scheduling. Earlier
installation and live results remain scoped to their recorded source revisions.

## Native operator lifecycle gate — 2026-10-14

One explicitly user-approved real Nomic embedding cycle now verifies the existing
Core native load/unload path with durable requested/server-confirmed audits. The
Qwen instance ID/context baseline survived. The new workflow defaults to
inspection, refuses pre-existing selected instances and stops without cleanup
on unrelated inventory drift or uncertainty. Its four fixture scenarios pass via
the actual CLI; the complete workspace has 513 passing tests and all quality
checks pass. Frozen inventories/reports/audits and reproduction are documented in
`MODELS.md` and `evaluation/results/lmstudio-native-embedding-lifecycle-smoke.json`.
This closes the absence of any real native mutation evidence. It does not close
generative model switching, exclusive ownership, authenticated live native access,
automatic residency scheduling, weight attestation or independent evaluation.

## Advisory-result recovery gate — 2026-10-14

Current runtime adds explicit recovery of the crash window after advisory inference
was recorded but before its subtask result was written. All 512 workspace tests,
formatting, checking and Clippy pass. Tests cover default/custom committed order,
original charges and aging deadline, unknown/corrupt/wrong-role/out-of-order results,
expired/missing clocks, audit rollback, no-approval/no-state creation, live leases,
and actual CLI reconciliation followed by managed HTTP continuation/reaping with
only two new calls. Advice remains unverified, task status/answer are preserved,
and no inference or tool is replayed. General within-cycle/other-boot/unknown-outcome
recovery remains incomplete. Earlier installed `e1f3ffd` evidence below is preserved
with its source scope; repeat installation against the final runtime before release.

## Acceptance reconciliation at e1f3ffd — 2026-10-14

Source reviewed and installed: `e1f3ffde2f416944b68a9d760dec8ca170c3dcc9`. The workspace test log reports 508 passing
tests; optional `core-audit` reports 24. Mandatory fmt/check/test/clippy passed with
`FLATC` unset. The bounded security command now tests operator module permission,
inference-actor denials and denial of that capability under reasoning task scope.
This adds diagnostic coverage, not a new production execution grant.

Fresh debug CLI evidence at `/tmp/cogfabric-v1-audit.qM0zKR` verifies initialization,
arithmetic self-test, FlatBuffers round-trip, schema 6 diagnostics, actual service
status, socket arithmetic answer 36 with zero inference and one tool, and clean
STOP. Real Qwen3 8B compatible inference returned a Rust explanation in 8309.62 ms
with one call, 158 reported input tokens and 243 reported output tokens; status was
correctly unverified. Endpoint advertisement is not weight attestation or native
residency ownership. These temporary paths are local observations; existing smoke
scripts provide portable reproduction. A separate optimized-install refresh at `e1f3ffd` subsequently passed for both
packages, including the complete CLI and HTTP scripts. See
`INSTALLATION_VALIDATION.md` for exact hashes and source scope; the debug inference
smoke itself does not establish optimized model quality.

The complete public RESEARCH graph report now covers all 96 tasks: expert arm
16/16 versus alone 11/16, 69 versus 16 model calls, approximately 251 versus 34
seconds of cumulative task latency. Twenty-four planning phases used typed timeout
fallback. This is a single project-authored public study with different modes and
runtime revisions from older reports; it establishes neither a causal/general
advantage nor an independent equal-budget held-out result. The manifest still
marks human review, held-out independence and independent study false. The
100-case V1 manifest also lacks independent review/live completion; the inactive
Rust reference candidate has only same-author draft retrieval cases.

Scope reconciliation: the goal permits tool, reference, workflow, adapter or
independent-model experts; it does not require Wasm as the universal expert ABI.
Existing versioned registry/rollback should be reused rather than replaced.
The optional Core/Wasm bridge is partial evidence toward executable modules,
not proof of general provisioning, lifecycle or domain correctness. The canonical
French roadmap remains unchanged; optional offline LoRA may stay experimental
under the owner's engineering instructions. Native ownership-aware model
switching, general module scheduling/provisioning, broader recovery, independent
technical/security evaluation and final source-install validation remain open.
No final release or tag is approved by this reconciliation.

## Next verifiable milestones

1. Implement remaining general module lifecycle/routing with deterministic
   authorization, immutable version references and bounded queues. Existing
   in-process handles and single-task worker authentication do not prove this.
2. Continue interrupted task graphs only after validating saved context, original
   budgets, permissions, pinned dependencies and remaining wall time. Same-boot
   wall-time reconstruction is now tested; other-boot/legacy allowances stay unknown. Current
   `recover-verified-result` completes checked saved answers without replay;
   audit inspection alone never authorizes continuation. Explicit pre-solver default-module
   continuation, including validated registered models, managed workers and unconsumed
   solver-cycle boundaries with completed, skipped or absent graphs, now has HTTP and failure regression tests; general backend/phase
   arbitrary continuation inside a consumed cycle is still incomplete; recorded
   structured failed answers can now prepare a freshly checked revision. Repeated unconsumed
   boundaries now preserve original context and cumulative limits.
3. Extend fixture-tested explicit native load/unload into residency scheduling
   with enforceable ownership and recoverable uncertain outcomes. Current native
   instance IDs do not prove exclusive cross-client ownership; no automatic
   cleanup is inferred from a successful load. Validate real model switching.
4. Extend the successful direct/managed Ollama integration to reproducible live
   studies; validate LM Studio-specific authentication separately if used. Establish an available small generative baseline, review
   the held-out technical corpus, then execute comparable model studies. Never
   search token files, weaken server authentication or substitute mock scores.
5. Repeat the verified source-install/offline workflow against the final implementation,
   additionally verify clean-OS prerequisites and live setup, then run the complete release gate. Preserve unavailable GPU attribution
   and energy measurements as unavailable.

Novel neural architectures and automatic neural promotion are not substitutes
for these infrastructure requirements. The goal explicitly permits optional
offline LoRA to remain experimental. Private GitHub publication was explicitly
authorized and performed. GitHub authentication was renewed and the private
repository synchronized through runtime commit `75e7e8a` on 2026-10-12. No new final release tag is authorized.

## Installed release binary evidence

At source commit `abb23c1f77aefa12c8b505ee71fbc972b004a163`,
`cargo install --offline --locked --path . --root /tmp/cogfabric-v1-install-audit-20261011`
with `FLATC=/tmp/cogfabric-flatbuffers/build/flatc` successfully compiled and installed
an optimized executable in a separate directory. Rust/Cargo 1.97.1 and flatc
25.12.19 were already available; dependencies came from the local cache. Binary
SHA-256: `1c9897138bd3e309cdfbb4a244969915c6a8efbb6ea9eff1b89de9239f7c6edc`.
This is not evidence of a fresh operating-system package installation.

`scripts/installed-smoke.sh` passed against that binary using a new temporary
workspace. It verifies arithmetic/SQL with zero inference calls, 24 security
boundary probes, checker registration/activation/health, reference ingestion and
retrieval, the ten-case arithmetic example, two reviewed example RAG promotions,
rollback to the first candidate, configuration/RAG artifact backup restoration,
restored self-test and an actual Unix socket ask/stop with socket cleanup.
Local JSON evidence is retained at `/tmp/cogfabric-installed-smoke.2yzLfs`.
Temporary evidence paths are observations, not portable repository artifacts;
the script provides reproduction. The example dataset's operator review flags
establish no independent held-out status or live-model result. Current CI is
configured to run the workflow with its built binary; remote CI has not been run.


### Current installed tool workflow verification — 2026-10-11

The optimized executable built from Rust source commit
`69190875aef58b821e6daa37e9dbd887698bba64` was installed with
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-tool-install-audit-20261011`.
Rust/Cargo 1.97.1, FlatBuffers 25.12.19 and cached locked dependencies were used.
The release build completed successfully (45.05 seconds reported by Cargo).
Executable SHA-256:
`4b0145d6323a9e9b81ddf342e535dfaa10f709bdc638d69bf6428bd56d176e42`.
Only smoke-script/documentation changes followed that Rust source commit.

The expanded `scripts/installed-smoke.sh` passed against that installed executable
in a new temporary workspace. It retains executable/script hashes and version
metadata along with JSON task traces. In addition to the earlier offline workflow,
it verifies one-call arithmetic worker launch/authorization/confirmed reaping,
cleanup after rejected arithmetic, SQL capability dispatch with no child launch,
restored worker configuration and actual worker execution after database/RAG
restore, and both arithmetic and SQL socket-service asks. All six new execution
cases used zero model calls. The rejected arithmetic case exited with status 1;
service STOP succeeded and removed the socket. Final local evidence is retained
at `/tmp/cogfabric-installed-smoke.Q1YToY`.

Mandatory formatting, offline all-target check/tests (292 passed: 272 library,
20 integration) and Clippy with warnings denied also passed. This supersedes the
older installed source evidence for tool routing. It does not establish clean-OS
prerequisite setup, real inference/authentication, model switching, independent
held-out results, executable sandboxing or neural safety. Future runtime changes
must repeat installation verification before release; the project remains alpha.

SQL Unix transport source now passes all mandatory checks and 301 tests (281
library, 20 integration), including a separate-process SQLite exchange. See
`QUALITY_REVIEW.md` for evidence. The installed executable recorded above predates
this change; installation verification must be repeated before release. This adds
a trusted-host transport API, not automatic SQL worker provisioning or sandboxing.

Fresh installed verification of SQL transport source (2026-10-11): commit
`af921687775177dbbdf2ed9fc865eb6dfdb1ab84` was installed with
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-sql-unix-install-20261011`.
Cargo reported 45.59 seconds for the optimized build. Executable SHA-256:
`b8d8bc58051acabd3f62ea2f8cc24dea093297c5061c2cc8d8fa9a156cc88d33`.
`scripts/installed-smoke.sh` (SHA-256
`02a8d1849612a3da264fbe778729bf24f2f5565bca9b83f450901fc3cb3c89e7`)
passed against that executable; evidence is retained at
`/tmp/cogfabric-installed-smoke.MImFGj`, with log
`/tmp/cogfabric-sql-unix-smoke.log`. This supersedes the installed-source gap
immediately above. It verifies the existing installed CLI/service workflow;
SQL Unix RPC itself is exercised by the separate-process Rust regression, since
CLI/service SQL worker provisioning remains unfinished. Live inference, clean-OS
setup and neural safety were not evaluated.

Managed SQL CLI/service integration now passes mandatory quality checks and all
304 tests (283 library, 21 integration). The expanded smoke script passed against
the current debug executable, including SQL configuration, rejected-query reaping,
restored SQL worker execution and socket SQL. Earlier installed evidence predates
this source; repeat optimized installation verification before release. General
executable lifecycle, crash cleanup and live inference remain incomplete.

Optimized managed SQL verification (2026-10-11): source commit
`04a3a03d4c0e25c16e45b488e43f60bf62f6aa7e` was installed with
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-managed-sql-install-20261011`.
The optimized build took 45.88 seconds as reported by Cargo. Executable SHA-256:
`576c3f01250bf058989dfd8a767f0c9b89fbb5e51cdab6fb2a843c11876a1783`.
The expanded installed smoke script (SHA-256
`fbb32f6b3997fc836341f673ed30871f9380288655b0e02ff28ae2032317254b`)
passed, including actual managed SQL CLI/service execution, rejected-query reaping
and restored SQL worker configuration. Evidence:
`/tmp/cogfabric-installed-smoke.KWPBZI`; log:
`/tmp/cogfabric-managed-sql-installed-smoke.log`. This supersedes the pending
optimized-install gap above. It does not establish clean-OS setup, real model
inference/switching, independent held-out results or crash cleanup.

Inference startup ownership now survives interrupted/failed handoff; all mandatory
checks pass with 306 tests (285 library, 21 integration). Actual child tests cover
cancellation and audit failure, retry refusal and confirmed cleanup. The smoke
script now checks demo inference launch/ownership/handoff/reaping and passes against
the debug executable. Prior optimized-install evidence predates this runtime change;
repeat installed verification before release. Crash cleanup and live inference
remain incomplete.

Optimized inference-startup verification (2026-10-11): source
`fcd64652bffad1c046d24fbae58c8d7c5eb6bd28` was installed with
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-inference-startup-install-20261011`.
Cargo reported 45.75 seconds for the optimized build. Binary SHA-256:
`7cbbe8d572711320829cedf98294c31632ca8bda2b94849dfca6c386754f739c`.
Smoke script SHA-256:
`4e07b549ed4aabe3b19506ac151ea6adc7f723b213bf488eb0f490a4d6bb10cf`.
The installed workflow passed, including demo inference launch intent, owned PID,
completed handoff and matching confirmed reap, in addition to SQL/arithmetic,
service, skill rollback and restore. Evidence:
`/tmp/cogfabric-installed-smoke.TOVIs8`; log:
`/tmp/cogfabric-inference-startup-installed-smoke.log`. This closes the pending
installed-source gap above. Real authenticated inference, host-crash cleanup,
clean-OS setup and independent live benchmarks remain unverified.

Mixed inference/math/SQL worker audit recovery passes mandatory checks and all
309 tests (288 library, 21 integration), including live tool lease exclusion and
mixed rollback/idempotence. The expanded debug smoke workflow confirms recovery
is empty after clean completion. Prior optimized evidence predates this source;
refresh installed verification before release. This is audit recovery, not
stale-directory cleanup or proof of completed external effects.

Optimized mixed-worker recovery verification (2026-10-11): source
`dbb9c47dfe5a8c7ed2f3710054873982e10e2729` was installed with
`FLATC=/tmp/cogfabric-flatbuffers/build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-tool-recovery-install-20261011`.
Cargo reported 45.63 seconds for the optimized build. Binary SHA-256:
`3e94eefd0686b688b50df7ed6b4d156e2577ffab726bfd2506f48fc5b09903c4`.
Smoke script SHA-256:
`3b87a03d93397c14073359b64a2ac659ada908b7caf4a37623300969dece1d17`.
The installed workflow passed, including clean inference/tool completion followed
by empty `recover-workers`, along with CLI/service, rollback and restore checks.
Evidence: `/tmp/cogfabric-installed-smoke.4mpTND`; log:
`/tmp/cogfabric-tool-recovery-installed-smoke.log`. This closes the pending
installed-source gap above. Abandoned-session behavior is proven by mixed/live
socket Rust regressions; the smoke script does not simulate a host crash. Stale
files, general module lifecycle and live-model release gates remain open.

Journaled worker-file cleanup now passes mandatory checks and 318 Rust tests
(297 library, 21 integration), including actual child-process death and negative
filesystem/plan/lease/audit boundaries. Expanded debug CLI/service checks pass.
Earlier installed source predates the verified cleanup changes; optimized installed
validation must follow. This closes the tested journaled cleanup slice, not older
unjournaled resource cleanup, hostile-owner race isolation or automatic recovery.

Optimized worker-file cleanup verification (2026-10-12): source
`054505c6b314c1de0820314a203b4dcc83108167` was installed with
`FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-worker-files-install-20261012`.
Rust/Cargo 1.97.1 and FlatBuffers 25.12.19 were used; Cargo reported 44.24 seconds
for the optimized build. Binary SHA-256:
`1abadf9a4b2188ef53e5a50dc124fb6d4012437c9e1b15b578c2f095517c3560`.
Smoke script SHA-256:
`a951e08f31ee0dcb733e6fd4ab21fb75aa74b2fa43c2044d86dbd9d29957e301`.
The installed workflow passed, including reviewed empty cleanup preview/apply,
CLI/service workers, recovery, rollback and restore. Evidence:
`/tmp/cogfabric-installed-smoke.N40kRN`; log:
`/tmp/cogfabric-worker-files-installed-smoke.log`. This closes the installed-source
gap above. Actual stale-file deletion after host death is covered by the Rust child
regression, not the empty-plan smoke case. Live inference, automatic resource
scheduling and broader module lifecycle remain incomplete.

Optimized tool-rollback verification (2026-10-12): source `c8b4a7e` was installed
with `FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-tool-rollback-install-20261012`.
Binary SHA-256:
`896b4679573a78636cbc21183a010cd372d10165a8755557a0320ca5d6636344`.
Smoke script SHA-256:
`a951e08f31ee0dcb733e6fd4ab21fb75aa74b2fa43c2044d86dbd9d29957e301`.
The installed CLI/service, worker cleanup/recovery, skill rollback and restore
workflow passed. Evidence: `/tmp/cogfabric-installed-smoke.7BRTBp`; logs:
`/tmp/cogfabric-tool-rollback-install.log` and
`/tmp/cogfabric-tool-rollback-installed-smoke.log`. This refreshes installation
proof after the runtime change. Live observations in `LIVE_INFERENCE.md` used the
debug executable separately; installed smoke still uses deterministic demo models.
No clean-OS setup, independent benchmark or neural safety claim follows.

Optimized inference-rollback verification (2026-10-12): source `dec46fa` installed
successfully with `FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-inference-rollback-install-20261012`.
Cargo reported 48.89 seconds. Binary SHA-256:
`6aec07685580e3e519571e5a1047af52e07592b1b3fcf93c5e50f98217766354`.
The unchanged smoke script passed; evidence:
`/tmp/cogfabric-installed-smoke.4S9cEL`; logs:
`/tmp/cogfabric-inference-rollback-install.log`,
`/tmp/cogfabric-inference-rollback-installed-smoke.log`.
A separate fresh installed socket service also passed real Ollama sorting with
independent proof, confirmed worker reaping and service socket removal; see
`LIVE_INFERENCE.md`. This closes the installed-source gap for inference rollback
and adds a real installed service case. No clean-OS setup or benchmark gain is claimed.

Optimized inference identity verification (2026-10-12): source `8506352` installed
with `FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-inference-identity-install-20261012`.
Binary SHA-256:
`a18761118b0042f0ae1df720a740333ca89e7fc615d921fb14dae7c8a6bef98e`.
The unchanged installed smoke script passed; evidence:
`/tmp/cogfabric-installed-smoke.k32MVh`; logs:
`/tmp/cogfabric-inference-identity-install.log`,
`/tmp/cogfabric-inference-identity-installed-smoke.log`.
This closes the pending installation gap for transport identity enforcement.
Actual transport discovery/role matching is exercised by the child regression;
installed smoke uses deterministic demo inference, not a new live quality study.

Optimized event rollback verification (2026-10-12): source `2ac215b` installed
with `FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-event-rollback-install-20261012`.
Binary SHA-256:
`6c745c0e497c183833e0f54093267eb92d858f1766774fd466e05862239d4bd9`.
The unchanged installed smoke workflow passed; evidence:
`/tmp/cogfabric-installed-smoke.Zz06rw`; logs:
`/tmp/cogfabric-event-rollback-install.log`,
`/tmp/cogfabric-event-rollback-installed-smoke.log`.
This closes the installed-source gap for event rollback. Event rollback semantics
are established by Rust live/replay/capacity regressions; installed smoke exercises
the existing CLI/service integration and deterministic inference.

Optimized interface reservation verification (2026-10-12): source `a918e18`
installed with `FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-interface-budget-install-20261012`.
Binary SHA-256:
`11924942eba5fc3dc8418aa81beed3d04cf680f75880c378b68d58a35dcd51d7`.
Installed smoke passed; evidence: `/tmp/cogfabric-installed-smoke.IddNUQ`; logs:
`/tmp/cogfabric-interface-budget-install.log`,
`/tmp/cogfabric-interface-budget-installed-smoke.log`.
An installed CLI check also persisted 384 interface tokens and rejected 4097
without changing the stored setting. This closes the installed-source gap; live
interpretation still timed out in the separate debug trial (`LIVE_INFERENCE.md`).

Optimized provider-control verification (2026-10-12): source `418b722` installed
with `FLATC=/tmp/cogfabric-flatbuffers-build/flatc cargo install --offline --locked --path . --root /tmp/cogfabric-v1-provider-effort-install-20261012`.
Binary SHA-256:
`5a60d4911f37aeedeb5e180fb73a2067d0ed69e0838cd681634168696fc9b69a`.
Installed smoke passed; evidence: `/tmp/cogfabric-installed-smoke.aN0baA`; logs:
`/tmp/cogfabric-provider-effort-install.log`,
`/tmp/cogfabric-provider-effort-installed-smoke.log`.
Real interpretation with explicit effort still failed strict JSON parsing and
exhausted its two calls on interface/planner, exposing remaining-budget planner
admission work. See `LIVE_INFERENCE.md`; no successful structured goal is claimed.

Optimized planner-admission verification (2026-10-12): source `5c41ac2` installed
successfully (Cargo 50.27 seconds) under
`/tmp/cogfabric-v1-planner-admission-install-20261012` with the pinned FLATC and
locked offline dependencies. Binary SHA-256:
`d4067942b4cbce48c6d4889778fc1031dfa80d4f346732f3d23a4b958c0c7792`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.kA3vOZ`; logs
`/tmp/cogfabric-planner-admission-install.log` and
`/tmp/cogfabric-planner-admission-installed-smoke.log`.

Real language integration now succeeds for one bounded STANDARD case on debug
source `3b186cd`, with accepted structured goal, original-goal solver, audited
planner skip and confirmed child reaping. Output remains unverified. See
`LIVE_INFERENCE.md`; controlled quality and synthesis measurements remain open.

Optimized final language-path verification (2026-10-12): source `3b186cd`
installed with the pinned compiler and locked offline dependencies under
`/tmp/cogfabric-v1-fenced-intent-install-20261012`. Cargo reported 1m 08s.
Binary SHA-256:
`42f3d1160fce9ce5402a1927b1490c384ac02ea3a9264da9633968b53c72ab63`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.W7hlXf`; logs
`/tmp/cogfabric-fenced-intent-install.log` and
`/tmp/cogfabric-fenced-intent-installed-smoke.log`.
This closes the installed-source gap for exact intent fences and remaining-budget
planner admission. Real natural-language evidence used the debug binary separately;
installed smoke remains deterministic. No new stable release is declared.

Optimized sorting-comparison verification (2026-10-12): runtime source `3eb870c`
installed with the pinned compiler and locked offline dependencies under
`/tmp/cogfabric-v1-sort-comparison-install-20261012` (Cargo 49.36 seconds).
Binary SHA-256:
`6200bccea222ec588ea2344be85a9bd21b3451cf6c52d19b3e61aa5c781bfb58`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.lAMiLy`; logs
`/tmp/cogfabric-sort-comparison-install.log` and
`/tmp/cogfabric-sort-comparison-installed-smoke.log`.
Actual six-arm sorting scoring is covered by the HTTP fixture regression; smoke
covers installed CLI/service behavior. Subsequent test-only recovery assertions
also pass all mandatory checks and do not change installed runtime source.
No independent held-out or live comparative performance study was run.

Optimized advisory JSON verification (2026-10-12): source `9b8385a`
installed with locked offline dependencies and the pinned FlatBuffers compiler
under `/tmp/cogfabric-v1-advisory-json-install-20261012`. Cargo reported 2m 02s.
Binary SHA-256:
`c8c72a990aa7246c6e4c0b5b58da8f7ea3a8d4a8a88076a79795ee725353a51a`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.32rG5a`; logs
`/tmp/cogfabric-advisory-json-install.log` and
`/tmp/cogfabric-advisory-json-installed-smoke.log`.
The separately recorded debug live trial exercised planner, solver and critic
within three calls; its prose remains unverified. All 335 tests and mandatory
quality checks passed for this source. Held-out comparative quality, intended
small-model measurements and other open release gates remain incomplete.

### Live comparison preflight observation — 2026-10-12

The optimized `9b8385a` executable rejected a proposed one-case public sorting
workflow check before task execution because `compare` requires `--held-out`.
The flag was deliberately omitted: the repository example has no independent
held-out review. Exit status was 1 with the explicit held-out/code-revision/run
bounds preflight error. No comparative result was produced. Evidence:
`/tmp/cogfabric-live-comparison.YafT5R/{cases.jsonl,stderr.txt,exit-status.txt}`.
The attempted provenance license argument was mistakenly MIT; the repository
license is Apache-2.0 and must be used for a future repository-example check.
The preflight refusal does not validate that incorrect provenance declaration.

A separate explicit non-held-out workflow-check mode would permit honest live
runner verification while retaining mandatory held-out attestations for benchmark
claims. That mode is not currently implemented. Do not bypass the gate by falsely
attesting the public example or presenting fixture results as live measurements.

Optimized workflow-check verification (2026-10-12): runtime source `55cd52d`
installed using locked offline dependencies and pinned FlatBuffers 25.12.19 under
`/tmp/cogfabric-v1-workflow-check-install-20261012`; Cargo reported 3m 20s.
Binary SHA-256:
`ea10a25942287a42b4a1b424fcf8aa98606e2473684e447190ef465f26309fdc`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.yHGC9O`; logs
`/tmp/cogfabric-workflow-check-install.log` and
`/tmp/cogfabric-workflow-check-installed-smoke.log`.
The separately recorded debug six-profile live workflow completed with five
checked answers and one baseline timeout. It is explicitly public and non-held-out;
no benchmark gain or small-model readiness follows. All six workers were reaped.
This supersedes the missing public workflow mode noted above, while preserving
independent held-out evaluation and other unfinished release requirements.

Optimized inference/tool availability verification (2026-10-12): source
`bb3c987` installed with locked offline dependencies and pinned FlatBuffers
25.12.19 under `/tmp/cogfabric-v1-availability-install-20261012`. Cargo reported
50.22 seconds. Binary SHA-256:
`e5fbecb21fa16596f10b314a9b9720647f1f98c3ad40f62001445fa4df009ddc`.
The unchanged installed smoke script (SHA-256
`a951e08f31ee0dcb733e6fd4ab21fb75aa74b2fa43c2044d86dbd9d29957e301`)
passed; evidence `/tmp/cogfabric-installed-smoke.1rflCQ`; logs
`/tmp/cogfabric-availability-install.log` and
`/tmp/cogfabric-availability-installed-smoke.log`.
This closes the installed-source gap after inference and tool availability changes.
Malformed-response routing/rollback refusal is established by actual Unix Rust
regressions; installed smoke verifies normal CLI/service/worker/restore paths.
Availability remains a local stream observation: grant expiry/call exhaustion,
provider health and external computation completion are separate concerns.
No new live model quality, general lifecycle or clean-OS claim follows.

Optimized grant-aware managed availability verification (2026-10-12): runtime
source `aa95552` installed with locked offline dependencies and pinned FlatBuffers
25.12.19 under `/tmp/cogfabric-v1-managed-availability-install-20261012`.
Cargo reported 49.11 seconds. Binary SHA-256:
`a80d7b96e56d0c54ffdcd5868ad53d8f006598084beb0b079c7eef2c18ff9a0c`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.x5a40p`; logs
`/tmp/cogfabric-managed-availability-install.log` and
`/tmp/cogfabric-managed-availability-installed-smoke.log`.
This closes pending installed-source gaps for expired/exhausted grant routing
and managed wrapper propagation. The actual socket/child and paused-clock Rust
regressions establish those failure semantics; installed smoke covers normal
CLI/service execution, worker cleanup, rollback and restore. Automatic residency
scheduling, general independent plugin provisioning and held-out live evaluation
remain incomplete. No release approval or remote publication follows.

Optimized terminal shutdown verification (2026-10-12): source `99c939f`
(runtime unchanged from `a2f37c0`) installed with locked offline dependencies and
pinned FlatBuffers 25.12.19 under
`/tmp/cogfabric-v1-terminal-shutdown-install-20261012`. Cargo reported 50.39 seconds.
Binary SHA-256:
`cfe929286de88abbb9b392326e763c75c41ae2718b17164be1d8f41b157515c9`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.ofDhda`; logs
`/tmp/cogfabric-terminal-shutdown-install.log` and
`/tmp/cogfabric-terminal-shutdown-installed-smoke.log`.
This closes the installed-source gap for terminal managed shutdown. Actual Rust
regressions separately verify no launch after shutdown and initialization-gate
coordination before owned-child cleanup; installed smoke covers normal process,
CLI/service, backup/restore and rollback behavior. No general plugin isolation,
external provider cancellation or stable-release claim follows.

Real model-selection evidence now includes two successful managed tasks on existing
Ollama quantizations, with respective provider-reported residency digests before
and after the transition. Both independent sorting checks and worker reaping
passed; see `LIVE_INFERENCE.md`. This proves provider-managed configurable model
selection, not framework-controlled load/unload, exclusive ownership, native
LM Studio lifecycle or neural adapter hot-swapping. Those gates remain open.

A bounded live adversarial-reference trial retrieved a forged permission/goal
instruction but timed out before the interface produced a response. Cleanup and
charged failure were recorded; this is explicitly inconclusive for live injection
resistance, not a security pass. Details and the exact artifact hash are retained
in `LIVE_INFERENCE.md`. Deterministic negative permission/schema regressions remain
valid, while live injection/poisoning robustness remains an open evaluation gate.

Optimized ranked inference routing verification (2026-10-12): source `d66dc07`
(runtime unchanged from `2a94b93`) installed with locked offline dependencies and
pinned FlatBuffers 25.12.19 under
`/tmp/cogfabric-v1-ranked-routing-install-20261012`. Cargo reported 1m 52s.
Binary SHA-256:
`c8ca2fce60ef719782410208ae3c02134e91c5a79b01c99e8f495b862d0b6c73`.
Installed smoke passed; evidence `/tmp/cogfabric-installed-smoke.FzCnAB`; logs
`/tmp/cogfabric-ranked-routing-install.log` and
`/tmp/cogfabric-ranked-routing-installed-smoke.log`.
This closes the installed-source gap for ranked inference candidates. Actual
rank selection, immutable pins and authenticated Unix destinations are verified
by Rust regressions; the installed workflow exercises ordinary CLI/service,
workers, rollback and restore. No adaptive cost, model-quality or general plugin
provisioning claim follows. Tool capability alternatives remain unimplemented.

## Latest installed service verification — 2026-10-12

The latest runtime `161dea5` installed from `9b0cf59` passes the existing full
installed smoke and actual LM Studio Qwen3 8B inference submitted with `ask --socket`.
Independent sorting verification, normal managed-worker reaping, clean service
exit and socket removal were observed. Exact binary hash, timing and evidence
paths are in QUALITY_REVIEW.md and LIVE_INFERENCE.md. This supersedes earlier
installed-runtime evidence for the SQL/reference changes, while clean-OS setup,
held-out quality/security studies and general lifecycle requirements remain open.

## Hosted compiler-bootstrap gate — 2026-10-12

[Run 38295462158](https://github.com/DrapNard/cognitive-fabric/actions/runs/38295462158)
passed for exact commit `fb7f9ec061dbb42b4ce2da6f81c9af6b3d397449` in 3m21s.
The shared pinned compiler bootstrap, mandatory Rust checks, CLI workflows,
security boundary evaluation and offline training preflight all passed on the
hosted runner. This closes the unobserved remote-CI gate for that commit while
retaining the broader functional, held-out evaluation and end-user installation
gaps listed above. A passing pipeline is not a final v1.0 release approval.

## Optimized tool-inventory runtime verification — 2026-10-13

Source `75e7e8ab83bffb2641754bdde605bb687b2f1102` installed with locked offline
dependencies and flatc 25.12.19 in 51.81 seconds at
`/tmp/cogfabric-v1-inventory-install-20261012`. Binary SHA-256:
`efed5f21d521dddeedb2915e95a3877b12024264d2571a505a576af4db86ff35`.
Expanded installed smoke passed, including local/socket tool inventory:
`/tmp/cogfabric-inventory-installed-smoke.log`, evidence
`/tmp/cogfabric-installed-smoke.IFjruK`. This closes the optimized installation gap
for typed inventory and the prior prompt-format/constraint changes.

An initial natural-language sorting request with default thinking failed at its
384-token reservation after 7875.163655 ms (task
`b70fef4e-ee99-49ef-95a9-31e075439df1`). The runtime reported the output-limit failure
without exposing hidden reasoning or retrying. With documented `configure-local
--reasoning-effort none --interface-tokens 384 --temperature 0`, the installed
executable submitted `sort:{"values":[5,-3,5,1]}` to LM Studio Qwen3 8B and returned
`[-3,1,5,5]`, independently verified by `integer_sort/v1`, task
`841b8d13-22cb-4063-b859-a9796771e72b`: one call, one cycle, 574.764228 ms elapsed,
573.428862 ms inference, 176 input/14 output tokens, 384 reserved tokens and two
tool charges. Files: `/tmp/cogfabric-inventory-installed-qwen.json` and
`/tmp/cogfabric-inventory-installed-qwen-sort.json`. This is one public direct
inference smoke, not evidence of general reasoning gains, held-out quality or
neural safety. Defaults do not guarantee useful answers from thinking models.

## Hosted typed-inventory verification — 2026-10-13

GitHub Actions run [38313961901](https://github.com/DrapNard/cognitive-fabric/actions/runs/38313961901)
passed for source `75e7e8ab83bffb2641754bdde605bb687b2f1102`, including verified
compiler bootstrap, formatting, check, Clippy, all tests, CLI/service smoke,
security probes and offline training preflight. Watcher exit status was zero;
log `/tmp/cogfabric-inventory-hosted-ci.log`. This supplies hosted fresh-checkout
evidence for typed inventory, not model quality or neural safety. Checkout Node
runtime and upcoming ubuntu-latest image warnings remain maintenance items.

## Clean Arch container validation — 2026-10-13

Source `35c4bdc6c4d4dcab5bc5c819fe94bd983f0eeef4` passed a fresh rootless Podman
run using official Arch image digest
`sha256:363129b67b2bc6c5a634f8fb4961a7e2eba213a2ec075d159127b1260290a722`.
No host Cargo cache, credentials or Git metadata were mounted; tracked source was
archived into a read-only temporary mount. Container package setup installed Rust
1.99.0, Cargo 1.99.0 and system FlatBuffers 25.12.19. The reviewed pinned compiler
bootstrap also passed. Fresh locked dependency downloads, formatting, all-target
check, all 359 tests (338 library, 21 integration) and Clippy with warnings denied
passed. Optimized install completed in 2m12s and the full installed smoke passed,
including socket tool inventory, security probes and managed worker cleanup.
The driver exited zero. Retained host log: `/tmp/cogfabric-arch-clean-install.log`.
Installed binary SHA-256:
`a8924a69ec8bf9bf9469b8f22f4f0795e5db26b676e4addfe7b21cdf6ada7fbd`.

The disposable container was removed normally; its smoke path
`/tmp/cogfabric-installed-smoke.irCFWl` is historical container-local output, not
a retained host artifact. `INSTALLATION_VALIDATION.md` supplies the reproducible
workflow; README now includes build/smoke prerequisites and locked commands.
This closes the clean Arch container prerequisite/build/install gap, not a fresh
CachyOS desktop, live GPU setup, plugin sandbox or model quality evaluation. Arch
package repositories remain rolling despite the pinned base image; versions are
recorded above. Broader module lifecycle and independent evaluations remain open.

## Optimized inference inventory installation — 2026-10-13

Source `da2a8b9910afc9e782aa270531991d0c94430050` installed with locked offline
dependencies and flatc 25.12.19 in 51.44s at
`/tmp/cogfabric-inference-inventory-install-20261013`. Binary SHA-256:
`e4056d00b8804680e75b91279dcd8b391941d7ad124160b4d0323d1b2358853e`.
Expanded installed CLI/service smoke passed, including inference inventory on
local and socket paths. Log `/tmp/cogfabric-inference-inventory-installed-smoke.log`,
evidence `/tmp/cogfabric-installed-smoke.dTE5ul`. This closes the optimized install
gap for typed inference inventory; the clean Arch result remains source `35c4bdc`.
No live inference or neural safety evaluation is implied by this smoke.

Hosted CI run [38315452946](https://github.com/DrapNard/cognitive-fabric/actions/runs/38315452946)
passed for `da2a8b9910afc9e782aa270531991d0c94430050`, including compiler bootstrap,
mandatory checks, all tests, expanded CLI/service smoke, security probes and
offline training preflight. Watcher exit zero; retained log
`/tmp/cogfabric-inference-inventory-hosted-ci.log`. Node-runtime and runner-image
migration warnings remain maintenance items. This is hosted workflow evidence,
not an independent model-quality evaluation.

## Full provisional Qwen workflow diagnostic — 2026-10-13

The fixed `da2a8b9` executable completed all 100 cases × six profiles, 600 tasks,
in 1138.772610 seconds; command exit zero and report `complete:true`. Strict
criterion matches were alone 2, tools 22, recursive 22, collective 50, memory 50
and experts 49 out of 100 each. These are not causal reasoning-gain measurements:
output-contract asymmetry, format-sensitive scoring, unreviewed authored labels,
empty reference/specialist corpus, uncontrolled warmup and overlapping host build
activity limit interpretation. There were 118 completed, 438 unverified, 32
rejected and 12 clarification outcomes, with no failed tasks. Raw public answers,
per-category results, resources and detailed caveats are retained under
`evaluation/results/`; this generated report is committed specifically for
reproducible evidence inspection, without weights, credentials or private data.
The raw checksum is
`b4fa58005f2e0498047826404642a275335ef11fac2a5efd761006f6a9596299`.

The original process ended before the corrected full rerun started. The rerun
uses fresh state, identical budgets and frozen binary SHA-256
`a2012ac53ec77dee168d4e2aadb1dca7067864b419a3ddd6e0db8f47e5c792d0`
at source `9fec58bc93fd97fb3096147360dc67681a7974b7`. Its initial temporary config
contained an unsupported home field; validation rejected it before task creation
or inference, and it was removed before the actual run. No corrected scores are
claimed before terminal completion. Independent review, a 1–3B baseline, larger
baseline, actual specialized corpus and wider security/quality studies remain open.

Hosted CI passed for `ba5dac1` (run 38316401789) and `9fec58b` (run
[38316751254](https://github.com/DrapNard/cognitive-fabric/actions/runs/38316751254)),
including compiler bootstrap, mandatory checks, tests, CLI/service smoke, security
probes and offline training preflight. The second watcher exited zero; log
`/tmp/cogfabric-tool-format-parity-hosted-ci.log`. These checks establish source
workflow verification, not benchmark quality or neural safety.


Corrected public Qwen3 8B output-contract parity workflow completed on 2026-10-13:
600/600 tasks at source `9fec58b`, 988.471 seconds, strict matches per 100 of
7 alone, 21 tools, 21 recursive, 51 collective, 48 memory and 50 experts. All 500
inference workers stopped normally. Full raw answers and category/resource summaries
are retained in `evaluation/results/`; see its README for exact hashes, configuration,
status interpretation and limitations. This supersedes earlier pending-run statements
only; independent label review, held-out evaluation, 1–3B/larger baselines and causal
quality/performance evidence remain incomplete. Host compilation overlapped the run.


## Optimized Linux worker image installation — 2026-10-13

Source `ef572c822063dac98f80950fab7042e69dd3d112` (runtime worker fix
`30a4c63`) installed successfully with `cargo install --offline --locked --path .
--root /tmp/cogfabric-worker-image-install-20261013`, FlatBuffers compiler 25.12.19
and cached dependencies. Cargo reported an optimized build time of 1m40s.
Binary SHA-256:
`d2a59d5a2dabb699ddba3d1163387bed8577af9b8640125ad2a3abb55011f91a`.
The complete installed smoke passed, including CLI/service, inventory, worker
cleanup, permissions, rollback and restore. Script SHA-256:
`e670653a0601fae59d96ce2c41f4d2871054bcc85e1de2eb6c7886b284e333dc`.
Local temporary evidence: `/tmp/cogfabric-installed-smoke.4Iot1W`; logs:
`/tmp/cogfabric-worker-image-install.log` and
`/tmp/cogfabric-worker-image-installed-smoke.log`. The executable-replacement
condition is separately covered by the Linux Rust service integration regression;
ordinary installed smoke alone does not exercise replacement. No live inference,
fresh OS installation or neural safety claim follows.

Hosted CI [38319222910](https://github.com/DrapNard/cognitive-fabric/actions/runs/38319222910)
completed successfully for exact source `30a4c63c3e463a48c549d21258e714d8ccdf34fa`.
Compiler bootstrap, formatting, check, Clippy, tests, CLI/service smoke, security
probes and offline training preflight passed. Watcher exited zero; local log:
`/tmp/cogfabric-worker-image-hosted-ci.log`. The broader release blockers remain.


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


## Optimized retirement lifecycle verification — 2026-10-13

Source `a665df998699000e0f64839b06582d4411729cc1` installed with locked offline
cached dependencies, Rust/Cargo 1.97.1 and flatc 25.12.19 under
`/tmp/cogfabric-retirement-install-20261013`. Cargo reported 52.15 seconds.
Binary SHA-256:
`294c31c71b2539a7ec3e1a8dda575b8e4b24c851ff67724d45578e9bd02a7958`.
Full installed CLI/service smoke passed; evidence `/tmp/cogfabric-installed-smoke.DgbpqN`;
logs `/tmp/cogfabric-retirement-install.log` and
`/tmp/cogfabric-retirement-installed-smoke.log`.

Separately, `cargo test --release --offline --lib retirement` passed both actual
inference and arithmetic/SQL retirement regressions at the same source. These
verify existing pinned execution and actual backend release under optimization;
ordinary installed smoke does not invoke trusted-host retirement APIs. Release
test build took 1m29s; log `/tmp/cogfabric-retirement-release-tests.log`.
This refreshes the optimized installation gap for both retirement APIs. Model
weight unloading, broad plugin provisioning and independent evaluation remain open.

Hosted CI [38319713203](https://github.com/DrapNard/cognitive-fabric/actions/runs/38319713203)
completed successfully for inference retirement source
`e362746ad335f8b4bb63e3b993629345e229c7b5`. Hosted tool-retirement verification
remains separately tracked; no result is inferred from this earlier successful run.


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


## Optimized registry cleanup and live Qwen verification — 2026-10-13

Source `b81fe34cec0e2eec183ca06ebaf194966f5162d8` passed optimized installation,
full installed CLI/service smoke and hosted CI
[38320574430](https://github.com/DrapNard/cognitive-fabric/actions/runs/38320574430).
Installed managed Qwen3 8B returned an unverified STANDARD ownership explanation
and an independently checked FAST sorting result; both workers were reaped normally.
`QUALITY_REVIEW.md` records installation hashes and CI evidence; `LIVE_INFERENCE.md`
records exact live task IDs, settings, resources and limitations. This closes the
installed-source gap for cleanup lock ordering, not general module provisioning,
provider unloading, held-out evaluation or broader V1 release requirements.


Equal-rank alternatives now use sampled local queue occupancy before the ID
tie-breaker, without changing rank priority or existing task pins. Mandatory checks
and 372 tests pass; exact gated coverage and limitations are in QUALITY_REVIEW.md.
This adds a bounded routing heuristic, not calibrated inference cost or automatic
residency scheduling. Optimized installation proof needs refreshing after this change.


Zero-tool ceilings now allow plain model inference while skipping optional
retrieval; actual arithmetic/SQL/check dispatch still fails before tool use.
Mandatory checks and 373 tests pass, and a managed live Qwen trial returned an
unverified explanation with one call and zero tools. See QUALITY_REVIEW.md and
LIVE_INFERENCE.md for exact evidence and remaining installed-source gap. This
does not establish general natural-language constraint enforcement or finish V1.


Explicit arithmetic and inline SQL now skip unrelated retrieval and pass under
one tool allowance, with zero model calls and original host-side result checks.
Mandatory checks, 374 tests and debug CLI/service smoke pass; QUALITY_REVIEW.md
records exact coverage and the pending optimized installation refresh. General
provisioning, residency scheduling and independent evaluation remain incomplete.


Optimized installation and expanded CLI/service smoke passed for runtime source
`8b30fd1`, closing the installed-source gap for queue routing, zero-tool inference
and retrieval-free inline tools. The expanded script verifies one-tool socket
execution, restored budget persistence, zero-tool direct/socket demo inference and
zero-budget socket dispatch denial before worker launch. QUALITY_REVIEW.md records
exact binary/script hashes, logs and scope. Queue-routing and zero-tool source CI
passed; the inline-tool source run remains separately tracked. The Rust runtime
retains its verified 374-test count; the expanded smoke changes no runtime code.
General provisioning, residency scheduling and independent evaluation remain open.

## Optional agent-facing HTTP service

The core/serve separation now includes `cogfabric-server`, with authenticated
loopback OpenAI-compatible model discovery and Chat Completions. Native client
conversations/tools and separate collective goal aliases are exercised by
workspace tests. Actual OpenCode v2.0.26 + Qwen3 8B completed a restricted
read/edit/read fixture; see HTTP_API.md and QUALITY_REVIEW.md. This adds usable
external agent interoperability without granting host tools to the Fabric model.
414 workspace tests pass; broader API/client
compatibility, independent agentic-quality evaluation and new-server optimized
installation validation remain incomplete. This does not close the full v1 gate.

Native conversation model registrations now use exact enabled metadata snapshots
with real HTTP disable-during-inference regression coverage. This closes the
registration limitation for native turns, while independent worker provisioning, provider
weight attestation and ownership-aware residency remain incomplete.

Native conversations now have authenticated same-image worker RPC, immutable
one-task grants, scoped credential forwarding, audit-before-dispatch, bounded
process capacity and actual child reaping tests. Managed OpenCode + Qwen read/edit/read
also passed, with all five workers reaped normally. This closes the native-worker
transport gap for the supplied binaries. Independent executable provisioning,
attestation/isolation, native role profiles, broader API compatibility, new-server
optimized installation and independent agentic evaluation remain open.

## Optimized managed-agent verification (2026-10-13)

Runtime source `d807e9e` passed locked offline release builds for both workspace
binaries (1m48s), using flatc 25.12.19 and cached dependencies. The optimized core
passed `scripts/installed-smoke.sh` with fresh state; evidence:
`/tmp/cogfabric-installed-smoke.OCsbbk`. The optimized server passed the managed
OpenCode/Qwen3 8B read/edit/read smoke; evidence:
`/tmp/cogfabric-opencode-smoke.yDN43Y`. The script checks actual fixture contents
and equal nonzero admitted, launched and reaped worker counts.

Binary SHA-256: core `5f55fc94a287ef06a0db135b2a8a729393eb4e2f06529ad3a327e1debbf7af77`;
server `b3954ff1357f8359b31f518f7f64d714e5f371d44ba25c5aae7265ce7ea7800b`.
Hosted CI [38377800359](https://github.com/DrapNard/cognitive-fabric/actions/runs/38377800359)
passed for the same runtime source. This closes optimized binary workflow
verification, not clean-OS/package installation or independent agentic evaluation.
General expert provisioning, residency scheduling and full V1 acceptance remain open.

## Collective dropped-future ownership

The previous native-only HTTP drain now also waits for collective inference/tool
ownership, including dynamically selected expert inference owners. Bounded
reapers retain socket guards and execution leases through child waiting when
task futures drop. Subprocess regressions prove startup cleanup, HTTP disconnect,
immediate shutdown and sticky failure when the stop audit is unavailable.
This addresses the collective dropped-future ownership gap for supplied same-image
workers. Forced host termination, untrusted plugin isolation and independent
agentic/reasoning evaluation remain outside this verification. Optimized binaries
recorded above predate this ownership change; installation refresh remains open.

## Installed core and HTTP server verification (2026-10-13)

Runtime source `38948b112a8ddad8aa16133d1dac5b06aeb048c2` passed separate
locked offline `cargo install --path .` and `cargo install --path
crates/cogfabric-server` into `/tmp/cogfabric-collective-cleanup-install-20261013`.
Build times were 1m30s and 1m05s using cached dependencies and flatc 25.12.19.
Installed core CLI/socket smoke passed: `/tmp/cogfabric-installed-smoke.KLFG3z`.
Installed HTTP offline smoke passed: `/tmp/cogfabric-installed-http.hLt7Ab`.
It verifies five authenticated aliases, unauthenticated/origin denials, actual
managed arithmetic/SQL results with reaping audits and no model dispatch, and
successful server shutdown. Script SHA-256:
`950ff16da845b970466e5af0b335dde9733e457760b4b81a65cc4dfc81976723`.

Installed managed OpenCode v2.0.26 + Qwen3 8B read/edit/read also passed:
`/tmp/cogfabric-opencode-smoke.04PbE7`. Actual fixture contents and matching
nonzero admitted/launched/reaped counts are checked by that existing script.
This is interoperability evidence, not independent coding-quality evaluation.
Core binary SHA-256:
`978cf090f81296bbe848472798c9155a97554e72f2456fbf584b7df467f57b6a`.
Server binary SHA-256:
`0e92638902ed376703794ee112871d9e14ca05c01329d452c005f98dc7f45b4e`.
Hosted CI [38381031102](https://github.com/DrapNard/cognitive-fabric/actions/runs/38381031102)
passed for this runtime source. These tests close the current host source-install
verification gap for both packages, including collective cleanup. Clean-OS
validation of the new server, plugin isolation, ownership-aware residency and
independent reasoning/expert evaluation remain open.

## Clean Arch core/server acceptance verification (2026-10-13)

Tracked source `a9933b323c54d6441210e104ec59d37808370357` passed the expanded
INSTALLATION_VALIDATION.md workflow in a fresh rootless Arch container using
`docker.io/library/archlinux@sha256:363129b67b2bc6c5a634f8fb4961a7e2eba213a2ec075d159127b1260290a722`.
Only read-only source and the validation script were mounted; no host Cargo
cache, credentials, Git metadata or model weights were shared. Prerequisite
installation and fresh dependency downloads succeeded. Rust/Cargo 1.99.0 and
flatc/Rust FlatBuffers 25.12.19 were used. Formatting, workspace all-target check,
all 417 tests and Clippy with warnings denied passed. Optimized core and server
installs completed in 3m37s and 1m41s respectively. Both installed offline
workflow scripts passed, including actual authenticated HTTP discovery, origin
denial, arithmetic/SQL child reaping and graceful shutdown.

Core binary SHA-256:
`9b371386214bb6c43a87d3fff5dc441148e20e86461c0b0ec9b136dfa1dd84e2`.
Server binary SHA-256:
`784bd85c90b758076b4eb87c3564b73b854965a42aed9a9bb2a9b4aebeb64bf2`.
Retained terminal log: `/tmp/cogfabric-clean-server-validation-20261013.log`,
SHA-256 `150cee98aceafb2c7f470a1bf1276bab6fbf10d7af7033bdd2f60c778c95305e`.
Read-only source snapshot: `/tmp/cogfabric-clean-server-source-20261013`.
The disposable container exited successfully and was removed; its temporary
smoke JSON directories were not exported. Successful script assertions and
binary hashes remain in the log. No native model inference, GPU integration,
CachyOS desktop setup or untrusted-plugin security evaluation was performed
inside the container. The separate host live OpenCode evidence remains valid
within its own stated scope.

Hosted CI [38381768218](https://github.com/DrapNard/cognitive-fabric/actions/runs/38381768218)
passed for this exact source, including the new offline HTTP smoke step. These
results close the clean-OS core/server installation gap; independent expert
quality, broader module provisioning, ownership-aware residency and complete
release acceptance remain incomplete. No release tag or public publication.

## Read-only residency inspection verification

`residency-inspect` validates a single bounded original operator intent and its
explicit native endpoint before one fresh inventory GET. It preserves original
task state, configuration and registry; observed desired state cannot finalize
the task, establish ownership or authorize retry/cleanup. Parsing rejects unknown
fields, invalid action bounds, missing/duplicate intents and oversized records
before HTTP. SQL checks payload length before JSON extraction and owned copies.
The CLI opens only an existing current-schema read-only database before normal
workspace initialization. Missing-workspace inspection creates no state.

Formatting, workspace all-target check, all 419 tests (378 library, 41 integration)
and Clippy with warnings denied pass. Log:
`/tmp/cogfabric-residency-inspection-tests.log`. CLI tests use actual local HTTP
fixtures for present/absent load/unload state and negative pre-network cases,
including unchanged task/configuration and no skill bootstrap. Mutation behavior
is unchanged except sharing the same action validation helper. No model was loaded
or unloaded; live authenticated operation inspection and optimized/clean-OS
installation of this added command are not yet verified. Previous clean Arch
evidence remains scoped to `a9933b3`. General exclusive residency ownership,
automatic lifecycle scheduling and complete V1 acceptance remain open.

### Inspection credential-boundary regression

The real inspection CLI now has a separate-authentication fixture regression:
only the dedicated residency token reaches its native inventory endpoint, while
a separate inference token is not forwarded. Unicode-escaped provider echoes are
rejected without printing provider metadata or changing the original task.
All 420 workspace tests and mandatory format/check/Clippy gates pass; log:
`/tmp/cogfabric-residency-inspection-auth-tests.log`. No production credential
was accessed and no real model was loaded/unloaded. This extends fixture coverage,
not live authentication or exclusive residency ownership proof. MODELS.md records
the reviewed public REST ownership limitation and its exact scope.

### Rust compiler diagnostic corpus

The new public 20-case corpus has balanced yes/no labels, versioned source/dataset
hashes and specific compiler error codes. The fixture test compiles only fixed
repository Rust 2021 library sources as metadata, with ten-second waits, two-second
kill/reap cleanup on timeout and kill-on-drop. It never executes snippets or model
output and expands no runtime permissions. All 421 tests and mandatory checks
pass; log `/tmp/cogfabric-rust-ownership-corpus-tests.log`. Compiler labels were
checked locally with rustc 1.97.1. Public authoring does not establish human review,
training independence, held-out robustness or model expertise.

### Frozen Rust diagnostic rerun

After the solver-format clarification, source `b04b5b5` matched 8/20 strict labels
in 51.657 seconds, with 20 calls, zero tools, 4,327 reported input tokens and 780
reported output tokens. The before run matched 6/20 in 95.053 seconds with 3,987
input and 1,452 output tokens. Dataset bytes, generation/resource configuration,
exact scoring and question order were unchanged; each used fresh isolated state.
All runtime answers remained unverified. Zero-based exact-match indices 1, 7 and
16 improved, while index 14 regressed. Extra prose and incorrect Rust judgments
remain. Both raw reports, summaries, source/dataset/binary hashes and the no-call
admission preflight are retained under evaluation/results.

This is a single public before/after diagnostic, not a randomized controlled
study, held-out robustness result, proof of reasoning improvement or Rust expert
readiness. Output-contract adherence remains imperfect; no labels or scorer were
relaxed to raise scores. Timings include host dispatch and are not GPU compute or
energy measurements. No compiler capability was granted to the model.

### Full basic-benchmark preflight verification

All benchmark rows, empty/size/field rules and the count ceiling are validated
before Engine admission. Regressions prove invalid later rows create no tasks
and dispatch no inference, expected labels stay out of model prompts, and no-call
exhaustion remains distinguishable from deterministic tool success. Reports add
read-once dataset SHA-256, mode, runtime status counts and charged model/tool task
counts; existing scores/details and missing token semantics are unchanged.

All 426 tests (383 library, 43 integration), formatting, workspace all-target
check and Clippy pass. Log: `/tmp/cogfabric-benchmark-preflight-tests.log`.
Actual CLI arithmetic smoke passed with ten verified tool results and zero model
calls: `/tmp/cogfabric-bench-report-smoke.TbaewW`. No new live model study or
optimized installation is claimed for this runner revision; historical diagnostic
reports retain the fields originally emitted. This improves evaluation admission
and disclosure, not held-out independence or reasoning/expert readiness.

### Benchmark run deadline and cancellation verification

`benchmark_until` and CLI `bench --max-run-seconds` now apply bounded run duration
and explicit cancellation on top of original task limits. Reports retain completed
prefixes and the cancelled active task, preserve call reservations/unknown token
usage, and stop later admission. Ready cancellation creates no task. Dataset
count stays `total`; `executed`/`unexecuted`, `complete`, run stop reason and
configured duration make partial coverage explicit. Partial accuracy excludes
unexecuted cases and is null for no attempts. CLI interruption prints valid JSON
and exits unsuccessfully. Synchronous preflight and approved cleanup grace can
extend observed command time; external provider/GPU termination is not inferred.

All 432 tests (387 library, 45 integration), formatting, workspace all-target check
and Clippy pass. Log: `/tmp/cogfabric-benchmark-cancellation-tests.log`. Virtual-time
and scripted cancellation tests retain a successful prefix, cancel the next task
and never admit the last one. Actual CLI/stalled-provider tests cover deadline,
SIGINT, JSON/nonzero exit and durable cancelled task status. Successful CLI
arithmetic with an explicit five-second ceiling also passed: evidence
`/tmp/cogfabric-bench-deadline-smoke.mOASVP`. No real model study or optimized
source-install refresh was run for this cancellation revision. Independent
reasoning/expert evaluation and broader lifecycle requirements remain open.

### Benchmark model/configuration disclosure verification

Basic reports now record validated declared configuration, its serde_json
checksum, package version and compiled local-role prompt identity. Bench and
compare share a bounded SQLite identity projection for backend labels, expert
bindings and primary registration snapshots. Reasoning text is not copied into
Rust for identity extraction. Limits cover source row length, projected labels,
snapshots, event count and aggregate projected bytes; malformed/non-object or
duplicate primary metadata fails closed. Declared configuration and compiled
prompt identity do not attest replacement-backend behavior or neural weights.

All 434 tests (389 library, 45 integration), formatting, workspace all-target
check and Clippy pass. Log: `/tmp/cogfabric-benchmark-identities-tests.log`.
Regressions cover large excluded reasoning, malformed/oversized identity data,
count/aggregate ceilings, configuration hashes/home omission, requested-versus-
reported labels, tool-only empty labels and invalid configuration before ready
cancellation. Actual CLI arithmetic metadata smoke passed with no model labels:
`/tmp/cogfabric-bench-identities-smoke.gX4eGx`. Scoring, permissions and historical
reports remain unchanged. No new live study or optimized install is claimed.

### Trusted replaceable Jev+ router verification

Expert selection now has an operator-trusted Rust interface with cached declared
semantic identity. The default calls the existing tag algorithm; only two known
enabled IDs from the original task snapshot are accepted. Policy audit precedes
callbacks, transactions/mutexes are released first, old Engine clones retain
handles and excluded comparison profiles never invoke the callback. Model/tool
budgets, kernel authorization and checker/workflow admission remain unchanged.
Native callbacks are not sandboxed or preemptible, and implementation identity
is not attested. General provisioning, policy catalogs, independent scheduler
replacement and calibrated utility remain incomplete. See ROUTING.md.

After interruption, previous temporary compiler/log artifacts were absent. The
pinned compiler was rebuilt from reviewed FlatBuffers commit
`7e163021e59cca4f8e1e35a7c828b5c6b7915953`, with no host package changes. Fresh
verification used rustc/cargo 1.97.1, flatc/crate 25.12.19 and
`/tmp/cogfabric-router-target-20261013`, reusing cached Cargo dependencies.
Formatting, workspace all-target check, all 438 tests (393 library, 45 integration)
and Clippy with warnings denied pass. Current log:
`/tmp/cogfabric-router-verification.log`; compiler bootstrap log:
`/tmp/cogfabric-router-flatc-build.log`. Four new router tests cover bounds,
lock-free re-entry, snapshot disable, clone/replacement isolation, audit failure
and ablation exclusions. Actual CLI arithmetic/policy audit smoke also passed:
`/tmp/cogfabric-router-smoke.NVCRKY`. No new live-model or optimized-install
claim follows. Historical /tmp evidence remains nonportable, not recreated proof.

### Skill inventory bounds and shared capacity verification

Skill row projections now cap count and SQL field bytes before owned Rust copies,
strictly parse tags and validate identity, description, enable state and versions.
New legacy writes use those rules, with nonempty unique tags; valid pre-existing
empty tags remain readable. Immediate transactions share retained-ID admission
between legacy and manifest paths, including concurrent connections. Existing-ID
idempotency and version registration still work at the ID ceiling. Invalid rows
and over-cap databases fail without truncating, repairing or deleting records.
Health validates retained inventory and graph/model/document metadata in one
read transaction, exposing `retained_skill_ids` as an informational count.

All 442 tests (397 library, 45 integration), formatting, workspace all-target check
and Clippy pass. Log: `/tmp/cogfabric-skill-inventory-verification.log`. Four new
regressions cover corrupt/oversized records, legacy empty tags, capacity boundaries
and rollback, competing connections and denial before routing/model dispatch.
Corrected positional-ID CLI registration/duplicate-tag smoke passed at
`/tmp/cogfabric-skill-inventory-smoke.sTMHAw`; an earlier `--id` spelling failed
Clap parsing and was not counted as validation. CLI health/count smoke passed at
`/tmp/cogfabric-skill-health-smoke.9Nzbzh`. No model load/inference or optimized
installation was tested for this storage revision. These checks bound metadata,
not authenticity against an owner who can rewrite storage or neural quality.

### Manifest read bounds and active identity verification

Active graph, activation and idempotent-registration queries now apply SQL byte
guards before Rust copies of bodies, checksums and identity fields. Bounded rows
are verified incrementally, with active-capacity and duplicate-identity denial.
Existing-version registration verifies its stored body before returning known
registration success; a retained checksum alone no longer hides corruption.
Activation rejects invalid explicit IDs/versions before querying. Errors preserve
the corrupted records and deny implicit promotion or repair. Hashes still provide
local consistency rather than owner-resistant authenticity or code signatures.

All 445 tests (400 library, 45 integration), formatting, workspace all-target
check and Clippy pass. Log: `/tmp/cogfabric-manifest-bounds-verification.log`.
Three regressions cover stored-body corruption, field bounds across read paths
and duplicate/excessive active sets, including a deliberately missing uniqueness
index. Audit counts and active flags remain unchanged on failure. Valid actual
CLI register/idempotent/activate/health smoke passed:
`/tmp/cogfabric-manifest-bounds-smoke.AWGFjP`. No model inference/load, new tool
permissions or optimized installation is claimed for this storage revision.

### Active routing projection consistency

Enabled active manifests now require their bounded routing description and parsed
tag projection to match the verified immutable body before graph acceptance.
Valid-but-different projection fields therefore fail task snapshots and health,
without rewriting data or issuing new success audits. This is a consistency
check, not authenticity against an owner who can rewrite both body and hash.
Reviewed version activation still updates the projection atomically, while
previous task snapshots retain their original fields/versions.

All 447 tests (402 library, 45 integration), formatting, workspace all-target
check and Clippy pass. Log: `/tmp/cogfabric-manifest-projection-tests.log`. Two
new regressions cover projection drift and legitimate version replacement with
retained snapshots. Valid actual CLI register/activate/health also passed:
`/tmp/cogfabric-projection-smoke.wPHJe9`. No model load or live quality measurement
was performed. Broader independent evaluation and module lifecycle remain open.

### Explicit damaged-expert containment

`skill-disable ID` now validates the remaining active graph with only the explicit
target excluded, then atomically changes its enable flag and records the audit.
A corrupted target body or drifting projection can therefore be contained
without treating it as valid. Reverse dependencies and unrelated corrupt active
records still block disable; there is no cascade, forced bypass or automatic
promotion. Snapshots/health/activation retain full validation for enabled experts.
Corrupted bodies/hashes remain untouched. An operator may subsequently activate
a separately verified older version under the normal lifecycle checks. Invalid
retained skill fields may still require reviewed repair/restoration even after
disable; this does not silently normalize bad inventory data.

All 450 tests (405 library, 45 integration), formatting, workspace all-target
check and Clippy pass. Log: `/tmp/cogfabric-expert-containment-tests.log`. Three
regressions verify containment plus reviewed rollback, reverse-dependency/other-
corruption refusal and atomic audit-failure rollback. Actual CLI containment
with an intentionally damaged temporary expert passed:
`/tmp/cogfabric-containment-smoke.7K1MQu`. The initial check used a misspelled
compiler path; verification was rerun with the existing pinned 25.12.19 compiler.
No model inference/load, executable permission or optimized install is claimed.


### Explicit dependent-set containment validation

`skill-disable-set ID...` contains only the operator-selected set, bounded to
256 distinct canonical IDs. Remaining graph verification precedes atomic flag
updates and per-ID disable audits. Three new regressions cover a corrupted
root/dependent chain with an unselected independent survivor, incomplete-set
refusal, empty/duplicate/invalid/oversized/unknown/already-disabled requests,
and rollback after a later audit failure. No inferred cascade or body repair.

All 453 tests (408 library, 45 integration) pass. `cargo fmt --all -- --check`,
`cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets` and
`cargo clippy --offline --workspace --all-targets -- -D warnings` pass with
flatc 25.12.19 paired to the pinned Rust crate. Test log:
`/tmp/cogfabric-disable-set-tests.log`. Actual temporary CLI corruption,
incomplete selection rejection, complete set disable and empty-active health
passed in `/tmp/cogfabric-disable-set-smoke.q17scmqs`. Initial smoke setup used
unsupported backend `mock` then searched `.db`; corrected to `demo` and
`fabric.sqlite3`. No inference, model load, schema migration, protocol change,
optimized install or fresh clean-OS validation is claimed. Independent reasoning
quality, general executable experts/isolation and model residency remain open.


### Independent shortest-path checker validation

The bounded `shortest_path/v1` criterion verifies supplied directed unweighted
graphs, including alternative optima and unreachable goals. Six new tests cover
all 64 three-node directed graphs and every endpoint pair against simple-path
enumeration, malformed/oversized criteria and answers, direction/cycles/nonoptimal
candidates, task/actor/agent denials, registered-checker revision and tool-budget
failure, and comparison scoring without expected-answer leakage. The existing
recovery accounting allowlist includes `path.verify`; saved-result checking uses
the same criterion parser and original reservations. No executable plugin,
external fact source or runtime filesystem/network permission is added.

All 459 tests (414 library, 45 integration), formatting, workspace all-target
check and Clippy pass with pinned flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Test log: `/tmp/cogfabric-shortest-path-tests.log`. Actual CLI plus temporary
loopback OpenAI HTTP fixture passes in `/tmp/cogfabric-path-smoke.h_5fzot5`,
returning a completed result with exact checked answer and criterion evidence.
This deterministic fixture does not establish Qwen effectiveness, held-out
reasoning gains, external graph truth, weighted routing or neural safety. No
optimized install/clean-OS refresh or real-model inference is claimed here.
General independent executable verification, expert provisioning/isolation,
model residency and broader release evaluation remain incomplete.


### Frozen graph diagnostic corpus validation

Added 16 original public shortest-path criteria, separately hashed manifest/oracle
labels and independent exhaustive simple-path enumeration in
`tests/shortest_path_corpus.rs`. Comparison regressions prepare all rows, accept
independently checked optima, and reject forged verification objects and rejected
runtime outcomes. Corpus includes four unreachable cases, two identity paths and
multiple alternative optima. Public examples are not independent human-reviewed,
held-out or contamination-assessed data. No model result is claimed.

All 461 tests (415 library, 46 integration) pass with formatting, workspace
all-target check and Clippy. Commands: `cargo fmt --all -- --check`,
`cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Pinned flatc/Rust FlatBuffers 25.12.19 pairing is unchanged. Log:
`/tmp/cogfabric-path-corpus-tests.log`. Read-only query of the configured local
LM Studio model list failed with connection refusal (curl exit 7); Qwen graph
comparison remains unrun. No server launch, weight download/load/unload, new tool
permission, runtime schema/protocol change or optimized-install validation.
Independent unseen quality/security evaluation and broader lifecycle gates stay
open. BENCHMARKING.md records provenance and the bounded follow-up procedure.


### Revision-chain admission verification

Jev+ now admits critique only if the complete critic/reflector/revised-solver
sequence fits remaining cycles, calls, output reservations and bounded prompt
bytes. Per-call durable charges, policy checks, wall time and cancellation remain
authoritative. Admission is not a precharge, time guarantee or measured utility
estimate. An unavailable chain preserves the unverified candidate and records
`skip_revision` / `insufficient_revision_budget`. Independent checker revision
is unchanged. No permission, provider call ceiling or recovery allowance grows.

All 462 tests (416 library, 46 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust pairing 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-revision-admission-tests.log`. Focused tests initially found
three old expectations of critic calls without a feasible follow-up. Tests now
assert avoided dispatch; the valid critique/revision test has explicit sufficient
call/output budgets and still exercises real critique/reflection. A new regression
covers exhausted cycles, output reservations and prompt bytes. Existing call-limit
regression now proves two calls saved without promoting the candidate.

Actual CLI/HTTP fixture `/tmp/cogfabric-revision-admission-smoke.qmquuoj1`
confirmed only planner+solver dispatch with a one-cycle ceiling, preserved answer,
unverified status and the new stop reason. Initial smoke setup wrongly assumed
`init` saves config; corrected by explicitly supplying a temporary configuration.
No live-model quality, energy/speed improvement, clean-OS install or calibrated
expected utility is claimed. General scheduling, independent reasoning evaluation,
expert provisioning/isolation and physical model residency remain incomplete.


### Current optimized host installation refresh

On 2026-10-13, exact source `840e63d55fa7af3b99a24d683104064e4a9f2c24`
installed both packages into `/tmp/cogfabric-current-install.rTuVwk` using
Rust/Cargo 1.97.1 and flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo install --offline --locked --path . --root PREFIX` and
`cargo install --offline --locked --path crates/cogfabric-server --root PREFIX`.
Core optimized build: 2m17s; server: 56.30s. This is the current host with cached
Cargo dependencies, not a fresh OS/dependency-download validation.

Both `scripts/installed-smoke.sh PREFIX/bin/cogfabric` and
`scripts/installed-http-smoke.sh PREFIX/bin/cogfabric PREFIX/bin/cogfabric-server`
passed. Evidence directories: `/tmp/cogfabric-installed-smoke.PRz9GB` and
`/tmp/cogfabric-installed-http.dWe7xP`; installation/toolchain/source/hash logs
remain under the installation prefix. Core checks cover persistence, RAG candidate
promotion/rollback, typed sockets, demo inference, same-image tool/inference
workers, denials, recovery and cleanup. HTTP checks cover authentication, Origin
denial, discovery, typed arithmetic/SQL workers with confirmed reaping and graceful
shutdown, deliberately using an unavailable upstream. Installed disable-set help
also parses. No live-model quality or GPU inference is established by these runs.

Binary SHA-256:
- CLI: `e16e12e89725a820f93518e31037e0c0fc14c1e0a49972eff4d73482be571ef2`
- Server: `61e40bc77fcc1978f9e3fdf799af7449178462c88544b33021c7ed38f69d5594`

Script SHA-256:
- CLI smoke: `afa1fba2e74971edf335ae7f485d10e0d2fe09566039d5d7dcccd679242073cd`
- HTTP smoke: `950ff16da845b970466e5af0b335dde9733e457760b4b81a65cc4dfc81976723`

Hosted CI run `38420337608` also completed successfully for this exact source:
https://github.com/DrapNard/cognitive-fabric/actions/runs/38420337608
The earlier clean Arch evidence remains scoped to its recorded older source;
this refresh does not replace it with a claim of current clean-OS validation.
Source verification already passed all 462 tests, formatting, workspace check
and Clippy before these installs; documentation-only evidence recording does not
require repeating those checks. Full V1 remains incomplete: general scheduling,
independently provisioned executable experts/isolation, physical model residency
and independent unseen reasoning/robustness evaluation are still open.


### Trusted recovery-pinned subtask scheduler validation

`Engine::with_subtask_scheduler` installs a cached versioned trusted Rust policy.
Validated full dependency orders cannot change graph definitions or allowances.
Intent audit precedes callback, and a graph-hash-bound decision precedes subtask
state changes. Recovery validates bounded, unique, correctly ordered records and
sequential execution; supported continuation reads the selected order without
reinvoking policy. Legacy unmarked graphs retain stable order. Benchmark reports
disclose the scheduler and no implementation attestation. See SCHEDULING.md.

All 466 tests (420 library, 46 integration) pass; formatting, workspace check and
Clippy pass with flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-scheduler-tests.log`. Four new tests cover custom reverse
independent-step execution, engine clone preservation, persisted order reads,
eight tampering/missing/duplicate variants, invalid selections/descriptors,
invalid-order dispatch denial and audit failure before callback. Existing actual
HTTP recovery regressions pass for default/legacy ordering. Custom reversed-order
execution/reconstruction is tested; a separate live HTTP resume of that custom
order is not claimed. CLI default-order HTTP/graph audit passed in
`/tmp/cogfabric-scheduler-smoke.ot9nq_t_` with four expected calls.

Initial compilation found a temporary mutex-guard type mismatch, corrected before
verification. The first unrestricted-in-scope library command ran under restricted
socket permissions and failed IPC fixtures; the permission-enabled run identified
one legacy test-copy filter omitting the new required records. The fixture now
copies the full graph scheduling history. Clippy required moving the recovery
helper before its test module; that source-order-only fix passed formatting,
check and Clippy after the passing complete test run.

Native policies remain trusted/nonpreemptible metadata callbacks, not untrusted
plugins or executable attestation. Graph hashes do not protect against an owner
rewriting all data. No new tool permission, budget, remote policy loader, physical
GPU residency or parallel/general module scheduling is supplied. The prior
optimized-install evidence is scoped to `840e63d`; no current optimized refresh
or independent model-quality improvement is claimed. Full release gates remain
open for broader scheduling, provisioning/isolation, residency and evaluation.


### Custom scheduler public HTTP continuation proof

Two added regressions extend scheduler proof from execution/reconstruction to
public approved continuation. A separately saved independent graph ordered
`[1,0]` with node 1 complete resumes only node 0 and the final solver over actual
loopback HTTP. Assertions preserve one execution-settings record, one schedule
intent/decision, four total charged calls, 1,024 reserved outputs, and original
remaining allowances (four calls / 1,976 output tokens). Final sorting is checked
independently. A companion regression changes completed prefix order, removes
the required schedule or changes descriptor version; preview and approved resume
fail before network dispatch, preserving all task/event state and charges.
This is deterministic HTTP integration, not Qwen quality or arbitrary recovery.

All 468 tests (422 library, 46 integration), formatting, workspace all-target
check and Clippy pass with the existing FlatBuffers 25.12.19 pairing. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-custom-resume-tests.log`; focused HTTP proof log:
`/tmp/cogfabric-custom-resume-focused.log`. Initial Clippy flagged a manual test
loop counter; replaced with enumeration and reran all gates, including strengthened
budget assertions. Production behavior is unchanged in this test/documentation
commit. No live provider inference, optimized-install refresh or general recovery
claim. SCHEDULING.md and IMPLEMENTATION_MATRIX.md record the expanded proof;
broader scheduling, untrusted expert isolation/provisioning, model residency and
independent unseen model studies remain open V1 requirements.


### Real Qwen graph diagnostics: bounded partial deep comparison

On 2026-10-13, source `22dbc5acbfb55df7c9d8f55c103fa0af28cfcd21` ran
`qwen/qwen3-8b` through LM Studio on the frozen 16-case public graph corpus.
The server was initially off and no model was loaded. Its CLI started the server
on `127.0.0.1:1234` without changing authentication; the existing configuration
allowed access. Default GPU loading failed (cause not established). With 6,905 MiB
of whole-device memory already occupied out of 12,288 MiB, a 35% GPU-offload load
succeeded in 4.05 seconds as reported by the CLI. Context was 8,192 tokens,
parallelism one, idle TTL 600 seconds. Other workloads were not stopped. This is
operator CLI setup, not proof of framework-managed residency ownership, native
REST lifecycle switching, immutable weights or GPU attribution. The loopback
LM Studio server remains started; the loaded model has the configured idle TTL.

A fresh isolated state registered `evaluation/shortest_path_workflow.json` as
`graph.path.s1` version 1.0.0. It is an advisory breadth-first-search checklist,
not an executable or verification grant. All six arms used the same Qwen backend
and original per-task ceilings: eight calls, three cycles, 3,000 reserved outputs,
eight tools, 128,000 input bytes, 90 seconds wall time, 60-second inference timeout,
temperature 0.15, reasoning effort none, interpretation/synthesis disabled. Profiles
may further narrow these common operator ceilings. The memory arm started without document evidence. No larger or 1–3B baseline ran.

The 1,200-second run cap produced **56/96 task outcomes**, with **40 unexecuted**,
`complete: false`, `stop_reason: run_deadline`, and one cancelled memory task.
CLI exit 1 explicitly reported the incomplete comparison. Raw evidence is
`evaluation/results/qwen3-8b-shortest-path-deep-partial.json`; its separately
hashed summary records load context, workflow/binary hashes and limitations.
Temporary execution state/logs remain at `/tmp/cogfabric-qwen-path-study.0f37ok`.
The initial real FAST probe independently verified `["a","c"]` in one call;
its temporary evidence is `/tmp/cogfabric-qwen-path-probe.GcHvdD`.

| Profile | Passed / executed | Passed / 9 common cases | Charged model calls | Total task seconds |
|---|---:|---:|---:|---:|
| Alone | 6 / 9 | 6 / 9 | 9 | 30.14 |
| Tools | 6 / 9 | 6 / 9 | 14 | 48.11 |
| Recursive | 7 / 9 | 7 / 9 | 11 | 32.34 |
| Collective | 6 / 10 | 6 / 9 | 40 | 293.72 |
| Memory | 6 / 10 | 6 / 9 | 38 | 272.09 |
| Experts | 2 / 9 | 2 / 9 | 13 | 523.56 |

There were 125 charged model calls and 73 charged tool invocations. These are
runtime reservations/dispatches, not proof of completed GPU work. Expert failures
were six inference timeouts and one HTTP transport failure. Missing reported token
usage stays null for affected arms. Backend labels do not attest weights; external
GPU activity and partial offload prevent comparing latency against earlier
full-GPU runs. The report's old general limitation strings predate explicit graph
wording; shortest-path scoring also shares the compiled runtime checker. The
separate corpus regression independently enumerates simple paths and rechecks
every frozen outcome's pass bit, identities, totals and reference hashes.

**No expert or collective benefit is demonstrated.** Recursive corrected one
additional case in this single nine-case matched subset. This is incomplete,
public, non-human-reviewed data with no replicate/seed-controlled study; it is not
causal evidence, held-out robustness, general reasoning ability or neural safety.
Direction-trap answers using reversed undeclared edges were independently rejected.
Corpus labels/scoring and budgets were not changed to improve results. The corpus
manifest now records partial live evaluation while retaining incomplete/independent
study flags. Framework residency, broader scheduling/expert isolation and unseen
independent comparisons remain release work.

To reproduce the procedure, configure a fresh state with the report's declared
settings, register/activate the committed workflow, then run:

```bash
cogfabric --home /path/to/fresh-state --json compare evaluation/shortest_path_cases.jsonl \
  --mode deep --source 'Public synthetic shortest-path diagnostics with advisory BFS workflow' \
  --license CC0-1.0 --code-revision "$(git rev-parse HEAD)" \
  --dataset-sha256 0fedaaf6bef27f45ae0a13debcd5c6e391a32637c84891b0f38a19d9cba93edc \
  --workflow-check --max-run-seconds 1200
```

Record the actual tested source and load configuration for each new run. This
reproduces the controlled procedure, not identical stochastic model responses.


### Optional advice timeout containment validation

All 470 tests (423 library, 47 integration), formatting, workspace all-target
check and Clippy pass with existing FlatBuffers 25.12.19 pairing. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-advice-timeout-tests.log`. A paused-clock regression covers
shared optional advice timeout followed by independently checked solving,
preserved call/output charges and unavailable usage; failure-audit denial and
non-timeout shared errors both prevent further calls. Existing bound-model,
structured-workflow, cancellation, recovery and managed-worker regressions pass.

Actual CLI/HTTP fixture `/tmp/cogfabric-advice-timeout-smoke.4fk1xuvh` stalled the
first optional call under a six-second task allowance, then served planner and
solver. The checked path completed with three calls and 832 reserved output
tokens; input/output usage stayed null and timeout fallback was durably audited.
The provider-side fixture remained independently controlled, so cancellation does
not prove upstream computation stopped. No new live Qwen score, optimized install,
calibrated expected utility or general provider recovery guarantee is claimed.
This addresses a measured failure mode while preserving the adverse historical
Qwen study. General scheduling, untrusted expert isolation/provisioning, physical
model residency and independent unseen model evaluation remain V1 blockers.


### Structured partial-advice deadline regression gate

All 471 tests (424 library, 47 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-partial-advice-tests.log`. The new paused-clock regression
proves one shared structured-workflow deadline, discarded partial advice, no
replay or refund, preserved 1,024 output reservations and independently checked
main solving. Actual CLI/HTTP fixture
`/tmp/cogfabric-partial-advice-smoke._4rwx3oj` confirms dependency advice is not
forwarded into main reasoning after the second step stalls. No production logic,
permissions, acceptance thresholds, original Qwen results or schema changes in
this test/documentation commit. Provider GPU stopping, general backend recovery,
independent reasoning quality, untrusted expert provisioning/isolation and
ownership-aware residency remain unproven release requirements.


### Bound-advice audit and integrity failure gate

All 472 tests (425 library, 47 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-bound-advice-tests.log`. New negative coverage injects bound
advice audit failures at expert dispatch, solver authorization, model reservation
and structured step logging; the test confirms model snapshot admission occurred
and primary inference never runs. Its policy trigger is scoped to the advice role
to avoid passing via an unrelated earlier retrieval failure. One prior regression
expected broad provider-error fallback; it now asserts fatal unclassified failure,
no primary calls, retained 192-token reservation and unavailable usage.

Actual CLI/HTTP `/tmp/cogfabric-bound-audit-smoke.qwjf3syq` returns bound advice,
then rejects its result audit: one charged domain call, no primary HTTP call and
no fake availability-fallback event. Positive fixture
`/tmp/cogfabric-bound-timeout-smoke.q9l2f6gx` proves typed timeout still gives
exactly domain/primary/primary requests, a checked final path, three calls and
832 output reservations with unknown usage. This is a deliberate fail-closed
compatibility tightening, not improved Qwen quality. An initial edit helper stopped
before writing; the corrected scoped regression and all gates were rerun. No new
model study, optimized install, protocol schema or release claim. General module
scheduling, isolated expert provisioning, physical residency and independent
unseen evaluations remain incomplete.


### Durable reservation and notification failure gate

All 474 tests (427 library, 47 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-reservation-order-tests.log`. Two new regressions first record
a successful model call, then verify: failed DB reservation preserves every prior
session metric/charge; post-commit observer poison retains the second reservation,
no second backend execution and unknown durable audit completion. Bound-advice
model-dispatch insertion denial now correctly reports zero uncommitted charges.

CLI/HTTP reservation-denial fixture
`/tmp/cogfabric-reservation-order-smoke.pha4yuev` passed with zero calls, output
reservations and prompt bytes, no model-dispatch event and no HTTP request. Its
initial smoke expectation of known zero usage was corrected to the existing
conservative terminal-failure null policy. The later committed-notification
classification is covered by the full passing unit gate; no claim of live CLI
observer poisoning or provider GPU completion is made. No new model study,
optimized installation, permissions, acceptance thresholds or protocol schemas.
General scheduling, independent expert provisioning/isolation, physical residency
and independent unseen quality evaluation remain release work.


### Strict agent JSON boundary validation

All 479 tests (431 library, 48 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-strict-agent-json-tests.log`. Five new regressions cover
valid numeric/scalar preservation, duplicate decoded/nested/array keys, bounds/
recursion/trailing data, proposal and correlated-history rejection, real provider
HTTP ambiguity with a valid control, and real server HTTP rejection before native
inference. Existing native FlatBuffers, managed workers, disconnects and client
workflow checks pass. No live model study, external tool execution, optimized
installation, general JSON Schema validator or arbitrary client compatibility
claim. General scheduling, isolated expert provisioning, physical residency and
independent unseen reasoning/security evaluations remain release work.


### Remaining provider JSON boundary gate

All 482 tests (434 library, 48 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-provider-json-tests.log`. Three new regressions cover actual
reasoning-provider HTTP duplicate answer/model/usage fields, fenced advisory
nested/escaped duplicates with valid controls, and actual native inventory HTTP
ambiguity while preserving valid unknown metadata. Existing CLI residency tests
now also inject duplicate initial inventory and duplicate post-load reply. The
first produces no POST; the second preserves interrupted/unknown outcome and
neither causes another network request. These are isolated fixtures, not real
model lifecycle mutations. The first expanded scenario assertion selected the
wrong test branch; corrected and reran all gates without changing acceptance
criteria. No live model benchmark, optimized install, new permissions or protocol
schema. General module scheduling, isolated expert provisioning, physical model
residency and independent unseen evaluations remain incomplete.


### Versioned capability discovery gate

All 487 tests (439 library, 48 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-capability-discovery-tests.log`. Five new regressions cover
legacy checksum preservation (pre-change CLI captured checksum
`e43b6ec09af5f9be056f0f6b27c971439b42a0857eaa0bee812d107c874ecc91`), invalid/
duplicate/excessive declarations, semantic lookup, disabled/current/pinned versions,
legacy-placeholder precedence, gap inspection without grants/candidates, actual
engine alias routing while activation changes during interpretation, and a large
catalog that preserves the solver by skipping oversized optional interpretation.
Sensitive alias metadata remains denied by the kernel.

Actual CLI registration/activation, compatible/incompatible lookup, explicit
metadata disclaimers and disabled exclusion pass in
`/tmp/cogfabric-capability-lookup-smoke.ce2skzeh`. No model inference or lifecycle
mutation is needed for lookup. Existing serialized manifests keep canonical hashes;
Rust struct-literal callers have an additive source-field requirement documented
in EXPERTS.md. No new live quality study, optimized install, executable plugin,
protocol schema or database migration. General scheduling, isolated provisioning,
physical residency and independent unseen model evaluations remain release work.


### Capability binding provenance gate

All 488 tests (440 library, 48 integration), formatting, workspace all-target
check and Clippy pass with existing flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-capability-binding-tests.log`. Existing in-flight replacement
coverage now asserts the old provider/capability versions in the audit. A new
regression requests five available providers plus one missing capability: four
are selected, the fifth is available/unselected with its versions, and the missing
binding stays null/unselected with explicit gap metadata. No version or permission
is fabricated.

Actual CLI/HTTP fixture `/tmp/cogfabric-capability-binding-smoke.i1wb1vse` records
`domain.sort` -> `reviewed.sort.s1`, provider 1.0.0 / capability 2.1.0, selected
and declaration-only with no permission grant. Original sorting is independently
checked after three expected interface/planner/solver calls. No live model study,
optimized install, operational expertise claim or new authority. General module
scheduling, isolated executable provisioning, physical residency and independent
unseen reasoning/security evaluations remain incomplete.


### Self-contained pinned protocol build validation

All 490 tests (440 library, 50 integration), formatting, workspace all-target
check and Clippy pass **with FLATC unset**, using the verified bundled bindings.
Commands: `cargo fmt --all -- --check`,
`cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-bundled-protocol-tests.log`. Runtime socket, malformed-frame,
managed-worker and compatibility regressions therefore run on the fallback output.
Two new bundle regressions cover exact source/normalized runtime pins, every
schema/output checksum, stale schema/generated bytes, duplicate inventory and
wrong generator pin without repair. The explicit compiler regenerated identical
outputs; its actual source commit matches the recorded upstream pin.

Wrong and missing explicit compiler checks fail as intended, then pinned compiler
and bundled all-target checks recover successfully. Evidence:
`/tmp/cogfabric-flatc-negative.aYyN1n` (final logs and package file list).
`scripts/update-protocol-bindings.sh` generated staging evidence at
`/tmp/cogfabric-protocol-bindings.a0foYr`. A local **unpublished** core crate archive
was extracted into `/tmp/cogfabric-protocol-package.YbSeI6`; all-target check and
its two bundle tests pass with no FLATC, including Cargo's normalized manifest.
This tests the actual distributed file inventory, not an optimized fresh-OS
installation. Both workspace packages passed source checks; only the core archive
was extracted/tested. Generated sources total 375,777 bytes and are committed
specifically to remove the native compiler prerequisite while retaining paired IPC.

An edit helper initially failed to wire the normalized-manifest checker, producing
an unused-function Clippy failure. It was corrected and all gates rerun before
package verification. README documents default versus developer regeneration modes;
CI now checks both. No schema migration, model weights, new runtime permissions,
public crate publication, release tag, live quality study or current clean-OS
claim. General scheduling, isolated expert provisioning, physical residency and
independent unseen evaluations remain incomplete.


### Optimized CLI and server installation without flatc — 2026-10-13

Exact runtime source `84e097cec37c71a09d71b75a5eee6bb841376979` installed both
optimized packages with `FLATC` unset into
`/tmp/cogfabric-bundled-install.nVwT2q`. Reported Rust/Cargo: 1.97.1. Commands:
`cargo install --offline --locked --path . --root PREFIX` and
`cargo install --offline --locked --path crates/cogfabric-server --root PREFIX`.
Core release build took 1m30s and server build 1m02s as reported by Cargo. This
uses the current host's C toolchain/linker and cached dependencies; it is not a
fresh OS or dependency-download study. No native FlatBuffers compiler was selected.

Both complete installed workflows passed:
- `scripts/installed-smoke.sh PREFIX/bin/cogfabric`: local persistence, reviewed
  example RAG promotion/rollback, typed sockets, demo inference, managed workers,
  policy denials, recovery and cleanup. Evidence:
  `/tmp/cogfabric-installed-smoke.RRHCKp`.
- `scripts/installed-http-smoke.sh PREFIX/bin/cogfabric PREFIX/bin/cogfabric-server`:
  authenticated discovery, Origin denial, arithmetic/SQL worker verification and
  reaping, and graceful shutdown against deliberately unavailable upstream
  inference. Evidence: `/tmp/cogfabric-installed-http.20mTuz`.

Binary SHA-256:
- CLI: `cd8a29fe6a37a71e6fb0339630d9c55665140ebe2c368df72dbf8704941d51af`
- Server: `6e4d5455e3a64a8eef9ad3cdd4c4515aa4fa3efa811670662b83ade45c904bc0`

Bundled generation manifest SHA-256:
`069657844f4c458c72858b69261e7097f2f6f59db8098f10bceba094c6391ba7`.
Source/toolchain/install/smoke/hash logs remain under the installation prefix.
The corresponding hosted CI run
[38454460668](https://github.com/DrapNard/cognitive-fabric/actions/runs/38454460668)
succeeded, including exact pinned compiler regeneration, all mandatory gates,
bundled all-target checking, local/HTTP workflows, security boundary probes and
offline training preflight. Source tests remain 490 passing (440 library,
50 integration); no runtime code changed in this evidence/documentation update.

No live-model coding quality, current clean-OS/GPU setup, provider ownership or
neural safety is established by installation success. The earlier clean Arch
results stay scoped to their recorded source. General scheduling, independently
provisioned isolated experts, physical residency and independent unseen model
studies remain open V1 requirements. No public crate/repository publication or
release tag was performed. HTTP_API.md now describes ordinary bundled builds
rather than requiring an obsolete compiler export.


### Agent control shape parity gate

All 492 tests (441 library, 51 integration), formatting, workspace all-target
check and Clippy pass with FLATC unset and the verified 25.12.19 bundle. Commands:
`cargo fmt --all -- --check`, `cargo check --offline --workspace --all-targets`,
`cargo test --offline --workspace --all-targets`, and
`cargo clippy --offline --workspace --all-targets -- -D warnings`.
Log: `/tmp/cogfabric-agent-control-shapes-tests.log`. A new core regression checks
supported string/named choices and rejects extra/nonnative controls. Actual server
HTTP coverage rejects unknown named-choice fields and text-part metadata before
backend invocation, then accepts a valid named choice as its positive control.
Existing native managed transports, proposals, disconnects and cleanup regressions
pass. This intentionally narrows previously silently ignored input; it does not
claim full JSON Schema enforcement or current live OpenCode/model compatibility.
No new host action, authority, optimized installation or independent quality study.
General scheduling, isolated executable provisioning, physical residency and
independent unseen evaluations remain open V1 work.

### Current optimized installation refresh

Source `fc839768a0dc2105dbdd1c971f9ba3abc2cf31ab` was installed offline with locked cached dependencies on
Rust/Cargo 1.97.1, leaving FLATC unset. Both packages built in optimized mode:
core 2m46s, HTTP server 1m04s (Cargo-reported build durations, not startup latency).
Prefix: `/tmp/cogfabric-current-install.cj8eTv`. Both `scripts/installed-smoke.sh` and
`scripts/installed-http-smoke.sh` passed against the installed executables.
CLI/socket evidence: `/tmp/cogfabric-installed-smoke.eQsFRe`;
HTTP evidence: `/tmp/cogfabric-installed-http.TrhSq2`.

Installed CLI SHA256:
`0f3b665c3b9416bb2736774bc847c9114ac0bdf4b9bd1e697286a58f052b1aa1`.
Installed server SHA256:
`02a8b456b08bafea1e05f29987af6ec6ff4c9db846603e96c59c1ae01ae817c0`.
Protocol bundle manifest SHA256:
`069657844f4c458c72858b69261e7097f2f6f59db8098f10bceba094c6391ba7`.
Build and smoke logs are retained under the prefix. Core smoke covers the existing
fresh-state installed CLI/socket workflows. HTTP smoke verifies authenticated
discovery, Origin denial, typed arithmetic/SQL workers and graceful cleanup.
The installed runtime includes concise advice and scoped subtask prompts.

This refresh uses the current host's cached dependencies and C toolchain; it is
not a new clean-OS validation. The observed LM Studio inventory had no loaded
models after their idle TTL, and these offline smokes perform no live inference.
The earlier real graph measurements remain source-specific. Independent quality,
physical residency and general expert lifecycle gates remain incomplete.

### Current installed live socket inference

The optimized CLI from source `fc839768a0dc2105dbdd1c971f9ba3abc2cf31ab`
was run through its real Unix-socket service with Qwen3 8B in fresh state.
FAST `sort:{"values":[3,1,2,1]}` returned `[1,1,2,3]`, independently checked,
with one call in 3.00 seconds. DEEP on the three-node directed graph with edges
`a->b`, `b->c`, `a->c` returned the minimum path `["a","c"]`, independently
checked, with six calls in 26.67 seconds under the original 30-second budget.
The latter audit contains one plan, one committed schedule and four subtask
resource records, so installed scoped intermediate calls actually ran.
Both clients exited zero. The owned service stopped and its socket was removed.

Raw outcomes, audit counts, installed source/binary identity and limitations are
retained in `evaluation/results/qwen3-8b-current-installed-socket-smoke.json`.
Temporary state/logs: `/tmp/cogfabric-installed-live-socket.p9vx_m4i`.
No model was initially loaded; the existing Qwen weights were loaded via LM Studio
CLI with 8192 context, 0.35 requested offload, one parallel prediction and 600s
idle TTL (3.40 seconds reported load). Other models/workloads were not unloaded.
This is two installed smoke tasks, not a held-out reasoning/latency study or proof
of native framework residency ownership. The prior 96-task graph report remains
source-specific. Broader independent quality and model/expert lifecycle stay open.

### Real-provider phase handoff and optimized refresh

Source `18de498` completed a public three-node path task in RESEARCH mode using
real Qwen3 8B: verified `["a","c"]`, three charged calls, 14.48 seconds within
an unchanged 30-second task ceiling. The planner hit its audited 9999 ms phase
ceiling, emitted typed planning_failure with partial strategy discarded, then
exploration ran under a 6665 ms ceiling and the original solver completed.
Reported input/output usage remained unknown after interruption. The distinct
endpoints and direct a->c edge independently establish the minimum one-edge path.
Raw outcome, configuration, dispatch/phase audits, binary hash and limitations:
`evaluation/results/qwen3-8b-research-phase-limits-smoke.json`.
Temporary evidence: `/tmp/cogfabric-live-phase-limits.nhyn3roq`.
This is one debug-CLI smoke, not a quality or causal latency study; provider stop,
immutable weights and framework-native residency ownership remain unproven.

The same source was then installed offline with locked cached dependencies and
FLATC unset on Rust/Cargo 1.97.1. Both optimized packages and installed workflow
scripts passed. Prefix/logs: `/tmp/cogfabric-phase-install.Z1M9bI`.
Cargo reported core build 1m31s and HTTP build 1m04s. CLI/socket evidence:
`/tmp/cogfabric-installed-smoke.pl80HA`; authenticated HTTP discovery, Origin denial,
typed arithmetic/SQL worker and graceful cleanup evidence:
`/tmp/cogfabric-installed-http.cnkIYD`.
CLI SHA256: `6f3f7faae624d5ef36098ded440882a6019832352b5fe1dddd3d2729793e1519`.
Server SHA256: `353996f14767a7eed63c05260bdde5028b01fedb050fe1cd26dbc59500d8f76d`.
These installed smokes do not themselves invoke a model; the real-provider result
above used the debug binary. This is a current-host refresh, not new clean-OS
proof. All 500 mandatory tests/checks remain tied to the preceding runtime gate;
no runtime source changed in this evidence-only update. General phase recovery,
module/residency lifecycle and independent quality evaluation remain incomplete.

Hosted verification refresh: source 5a782a2e3a2fc8f7c0e933a9c2dfcb357c234166
passed run 38591572231, including 502 tests, native compiler/bundled binding checks,
mandatory quality gates, complete offline CLI/HTTP workflows, security evaluation
and training preflight. See QUALITY_REVIEW.md for the verified run and scope.
No release tag, public publication, GPU result or independent quality approval
follows from this hosted gate.

Complete RESEARCH public graph evidence: all 96 tasks ran at source 5a782a2,
with independently recomputed scores and diagnostic totals. Expert workflow
scored 16/16 versus alone 11/16 with 69 versus 16 calls; 24 planning timeouts
were contained and no task exhausted. See BENCHMARKING.md and raw phase report.
This does not complete independent held-out evaluation or general expert quality.
