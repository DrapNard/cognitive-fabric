# Reviewed empirical workflow routing

Jev+ can rank or decline optional workflow experts using operator-reviewed paired
baseline/expert outcomes. This is an optional native policy, not a trained model
or a demonstrated improvement in reasoning quality.

## Evidence and installation

`routing::empirical::EvidenceRouter::from_json` accepts bounded, strict JSON and
explicit `Thresholds`. Install the immutable router using
`Engine::with_expert_router(Arc::new(policy))`. The default router preserves the
existing tag-based workflow order. Installation is trusted host configuration;
models cannot install policies, change thresholds or grant permissions.

Inspect files without creating application state:

```sh
cogfabric workflow-study-evaluate study.json --thresholds thresholds.json
```

This command reports `installed: false`. It does not install a policy. Public
Rust structures in `src/routing/empirical.rs` define the versioned file format.
Provenance, license, reviewer, dataset hash and operator attestations of held-out
cases, independent scoring and inclusion of failed attempts are mandatory.
Attestations are not independently proven by the loader. No real utility study is
bundled; test studies are synthetic.

## Scope and decisions

Evidence must match the mode, compiled verification criterion (integer sorting,
dependency ordering or shortest path), configuration, role prompts, compiled
runtime sources, pinned knowledge cohort, workflow manifest and bound model
metadata. Knowledge identity includes selected documents and memories and pinned
skill/manifest metadata. It is not a guarantee about an entire external corpus.
Drift or absent evidence declines optional workflows under this policy.

Each paired case has distinct baseline/expert task IDs, a case hash, independently
scored success and recorded calls, reserved output tokens, prompt bytes, tools and
elapsed time. All cases count, including failures and regressions. Net benefit is
wins minus regressions, floored at zero. Cost sums positive extra reserved output
tokens without credit for savings and is at least the planned workflow output
reservation for every case. Integer thresholds enforce minimum samples, net wins,
net gain per thousand and maximum cost per net win. Candidates are ordered by
net benefit/cost, with deterministic identity ties. These are empirical ratios,
not calibrated probabilities or statistical generalization guarantees.

Remaining call/output/prompt/time budgets must cover conservative observed extra
costs, the planned workflow minimum and a main-solver reserve. A verification tool
must remain available. The existing deterministic workflow admission and kernel
limits still enforce every real operation; observations cannot expand them.

## Failure and security boundaries

Ranking runs after language capability refinement, so advisory capability output
cannot reintroduce a declined workflow. Inputs contain identities and bounded
resource metadata, not prompt bodies. The engine commits an intent before calling
the router and a decision before admitting expert work. Unknown/duplicate returned
IDs or failed audit writes fail the task before expert inference. Native callbacks
execute outside store locks; they remain trusted host code, not isolated plugins.

Files are size bounded, reject ambiguous duplicate keys and unknown fields, and
use regular-file reads that reject symlinks. At most 32 studies and 1,024 paired
cases are admitted. Policy identities cover canonical evidence and thresholds.
This feature grants no capabilities, proves no provider health, and supplies no
real-model speed, GPU efficiency or quality benchmark.
