# Joint Jev+ capability resolution

After optional language interpretation, Jev+ resolves up to 16 requested capability
IDs against the task's pinned enabled skill/manifest/model snapshot. Interpretation
is advisory: it cannot change the original objective, permissions or budgets.
Canonical IDs follow the registry grammar; invalid interpretation falls back to
the original request.

## Selection

The `bounded-cover/v3` pipeline preserves the initial router's selected IDs and
order except exclusively version-incompatible routes. Available compatible
coverage from those retained routes is reused. At most four total routes remain
selected. An unavailable retained route still consumes its original slot.

When a complete declared cover fits the remaining slots, `exact-cover/1` finds
one with the minimum number of additional providers. Providers must be enabled,
version compatible and available in the pinned snapshot. Equivalent uncovered
coverage masks share a deterministic representative, preferring a directly
requested skill identity and then lexical ID. Input skill, manifest and requested
ID order do not affect new selections. Retained route order remains authoritative.
Ties are deterministic; global lexicographic optimality is not claimed.

If no complete cover fits, the existing greedy rule supplies partial coverage:
choose the most still-uncovered compatible capabilities, then direct identity and
lexical ID. This partial fallback is not a maximum-coverage optimizer. Missing,
incompatible, unavailable and available-but-unselected requirements stay distinct.

A model-bound workflow without an available pinned model registration is excluded
from additions. Here "available" means declared version/registration prerequisites,
not endpoint health, weight identity, executable provisioning, permission or task
usefulness. Existing workflow ranking, per-call resource admission and independent
authorization still decide what runs; at most one workflow is admitted by the
current execution rules. Selection adds no model call or network probe.

### Why the complete-cover result is exact

Every feasible set of at most four additional providers can be split into two
groups of at most two. The solver records the smallest deterministic representation
of every one/two-provider union mask. A superset transform supplies the smallest
second group that covers each first group's missing bits. Replacing either group
with an equal-coverage or covering representation cannot increase its size;
merging repeated providers can only reduce the final count. Enumerating all first
union masks therefore finds a complete cover with the minimum additional route
count whenever one fits. This proof concerns declared compatible masks only.

### Work and resource bounds

The original ceilings remain 16 requested IDs, 4,096 pinned skills/manifests and
four total routes. The exact search uses 16-bit masks and compact pair indices,
with at most 8,386,560 distinct provider-pair visits, two tables of at most 65,536
entries, and 16 bounded superset passes. It creates no allocation per pair. Cheap
complete-cover and impossible-union cases avoid the full transform. These are
logical work/allocation bounds, not hard process-memory or physical CPU limits.

The engine yields before and after synchronous metadata resolution, allowing a
ready cancellation or expired task clock to win before route publication and
further inference. A cancel arriving during the synchronous search is observed
after it returns; this is not preemptive cancellation or a hard latency guarantee.
No task deadline, call/token/tool ceiling or queued resource allowance is extended.

Minimum provider count is not calibrated usefulness, weighted inference cost,
measured backbone sharing, VRAM savings or a trained decision network. Jev+ and
Jev-R remain partial.

## Observable distinctions

A `capability_resolution` event records `bounded-cover/v3`, selected IDs, binding
versions and these distinct lists:

| Field | Meaning |
| --- | --- |
| `missing` | No enabled declaration provides this identity or alias. |
| `incompatible` | Enabled declarations exist, but none matches the explicit version requirement (or a legacy category has no declared version). |
| `unavailable` | Compatible declarations exist, but all matching workflow bindings lack a usable pinned model registration. |
| `unselected` | Available declarations exist but the route ceiling or an ablation exclusion prevents selection. |

Each binding records the selected or best remaining declared provider, versions,
selection state and prerequisite availability. Legacy skill categories have no
manifest/capability version. All bindings retain `declaration_only: true` and
`permissions_granted: false`. `provider_health_proven` is false.

When refinement is enabled, `resolution.coverage_search` records the exact search
version, conditional full-cover feasibility, eligible provider/mask counts, pair
visits, added-route count and whether partial greedy fallback was used. It retains
`declaration_only:true` and `permissions_granted:false`. Profiles excluding
refinement omit this optional search metadata and acquire no new routes.

The event must persist before route refinement and downstream expert admission;
audit failure stops the task with original charges retained. Existing `route_refined`
version bindings remain available. `capability_gap.missing` preserves its historical
meaning of absent declarations; incompatible, unavailable and unselected cases are separately
visible in the resolution event. No RAG candidate is automatically created.

## Scope and evidence

Resolution uses only the existing bounded snapshot: at most 4,096 skills and
manifests, 16 requested identities and four selected routes. It does not reopen the
registry, mutate skills, load models or grant tools. A disable/version replacement
after pinning affects future tasks, not an in-flight resolution. Profiles excluding
expert refinement do not acquire extra routes.

Tests cover overlapping requirements, order invariance, stable ties, legacy IDs,
disabled candidates, unavailable models, capacity/exclusion distinctions, malformed
inputs, actual workflow execution, pinned disable, fallback providers and audit
failure before further inference. These synthetic checks establish behavior, not
live-model quality, VRAM savings or superiority over a baseline. Full and Experts tasks can use [pinned capability-directed retrieval](CAPABILITY_RETRIEVAL.md)
after resolution; its separate snapshot, authorization, budget and audit gates apply.
Calibrated provider utility and broader module scheduling remain incomplete;
operator versioned lookup remains separate.


Version 2 introduced optional version filtering. Version 3 adds exact complete
coverage while preserving those version requirements and original route pins.

## Optional declared-version requirements

The interpreter may include `capability_versions` for explicitly requested versions:

```json
{
  "objective": "Use a sorting capability compatible with version 2",
  "constraints": [],
  "required_capabilities": ["domain.sort"],
  "capability_versions": {"domain.sort": "^2"},
  "clarification": null
}
```

The optional map has at most 16 entries. Keys must already appear in
`required_capabilities`; every nonempty requirement is capped at 64 bytes and
parsed with the existing Rust SemVer implementation. Duplicate JSON keys, hidden
capability requests, invalid ranges and non-string values reject interpretation.
The original request remains authoritative and normal invalid-interpretation
fallback applies. The interface is instructed to include requirements only when
specified by the user. Correct extraction from natural language remains unproven.

A direct skill-ID requirement matches its active pinned manifest version. An alias
requirement matches that alias's declared capability version, which can differ
from the provider manifest version. Unknown legacy category versions cannot satisfy
an explicit requirement, including `*`; absent requirements preserve legacy
behavior and the old interpretation JSON shape. Prerelease matching uses the
SemVer library's range rules rather than string ordering. Compatible providers are
chosen by complete coverage, minimum additional count and deterministic ties, not automatic latest-version
activation or download.

`version_requirement` and `version_compatible` expose the binding decision.
`excluded_version_routes` records initial providers whose requested declarations
are all incompatible; they cannot consume the optional workflow opportunity just
because tags selected them earlier. A mixed provider can remain for its compatible
requested capabilities, but receives no credit for mismatches. Unrelated initial
routes retain their order. Capability-directed retrieval requires a selected,
available, version-compatible binding.

Compatibility here proves only a match against operator-declared metadata. It does
not attest API implementation, loaded weights, health, usefulness or fulfillment of
all requested capabilities. Existing budgets can still admit only one workflow.
No requirement activates retired/disabled versions or grants permissions. Later
activation affects future tasks while the current task retains its pinned version.

The updated interface prompt changes the compiled role-prompt fingerprint. Existing
recovery checks may refuse older checkpoints with a different pinned prompt hash;
this change does not silently migrate them. Native Rust `Interpretation` literals
now initialize the optional map explicitly (empty for legacy requests).

## Deterministic cost check (2026-10-22)

The fixed stress test builds 4096 distinct five-bit providers over 16 requirements.
Any three cover at most 15 requirements; a four-provider cover is present. The
exact kernel visits all 8,386,560 pairs, returns four providers, and uses 786432
bytes of pair-table entries. That number excludes provider strings, pinned data,
allocator/stack overhead and total process RSS.

Reproduce the selection-only timer with the ordinary unoptimized test profile:

```bash
cargo test --locked --lib capabilities::cover::tests::worst_case_pair_table_visits_are_bounded_without_per_pair_allocations -- --exact --nocapture
```

Two sequential local observations were 1089.067 and 1083.882 milliseconds for
the kernel after fixture construction. They exclude Cargo compilation/startup
and are not optimized-build, end-to-end routing, physical CPU-limit, GPU or model
quality measurements. Host load/thermal state was not controlled. Small complete
one-provider cases bypass pair construction; two-route capacity and complete
pairs avoid the superset transform. No general efficiency improvement is inferred
from this synthetic cost check.

Final workspace verification passes: formatting, all-target build, **804 tests
across 47 targets** (none ignored), and Clippy with warnings denied. The focused
capability selection has 18 passing tests; Engine coverage runs the three enabled
profiles and checks actual pinned workflow dispatch. Existing protocol/socket,
persistence, denial, cancellation, recovery and rollback checks remain green.
`flatc` is unavailable; unchanged bundled protocol pairing/integrity tests pass.
