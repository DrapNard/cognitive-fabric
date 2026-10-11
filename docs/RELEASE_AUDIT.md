# v1.0 release acceptance audit

This checklist preserves the 18 acceptance criteria in the goal objective. It is
an evidence inventory, not a release approval. The canonical French architecture
and the broader implementation matrix remain authoritative. Earlier test counts
in the quality review are historical milestones. Current local verification is
278 Rust tests (261 library, 17 integration), formatting, all-target checking and
Clippy with warnings denied, using FlatBuffers compiler/crate 25.12.19.

| Goal criterion | Evidence inspected | Assessment / remaining proof |
|---|---|---|
| 1. Workspace builds | `Cargo.toml`, all-target check | Passed locally; isolated optimized source installation also passed with cached dependencies. |
| 2. Formatting and Clippy | Mandatory checks in `QUALITY_REVIEW.md` | Passed locally. |
| 3. Mandatory tests | All-target test results, `tests/` | 278 passed; fixtures do not establish live model quality. |
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
