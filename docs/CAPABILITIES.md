# Joint Jev+ capability resolution

After optional language interpretation, Jev+ resolves up to 16 requested capability
IDs against the task's pinned enabled skill/manifest/model snapshot. Interpretation
is advisory: it cannot change the original objective, permissions or budgets.
Canonical IDs follow the registry grammar; invalid interpretation falls back to
the original request.

## Selection

The deterministic `greedy-cover/v2` algorithm preserves the initial router's
selected IDs and order except exclusively version-incompatible routes, reuses
their available compatible coverage, then adds the candidate
covering the most still-uncovered requirements. Equal coverage prefers a directly
requested skill identity, then lexical provider ID. At most four total routes are
selected, as before. One provider can cover many requirements without consuming
several routes. Input skill, manifest and requirement ordering does not affect
new selections. Retained route priority remains authoritative.

A model-bound workflow without an available pinned model registration is excluded
from additions. An alternative enabled provider may cover its aliases. Here
"available" means only that declared version and configured-registration
prerequisites are met;
it is not endpoint health, weight identity, executable provisioning, permission
or task usefulness. Unbound workflows still use their configured default adapter.

This is a bounded greedy heuristic, not globally optimal set cover, calibrated
utility or a trained decision network. Selection adds no model call or network
probe. Existing workflow ranking, resource admission and per-dispatch authorization
still decide whether any chosen workflow actually runs. At most one workflow is
admitted under the existing execution rules.

## Observable distinctions

A `capability_resolution` event records the algorithm, selected IDs, binding
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


Version 2 adds the optional version filtering described below; unversioned
coverage and tie behavior are retained.

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
still chosen by coverage and deterministic ties, not automatic latest-version
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
