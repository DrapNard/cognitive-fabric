# v1.0 release acceptance audit

This checklist preserves the 18 acceptance criteria in the goal objective. It is
an evidence inventory, not a release approval. The canonical French architecture
and the broader implementation matrix remain authoritative. Earlier test counts
in the quality review are historical milestones. Current local verification is
419 Rust tests (378 library, 41 integration), formatting, all-target checking and
Clippy with warnings denied, using FlatBuffers compiler/crate 25.12.19.

| Goal criterion | Evidence inspected | Assessment / remaining proof |
|---|---|---|
| 1. Workspace builds | `Cargo.toml`, all-target check | Passed locally; isolated optimized source installation also passed with cached dependencies. |
| 2. Formatting and Clippy | Mandatory checks in `QUALITY_REVIEW.md` | Passed locally. |
| 3. Mandatory tests | Workspace all-target test results, `tests/`, server integration tests | 419 passed; fixtures do not establish live model quality. |
| 4. FlatBuffers IPC | `protocol/`, `src/wire.rs`, `src/fabric/`, malformed/compatibility/socket tests | Implemented for present transports; authenticated process event delivery/replay is now tested; broader module compatibility remains partial. |
| 5. CLI/service communication | `src/service.rs`, socket tests, `tests/process_worker.rs`, `crates/cogfabric-server/tests/` | Tested typed local transport, progress, cancellation and optional HTTP client interoperability; native and collective ownership drains cover dropped request futures. No general independent plugin service is claimed. |
| 6. Compatible local LLM | `src/backend.rs`, `tests/inference_auth.rs`, `LIVE_INFERENCE.md` | Passed for Ollama and LM Studio Qwen3 8B compatible endpoints, including managed verified sorting. Native LM Studio read-only inventory passed; authenticated native access and controlled load/unload remain unverified. |
| 7. Jev+ routes real tasks | `src/engine.rs`, registry/routing tests, built-in SQL execution | Deterministic tools and registered advice routing work; live expert usefulness and general module scheduling remain incomplete. |
| 8. Bounded Jev-R revision | `src/reasoning.rs`, scripted revision and budget tests | Enforced calls/cycles/tools/time; real-model effectiveness unverified. |
| 9. Skill registration/discovery | `src/registry.rs`, `EXPERTS.md` | Versioned RAG/workflow/compiled-checker registrations work; arbitrary executable loading is absent. |
| 10. Persistent memory | `src/store.rs`, migrations, retrieval/reopen/rollback tests | Implemented bounded persistence and fresh structured-answer checks; general fact verification and physical disk bounds remain partial. |
| 11. Budgets/cancellation | Session/broker/worker tests, `RECOVERY.md` | Current execution ceilings tested; exact tokenizer/GPU ceilings and safe automatic interrupted execution remain incomplete. |
| 12. Security regressions | `security-eval`, policy/transport/artifact tests, `THREAT_MODEL.md` | 24 fixed boundary probes plus regression tests. Live Qwen reference injection contaminated answers; quoted-reference mitigation resisted that attack once. Broader poisoning, sandbox escape and peer identity evaluation remain incomplete. |
| 13. RAG evaluation/promotion | `src/evaluation.rs`, `src/evolution.rs`, promotion/rollback tests | Transactional reviewed promotion with integrity/regression gates; dataset independence is operator-attested, not independently established. |
| 14. Lifecycle documented | `MODELS.md`, `EXPERTS.md`, `WORKERS.md` | Metadata lifecycle documented; explicit native load/unload is fixture-tested; real native read-only inventory passed; lifecycle mutations and ownership-aware automatic scheduling remain incomplete. |
| 15. Reproducible benchmarks | `src/benchmarking.rs`, `evaluation/`, `BENCHMARKING.md` | Six-arm runner and public live workflows exist for Ollama and Qwen3 8B; reviewed unseen dataset and held-out equal-budget model study missing. |
| 16. Accurate matrix | `IMPLEMENTATION_MATRIX.md` | Partial statuses retained; historical paragraphs must be read with later explicit superseding entries. |
| 17. Verified docs/installation | `README.md`, HTTP_API.md, CI workflow, INSTALLATION_VALIDATION.md | Host source installs and offline workflows passed for core and HTTP packages; installed managed OpenCode/Qwen read/edit/read passed. Expanded clean Arch setup, all checks and installed core/server workflows passed at `a9933b3`. CachyOS desktop/GPU setup remains unverified. |
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
