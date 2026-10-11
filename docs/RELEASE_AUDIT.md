# v1.0 release acceptance audit

This checklist preserves the 18 acceptance criteria in the goal objective. It is
an evidence inventory, not a release approval. The canonical French architecture
and the broader implementation matrix remain authoritative. Earlier test counts
in the quality review are historical milestones. Current local verification is
216 Rust tests (204 library, 12 integration), formatting, all-target checking and
Clippy with warnings denied, using FlatBuffers compiler/crate 25.12.19.

| Goal criterion | Evidence inspected | Assessment / remaining proof |
|---|---|---|
| 1. Workspace builds | `Cargo.toml`, all-target check | Passed locally; clean installation still needs verification. |
| 2. Formatting and Clippy | Mandatory checks in `QUALITY_REVIEW.md` | Passed locally. |
| 3. Mandatory tests | All-target test results, `tests/` | 216 passed; fixtures do not establish live model quality. |
| 4. FlatBuffers IPC | `protocol/`, `src/wire.rs`, `src/fabric/`, malformed/compatibility/socket tests | Implemented for present transports; broader module compatibility remains partial. |
| 5. CLI/service communication | `src/service.rs`, socket tests, `tests/process_worker.rs` | Tested local transport, progress and cancellation; no general independent plugin service is claimed. |
| 6. Compatible local LLM | `src/backend.rs`, `tests/inference_auth.rs`, `AUTHENTICATION.md` | Synthetic authenticated HTTP works; real LM Studio generation last returned 401. Not passed. |
| 7. Jev+ routes real tasks | `src/engine.rs`, registry/routing tests, built-in SQL execution | Deterministic tools and registered advice routing work; live expert usefulness and general module scheduling remain incomplete. |
| 8. Bounded Jev-R revision | `src/reasoning.rs`, scripted revision and budget tests | Enforced calls/cycles/tools/time; real-model effectiveness unverified. |
| 9. Skill registration/discovery | `src/registry.rs`, `EXPERTS.md` | Versioned RAG/workflow/compiled-checker registrations work; arbitrary executable loading is absent. |
| 10. Persistent memory | `src/store.rs`, migrations, retrieval/reopen/rollback tests | Implemented bounded persistence and fresh structured-answer checks; general fact verification and physical disk bounds remain partial. |
| 11. Budgets/cancellation | Session/broker/worker tests, `RECOVERY.md` | Current execution ceilings tested; exact tokenizer/GPU ceilings and safe automatic interrupted execution remain incomplete. |
| 12. Security regressions | `security-eval`, policy/transport/artifact tests, `THREAT_MODEL.md` | 24 fixed boundary probes plus regression tests; live injection, poisoning, sandbox escape and general peer identity evaluations remain unmeasured. |
| 13. RAG evaluation/promotion | `src/evaluation.rs`, `src/evolution.rs`, promotion/rollback tests | Transactional reviewed promotion with integrity/regression gates; dataset independence is operator-attested, not independently established. |
| 14. Lifecycle documented | `MODELS.md`, `EXPERTS.md`, `WORKERS.md` | Metadata lifecycle documented; actual backend loading/unloading remains unimplemented. |
| 15. Reproducible benchmarks | `src/benchmarking.rs`, `evaluation/`, `BENCHMARKING.md` | Six-arm runner and provisional corpus exist; reviewed unseen dataset and real equal-budget model study missing. |
| 16. Accurate matrix | `IMPLEMENTATION_MATRIX.md` | Partial statuses retained; historical paragraphs must be read with later explicit superseding entries. |
| 17. Verified docs/installation | `README.md`, CI workflow | Existing checkout builds; fresh installation and full documented walkthrough not yet demonstrated. |
| 18. Honest experimental claims | `EVOLUTION.md`, `MODELS.md`, `THREAT_MODEL.md` | Neural adaptation remains experimental and neural promotion denied; no trained Jev or superiority claim. |

## Next verifiable milestones

1. Implement remaining general module lifecycle/routing with deterministic
   authorization, immutable version references and bounded queues. Existing
   in-process handles and single-task worker authentication do not prove this.
2. Continue interrupted task graphs only after validating saved context, original
   budgets, permissions, pinned dependencies and remaining wall time. Current
   `recover-verified-result` completes checked saved answers without replay;
   audit inspection alone never authorizes continuation.
3. Add backend-specific model residency control through an explicit reviewed
   adapter, with ownership-aware unload, bounded loading and failure cleanup.
   OpenAI-compatible chat endpoints alone do not supply this lifecycle.
4. Validate authenticated real generation after the operator configures a scoped
   credential locally. Establish an available small generative baseline, review
   the held-out technical corpus, then execute comparable model studies. Never
   search token files, weaken server authentication or substitute mock scores.
5. Run installation/documentation walkthroughs and the complete release gate
   again against the final implementation. Preserve unavailable GPU attribution
   and energy measurements as unavailable.

Novel neural architectures and automatic neural promotion are not substitutes
for these infrastructure requirements. The goal explicitly permits optional
offline LoRA to remain experimental. No release tag or publication is authorized.
