# Provisional v1 task corpus

`v1_cases.jsonl` contains 100 original self-contained synthetic tasks across ten
categories, ten cases each: arithmetic, dependency ordering, inline SQL, logic,
data transformation, resource admission, permission boundaries, ambiguity,
recovery/state and versioned expert selection. No third-party dataset was copied.
The examples and labels in this directory are provided under CC0-1.0.

`v1_manifest.json` records the exact dataset SHA-256, stable case IDs, line numbers,
categories, authorship and explicit review limitations. Ordering cases include a
valid example in the manifest for test review; that manifest is not sent to the
inference engine. The evaluator accepts any order satisfying the supplied graph.

This is a provisional engineering corpus, not a published performance result.
Independent review, held-out independence and live-model evaluation are all false.
The suite was authored alongside the framework and can reflect its design biases.
It is not verified unseen data, a comprehensive security evaluation, or a neural
safety suite. Classification cases contain explicit premises; they test bounded
rule application rather than general domain knowledge or real privileged actions.
There are no Rust compilation, external filesystem or Git execution workloads.

## What is checked automatically

The Rust regression `authored_v1_suite_has_bounded_unique_cases_and_checked_tool_labels`
checks the checksum, case count, IDs, category balance, normalized uniqueness,
input bounds and criterion preflight. Ten arithmetic labels are recomputed by the
bounded parser, ten SQL labels by the task-local SQLite executor, and ten ordering
examples by the independent constraint checker. These use framework checkers;
they do not constitute external independent oracle review. The other 70 labels
require human/domain review. A parser-valid label is not proof of its correctness.

## Review and reproducible evaluation

Before claiming held-out results:

1. Have an independent reviewer check every objective, label and ordering graph;
   record reviewer identity, date, corrections and unresolved ambiguities.
2. Check contamination against training/RAG/workflow sources and document the
   reviewed split. Add independently sourced harder/unseen tasks when required.
   The 100-case size alone does not complete the architecture's study requirement.
3. Freeze the approved dataset, update its checksum and review metadata, and pin
   the code revision and actual inference configuration.
4. Run the comparison with a real local endpoint and record resource/quality
   results, failure cases and limitations. Review bound expert model disclosures.

After that review, use the normal `compare` command described in
`../docs/BENCHMARKING.md`, supplying this dataset, its actual reviewed provenance and
`--dataset-sha256` with the approved file hash.
Do not pass `--held-out` merely because this file exists. The current manifest
supplies no such attestation. Do not use `bench` for the mixed suite: dependency
ordering uses constraint scoring and SQL uses structural result scoring.

The Rust test executes deterministic tools only, without a model/GPU. No suite
accuracy, collective advantage or larger-model equivalence is claimed here.

## Public diagnostic evidence

The complete pre-contract-parity Qwen3 8B workflow (600 tasks) is retained under
[results](results/README.md). It exposed output-format fairness limitations and
strict-format false negatives; it does not supply the missing independent review
or a validated comparative study. The manifest's formal evaluation/review flags
remain false. The corrected full public rerun also completed all 600 tasks; raw reports and limitations are retained in the same results directory.

## Compiler-backed Rust ownership diagnostics

`rust_ownership_cases.jsonl` adds 20 public yes/no compilation questions, balanced
between valid and invalid Rust 2021 library snippets. The separate manifest pins
dataset/source hashes and the specific expected error code for each negative
case. Topics include moves, clones, Copy values, overlapping/sequential borrows,
mutable bindings, returned references, block lifetimes, iteration and closures.
These original snippets are provided under CC0-1.0. They supplement rather than
rewrite the historical 100-case suite or its reports.

`tests/rust_ownership_corpus.rs` invokes the configured trusted `rustc` on only
these fixed repository fixtures, emits metadata and checks exit status plus
specific negative diagnostic codes. It executes neither snippets nor model
output, uses no external crates and bounds each compiler wait to ten seconds
with owned-child cleanup. It also checks source/prompt correspondence, hashes,
balance, unique IDs and conservative input/diagnostic bounds. Run with
`cargo test --test rust_ownership_corpus -- --nocapture` to record compiler version.

This oracle is the Rust compiler, separate from the model and Fabric's built-in
checkers. It is not independent human curation, held-out data, general code
execution support or proof of Rust expertise. Compiler identity/version and
fixture provenance must accompany measurements. The manifest deliberately keeps
formal review/held-out/live-study flags false. Public diagnostics may use `bench`
or `compare --workflow-check`; never attest held-out status just because labels
compile as expected. Runtime tasks see source questions, not labels or compiler
diagnostics. The runtime itself receives no compiler tool capability.
