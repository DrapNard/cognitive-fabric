# Joint Jev+ capability resolution

After optional language interpretation, Jev+ resolves up to 16 requested capability
IDs against the task's pinned enabled skill/manifest/model snapshot. Interpretation
is advisory: it cannot change the original objective, permissions or budgets.
Canonical IDs follow the registry grammar; invalid interpretation falls back to
the original request.

## Selection

The deterministic `greedy-cover/v1` algorithm preserves the initial router's
selected IDs and order, reuses their available coverage, then adds the candidate
covering the most still-uncovered requirements. Equal coverage prefers a directly
requested skill identity, then lexical provider ID. At most four total routes are
selected, as before. One provider can cover many requirements without consuming
several routes. Input skill, manifest and requirement ordering does not affect
new selections. Existing route priority remains authoritative.

A model-bound workflow without an available pinned model registration is excluded
from additions. An alternative enabled provider may cover its aliases. Here
"available" means only that this configured-registration prerequisite is met;
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
| `unavailable` | Declarations exist, but all matching workflow bindings lack a usable pinned model registration. |
| `unselected` | Available declarations exist but the route ceiling or an ablation exclusion prevents selection. |

Each binding records the selected or best remaining declared provider, versions,
selection state and prerequisite availability. Legacy skill categories have no
manifest/capability version. All bindings retain `declaration_only: true` and
`permissions_granted: false`. `provider_health_proven` is false.

The event must persist before route refinement and downstream expert admission;
audit failure stops the task with original charges retained. Existing `route_refined`
version bindings remain available. `capability_gap.missing` preserves its historical
meaning of absent declarations; unavailable and unselected cases are separately
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
Semver requirement negotiation in interpreted requests, calibrated provider utility and broader
module scheduling remain incomplete; operator versioned lookup is separate.
