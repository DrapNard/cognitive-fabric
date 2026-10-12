# Public Qwen3 8B workflow study — 2026-10-13

This is diagnostic evidence, not held-out validation or proof of comparative reasoning gains.
The 100-case authored provisional corpus completed all 600 profile runs in 1138.773 seconds
using source `da2a8b9910afc9e782aa270531991d0c94430050`, Qwen3 8B and managed inference.
The raw report is retained to support independent inspection of all public answers and scores.

| Profile | Strict criterion matches / 100 | Model calls | Total task latency (seconds) |
|---|---:|---:|---:|
| alone | 2 | 100 | 256.396 |
| tools | 22 | 80 | 115.868 |
| recursive | 22 | 150 | 224.294 |
| collective | 50 | 156 | 184.072 |
| memory | 50 | 156 | 179.886 |
| experts | 49 | 156 | 178.069 |

The raw status counts are 118 completed, 438 unverified, 32 rejected and 12 needing
clarification, with no failed tasks. A strict criterion match is separate from
runtime evidence verification. For example, model-alone answered the fragile-item
question correctly with “Yes” followed by an explanation; the yes/no exact-match
criterion rejected the extra text. Do not label every strict mismatch wrong reasoning.

Known limitations:

- Before fixes `ba5dac1` / `9fec58b`, the baseline lacked explicit sorting/ordering,
  arithmetic and SQL output-format instructions supplied or embodied by other profiles.
- The public corpus was authored with the framework and lacks independent label review
  or held-out contamination review. Scores include format adherence.
- Memory and experts used an empty reference corpus and no specialized model bindings.
  Their labels do not establish retrieval or specialist benefits.
- There was no controlled warmup; host compiler and smoke activity overlapped portions
  of this diagnostic run. Before/after latency cannot establish causal gains.
- This used Qwen3 8B, not a 1–3B baseline; there was no larger-model arm, weight
  attestation, task GPU/energy attribution or broad neural security evaluation.

All profiles used ceilings of three calls, one cycle, 768 reserved output tokens,
8192 prompt bytes, four tools and 45 seconds per task, with a 30-second HTTP timeout.
Temperature was zero and requested reasoning effort was none. The run limit was
1800 seconds. Expected outputs stayed outside inference requests.

Raw report: [qwen3-8b-v1-pre-contract-parity.json](qwen3-8b-v1-pre-contract-parity.json).
Category/resource summary: [qwen3-8b-v1-pre-contract-parity-summary.json](qwen3-8b-v1-pre-contract-parity-summary.json).
Raw SHA-256: `b4fa58005f2e0498047826404642a275335ef11fac2a5efd761006f6a9596299`.
Dataset SHA-256: `6b2c7dd82bce6473f527daff8be56c5bcf3e54e7a93db088cb46def5fc06cd05`.

The corrected full rerun uses source `9fec58b` with a frozen separate executable
and fresh state. It must complete before any new scores are reported.
