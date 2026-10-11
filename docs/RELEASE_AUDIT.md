# v1.0 release acceptance audit

This checklist preserves the 18 acceptance criteria in the goal objective. It is
an evidence inventory, not a release approval. The canonical French architecture
and the broader implementation matrix remain authoritative. Earlier test counts
in the quality review are historical milestones. Current local verification is
309 Rust tests (288 library, 21 integration), formatting, all-target checking and
Clippy with warnings denied, using FlatBuffers compiler/crate 25.12.19.

| Goal criterion | Evidence inspected | Assessment / remaining proof |
|---|---|---|
| 1. Workspace builds | `Cargo.toml`, all-target check | Passed locally; isolated optimized source installation also passed with cached dependencies. |
| 2. Formatting and Clippy | Mandatory checks in `QUALITY_REVIEW.md` | Passed locally. |
| 3. Mandatory tests | All-target test results, `tests/` | 309 passed; fixtures do not establish live model quality. |
| 4. FlatBuffers IPC | `protocol/`, `src/wire.rs`, `src/fabric/`, malformed/compatibility/socket tests | Implemented for present transports; authenticated process event delivery/replay is now tested; broader module compatibility remains partial. |
| 5. CLI/service communication | `src/service.rs`, socket tests, `tests/process_worker.rs` | Tested local transport, progress and cancellation; no general independent plugin service is claimed. |
| 6. Compatible local LLM | `src/backend.rs`, `tests/inference_auth.rs`, `AUTHENTICATION.md` | Synthetic authenticated HTTP works; real LM Studio generation last returned 401. Not passed. |
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
4. Validate authenticated real generation after the operator configures a scoped
   credential locally. Establish an available small generative baseline, review
   the held-out technical corpus, then execute comparable model studies. Never
   search token files, weaken server authentication or substitute mock scores.
5. Repeat the verified source-install/offline workflow against the final implementation,
   additionally verify clean-OS prerequisites and live setup, then run the complete release gate. Preserve unavailable GPU attribution
   and energy measurements as unavailable.

Novel neural architectures and automatic neural promotion are not substitutes
for these infrastructure requirements. The goal explicitly permits optional
offline LoRA to remain experimental. No release tag or publication is authorized.

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
