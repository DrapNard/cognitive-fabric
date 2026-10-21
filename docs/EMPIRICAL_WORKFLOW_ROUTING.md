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
dependency ordering, shortest path or finite-domain constraint satisfaction),
configuration, role prompts, compiled
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

## Direct CLI and local service deployment

Explicitly install a reviewed policy for one direct task:

```sh
cogfabric ask 'sort:{"values":[3,1,2]}' --mode deep \
  --workflow-study study.json --workflow-thresholds thresholds.json
```

Or pin it for the local Unix service lifetime:

```sh
cogfabric serve --workflow-study study.json --workflow-thresholds thresholds.json
```

Both options are required together. Invalid/ambiguous/oversized files and symlinks
are rejected before workspace state or backends are initialized. `ask --socket`
rejects these options: the service operator chooses the service's policy, and a
socket client cannot replace it. Omitted options retain the default tag router.

A host reads and validates the files exactly once at startup. Replacing either
file does not alter its policy or admitted tasks. A new process must explicitly
load the revised policy. No configuration file is changed, no executable plugin
is loaded, and no model capabilities are granted. Every task records the policy
identity. This deployment applies to direct Core tasks and the Unix service. The
optional HTTP server accepts the same paired startup options:

```sh
cogfabric-server --home /path/to/private/state --bind 127.0.0.1:8234 \
  --workflow-study study.json --workflow-thresholds thresholds.json
```

The HTTP host validates and pins the policy before creating workspace state or
constructing backends. It applies to collective aliases; native `cogfabric-agent`
turns retain their one-call conversation path. HTTP clients cannot submit policy
files or thresholds. Worker entry points reject these host options. Omission
preserves the default router. The existing dedicated server bearer key is still
required. Continuation supports only its existing post-routing ready checkpoints.
It does not rerun a missing routing phase or reconstruct a router from stored metadata;
policy metadata alone cannot restore code or establish utility.

The end-to-end fixture covers accepted and declined direct tasks, actual HTTP
inference, Unix service tasks, and file replacement after service startup. These
are synthetic functional checks, not a held-out effectiveness study.

Admitted [capability-directed references](CAPABILITY_RETRIEVAL.md) extend the
pinned evidence fingerprint with canonical document content/provenance identities.
Prior studies do not automatically cover that enlarged knowledge cohort.
