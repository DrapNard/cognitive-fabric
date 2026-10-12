# v1.0 release acceptance audit

This checklist preserves the 18 acceptance criteria in the goal objective. It is
an evidence inventory, not a release approval. The canonical French architecture
and the broader implementation matrix remain authoritative. Earlier test counts
in the quality review are historical milestones. Current local verification is
343 Rust tests (322 library, 21 integration), formatting, all-target checking and
Clippy with warnings denied, using FlatBuffers compiler/crate 25.12.19.

| Goal criterion | Evidence inspected | Assessment / remaining proof |
|---|---|---|
| 1. Workspace builds | `Cargo.toml`, all-target check | Passed locally; isolated optimized source installation also passed with cached dependencies. |
| 2. Formatting and Clippy | Mandatory checks in `QUALITY_REVIEW.md` | Passed locally. |
| 3. Mandatory tests | All-target test results, `tests/` | 343 passed; fixtures do not establish live model quality. |
| 4. FlatBuffers IPC | `protocol/`, `src/wire.rs`, `src/fabric/`, malformed/compatibility/socket tests | Implemented for present transports; authenticated process event delivery/replay is now tested; broader module compatibility remains partial. |
| 5. CLI/service communication | `src/service.rs`, socket tests, `tests/process_worker.rs` | Tested local transport, progress and cancellation; no general independent plugin service is claimed. |
| 6. Compatible local LLM | `src/backend.rs`, `tests/inference_auth.rs`, `LIVE_INFERENCE.md` | Passed for existing Ollama OpenAI-compatible endpoint: direct generation and managed-worker sorting with independent verification. LM Studio-specific authenticated generation and real switching remain unverified. |
| 7. Jev+ routes real tasks | `src/engine.rs`, registry/routing tests, built-in SQL execution | Deterministic tools and registered advice routing work; live expert usefulness and general module scheduling remain incomplete. |
| 8. Bounded Jev-R revision | `src/reasoning.rs`, scripted revision and budget tests | Enforced calls/cycles/tools/time; real-model effectiveness unverified. |
| 9. Skill registration/discovery | `src/registry.rs`, `EXPERTS.md` | Versioned RAG/workflow/compiled-checker registrations work; arbitrary executable loading is absent. |
| 10. Persistent memory | `src/store.rs`, migrations, retrieval/reopen/rollback tests | Implemented bounded persistence and fresh structured-answer checks; general fact verification and physical disk bounds remain partial. |
| 11. Budgets/cancellation | Session/broker/worker tests, `RECOVERY.md` | Current execution ceilings tested; exact tokenizer/GPU ceilings and safe automatic interrupted execution remain incomplete. |
| 12. Security regressions | `security-eval`, policy/transport/artifact tests, `THREAT_MODEL.md` | 24 fixed boundary probes plus regression tests; live injection, poisoning, sandbox escape and general peer identity evaluations remain unmeasured. |
| 13. RAG evaluation/promotion | `src/evaluation.rs`, `src/evolution.rs`, promotion/rollback tests | Transactional reviewed promotion with integrity/regression gates; dataset independence is operator-attested, not independently established. |
| 14. Lifecycle documented | `MODELS.md`, `EXPERTS.md`, `WORKERS.md` | Metadata lifecycle documented; explicit native load/unload is fixture-tested; automatic scheduling and real host validation remain incomplete. |
| 15. Reproducible benchmarks | `src/benchmarking.rs`, `evaluation/`, `BENCHMARKING.md` | Six-arm runner and provisional corpus exist; reviewed unseen dataset and real equal-budget model study missing. |
| 16. Accurate matrix | `IMPLEMENTATION_MATRIX.md` | Partial statuses retained; historical paragraphs must be read with later explicit superseding entries. |
| 17. Verified docs/installation | `README.md`, CI workflow | Isolated source install and offline CLI walkthrough passed; clean-OS prerequisite setup and live inference instructions remain unverified. |
| 18. Honest experimental claims | `EVOLUTION.md`, `MODELS.md`, `THREAT_MODEL.md` | Neural adaptation remains experimental and neural promotion denied; no trained Jev or superiority claim. |

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
authorized and performed; a subsequent synchronization is currently blocked by
invalid GitHub authentication. No new final release tag is authorized.

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
