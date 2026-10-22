use super::*;
use serde_json::json;
fn candidate(id: &str, aliases: &[&str]) -> (Skill, Manifest) {
    let manifest:Manifest=serde_json::from_value(json!({"id":id,"version":"1.0.0","kind":"reference","description":"Synthetic capability metadata","tags":["fixture"],"capabilities":aliases.iter().map(|id|json!({"id":id,"version":"2.1.0"})).collect::<Vec<_>>(),"source":"Synthetic regression fixture","license":"CC0-1.0"})).unwrap();
    manifest.validate().unwrap();
    (
        Skill {
            id: id.into(),
            description: manifest.description.clone(),
            tags: manifest.tags.clone(),
            active_version: 1,
            enabled: true,
            manifest_version: Some("1.0.0".into()),
        },
        manifest,
    )
}
fn run(
    pairs: &[(Skill, Manifest)],
    requirements: &[&str],
    initial: &[&str],
    refine: bool,
) -> Resolution {
    resolve(
        &pairs.iter().map(|p| p.0.clone()).collect::<Vec<_>>(),
        &pairs.iter().map(|p| p.1.clone()).collect::<Vec<_>>(),
        &BTreeMap::new(),
        &requirements
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        &BTreeMap::new(),
        &initial.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        refine,
    )
    .unwrap()
}
#[test]
fn joint_coverage_reuses_routes_and_avoids_redundant_additions_independent_of_order() {
    let mut pairs = vec![
        candidate("expert.alpha", &["domain.one"]),
        candidate("expert.beta", &["domain.two"]),
        candidate("expert.joint", &["domain.one", "domain.two"]),
    ];
    let first = run(&pairs, &["domain.one", "domain.two"], &[], true);
    assert_eq!(first.skills, vec!["expert.joint"]);
    assert!(first.bindings.iter().all(|b| b.selected
        && b.available
        && b.capability_version.as_deref() == Some("2.1.0")
        && !b.permissions_granted));
    pairs.reverse();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(run(&pairs, &["domain.two", "domain.one"], &[], true)).unwrap()
    );
    let reused = run(
        &pairs,
        &["domain.one", "domain.two"],
        &["expert.alpha"],
        true,
    );
    assert_eq!(reused.skills, vec!["expert.alpha", "expert.beta"]);
}
#[test]
fn distinguishes_absence_unavailable_model_and_route_capacity_without_granting_work() {
    let mut unavailable = candidate("expert.unavailable", &["domain.offline"]);
    unavailable.1.kind = "workflow".into();
    unavailable.1.workflow = Some("Return bounded advice".into());
    unavailable.1.model = Some(crate::models::Selection {
        id: "fixture.model".into(),
        version: "1.0.0".into(),
    });
    unavailable.1.validate().unwrap();
    let mut pairs = vec![unavailable, candidate("expert.extra", &["domain.extra"])];
    for id in ["route.one", "route.two", "route.three", "route.four"] {
        pairs.push(candidate(id, &[]));
    }
    let result = run(
        &pairs,
        &["domain.missing", "domain.offline", "domain.extra"],
        &["route.one", "route.two", "route.three", "route.four"],
        true,
    );
    assert_eq!(result.skills.len(), 4);
    assert_eq!(result.missing, vec!["domain.missing"]);
    assert_eq!(result.unavailable, vec!["domain.offline"]);
    assert_eq!(result.unselected, vec!["domain.extra"]);
    let disabled = run(&pairs, &["domain.extra"], &[], false);
    assert!(disabled.skills.is_empty());
    assert_eq!(disabled.unselected, vec!["domain.extra"]);
}
#[test]
fn prefers_direct_identity_on_equal_coverage_and_excludes_disabled_candidates() {
    let mut pairs = vec![
        candidate("aaa.alias", &["domain.one"]),
        candidate("domain.one", &[]),
    ];
    assert_eq!(
        run(&pairs, &["domain.one"], &[], true).skills,
        vec!["domain.one"]
    );
    pairs[1].0.enabled = false;
    assert_eq!(
        run(&pairs, &["domain.one"], &[], true).skills,
        vec!["aaa.alias"]
    );
    let legacy = pairs[0].0.clone();
    let result = resolve(
        &[legacy],
        &[],
        &BTreeMap::new(),
        &["aaa.alias".into()],
        &BTreeMap::new(),
        &[],
        true,
    )
    .unwrap();
    assert_eq!(result.skills, vec!["aaa.alias"]);
    assert!(result.bindings[0].provider_version.is_none());
}
#[test]
fn rejects_duplicate_or_out_of_bounds_inputs_before_selection() {
    let (skill, manifest) = candidate("expert.one", &["domain.one"]);
    for requirements in [
        vec!["domain.one".into(); 17],
        vec!["domain.one".into(); 2],
        vec!["../escape".into()],
    ] {
        assert!(resolve(
            std::slice::from_ref(&skill),
            std::slice::from_ref(&manifest),
            &BTreeMap::new(),
            &requirements,
            &BTreeMap::new(),
            &[],
            true
        )
        .is_err());
    }
    assert!(resolve(
        &[skill.clone(), skill.clone()],
        &[],
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        &[],
        true
    )
    .is_err());
    assert!(resolve(
        std::slice::from_ref(&skill),
        &[manifest.clone(), manifest],
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        &[],
        true
    )
    .is_err());
    for initial in [
        vec!["expert.unknown".into()],
        vec!["expert.one".into(); 2],
        vec!["expert.one".into(); 5],
    ] {
        assert!(resolve(
            std::slice::from_ref(&skill),
            &[],
            &BTreeMap::new(),
            &[],
            &BTreeMap::new(),
            &initial,
            true
        )
        .is_err());
    }
}

#[test]
fn versioned_coverage_excludes_incompatible_initial_routes_and_is_order_invariant() {
    let mut old = candidate("a.old", &["domain.one"]);
    old.1.capabilities[0].version = "1.9.0".into();
    let new = candidate("z.new", &["domain.one"]);
    let mut pairs = vec![old, new];
    let versions = BTreeMap::from([("domain.one".into(), "^2".into())]);
    let resolve_pairs = |pairs: &[(Skill, Manifest)]| {
        resolve(
            &pairs.iter().map(|p| p.0.clone()).collect::<Vec<_>>(),
            &pairs.iter().map(|p| p.1.clone()).collect::<Vec<_>>(),
            &BTreeMap::new(),
            &["domain.one".into()],
            &versions,
            &["a.old".into()],
            true,
        )
        .unwrap()
    };
    let result = resolve_pairs(&pairs);
    assert_eq!(result.skills, vec!["z.new"]);
    assert_eq!(result.excluded_version_routes, vec!["a.old"]);
    assert!(result.bindings[0].version_compatible);
    assert_eq!(
        result.bindings[0].version_requirement.as_deref(),
        Some("^2")
    );
    assert_eq!(
        result.bindings[0].capability_version.as_deref(),
        Some("2.1.0")
    );
    pairs.reverse();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::to_value(resolve_pairs(&pairs)).unwrap()
    );
    assert_eq!(
        run(&pairs, &["domain.one"], &["a.old"], true).skills,
        vec!["a.old"]
    );
}
#[test]
fn explicit_versions_distinguish_unknown_legacy_and_mismatched_declarations() {
    let (skill, manifest) = candidate("expert.one", &["domain.one"]);
    for (capability, requirement, compatible) in [
        ("expert.one", "^1", true),
        ("expert.one", "^2", false),
        ("domain.one", "^2", true),
        ("domain.one", "<2", false),
    ] {
        let result = resolve(
            std::slice::from_ref(&skill),
            std::slice::from_ref(&manifest),
            &BTreeMap::new(),
            &[capability.into()],
            &BTreeMap::from([(capability.into(), requirement.into())]),
            &[],
            true,
        )
        .unwrap();
        assert_eq!(result.bindings[0].version_compatible, compatible);
        assert_eq!(result.skills.is_empty(), !compatible);
        assert!(result.missing.is_empty());
        assert_eq!(result.incompatible.is_empty(), compatible);
    }
    let legacy = resolve(
        std::slice::from_ref(&skill),
        &[],
        &BTreeMap::new(),
        &["expert.one".into()],
        &BTreeMap::from([("expert.one".into(), "*".into())]),
        &[],
        true,
    )
    .unwrap();
    assert_eq!(legacy.incompatible, vec!["expert.one"]);
    assert!(!legacy.bindings[0].selected);
    assert!(legacy.bindings[0].capability_version.is_none());
    let mut prerelease = manifest.clone();
    prerelease.capabilities[0].version = "2.0.0-alpha.1".into();
    for (requirement, compatible) in [("^2", false), ("^2.0.0-alpha.1", true)] {
        let result = resolve(
            std::slice::from_ref(&skill),
            std::slice::from_ref(&prerelease),
            &BTreeMap::new(),
            &["domain.one".into()],
            &BTreeMap::from([("domain.one".into(), requirement.into())]),
            &[],
            true,
        )
        .unwrap();
        assert_eq!(result.bindings[0].version_compatible, compatible);
    }
}

#[test]
fn mixed_coverage_keeps_only_compatible_credit_and_separates_model_unavailability() {
    let mut partial = candidate("a.partial", &["domain.one", "domain.two"]);
    partial.1.capabilities[0].version = "1.9.0".into();
    let other = candidate("z.other", &["domain.one"]);
    let ids = vec!["domain.one".into(), "domain.two".into()];
    let versions = BTreeMap::from([
        ("domain.one".into(), "^2".into()),
        ("domain.two".into(), "^2".into()),
    ]);
    let result = resolve(
        &[partial.0.clone(), other.0],
        &[partial.1.clone(), other.1],
        &BTreeMap::new(),
        &ids,
        &versions,
        &["a.partial".into()],
        true,
    )
    .unwrap();
    assert_eq!(result.skills, vec!["a.partial", "z.other"]);
    assert!(result.excluded_version_routes.is_empty());
    assert_eq!(result.bindings[0].provider.as_deref(), Some("z.other"));
    assert_eq!(result.bindings[1].provider.as_deref(), Some("a.partial"));
    assert!(result
        .bindings
        .iter()
        .all(|b| b.selected && b.version_compatible));
    let mut bound = candidate("bound.workflow", &["domain.one"]);
    bound.1.kind = "workflow".into();
    bound.1.workflow = Some("Return bounded advice".into());
    bound.1.model = Some(crate::models::Selection {
        id: "fixture.model".into(),
        version: "1.0.0".into(),
    });
    for (requirement, compatible) in [("^2", true), ("^1", false)] {
        let result = resolve(
            std::slice::from_ref(&bound.0),
            std::slice::from_ref(&bound.1),
            &BTreeMap::new(),
            &["domain.one".into()],
            &BTreeMap::from([("domain.one".into(), requirement.into())]),
            &[],
            true,
        )
        .unwrap();
        assert_eq!(result.incompatible.is_empty(), compatible);
        assert_eq!(result.unavailable.is_empty(), !compatible);
        assert!(result.missing.is_empty());
        assert!(result.skills.is_empty());
    }
}

#[test]
fn exact_cover_avoids_the_greedy_trap_and_uses_the_minimum_number_of_routes() {
    // `a.trap` covers five requirements, so greedy selection takes it first.
    // That leaves at least one uncovered requirement in each of four disjoint
    // groups and needs five total routes. The four group providers are a full
    // cover within the actual four-route limit.
    let requirements = (0..12)
        .map(|i| format!("domain.cap{i}"))
        .collect::<Vec<_>>();
    let mut pairs = vec![candidate(
        "a.trap",
        &[
            "domain.cap0",
            "domain.cap3",
            "domain.cap6",
            "domain.cap9",
            "domain.cap1",
        ],
    )];
    for (name, start) in [("b.one", 0), ("c.two", 3), ("d.three", 6), ("e.four", 9)] {
        let aliases = (start..start + 3)
            .map(|i| format!("domain.cap{i}"))
            .collect::<Vec<_>>();
        let alias_refs = aliases.iter().map(String::as_str).collect::<Vec<_>>();
        pairs.push(candidate(name, &alias_refs));
    }

    let result = resolve(
        &pairs.iter().map(|pair| pair.0.clone()).collect::<Vec<_>>(),
        &pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>(),
        &BTreeMap::new(),
        &requirements,
        &BTreeMap::new(),
        &[],
        true,
    )
    .unwrap();

    assert_eq!(result.skills.len(), 4);
    assert!(!result.skills.contains(&"a.trap".to_owned()));
    assert!(result.unselected.is_empty());
    assert!(result.missing.is_empty());
    assert!(result.incompatible.is_empty());
    assert!(result.unavailable.is_empty());
    assert!(result.bindings.iter().all(|binding| binding.selected));
}

#[test]
fn exact_cover_is_order_invariant_for_requirements_and_provider_snapshots() {
    let requirements = (0..12)
        .map(|i| format!("domain.cap{i}"))
        .collect::<Vec<_>>();
    let mut pairs = vec![candidate(
        "a.trap",
        &[
            "domain.cap0",
            "domain.cap3",
            "domain.cap6",
            "domain.cap9",
            "domain.cap1",
        ],
    )];
    for (name, start) in [("b.one", 0), ("c.two", 3), ("d.three", 6), ("e.four", 9)] {
        let aliases = (start..start + 3)
            .map(|i| format!("domain.cap{i}"))
            .collect::<Vec<_>>();
        pairs.push(candidate(
            name,
            &aliases.iter().map(String::as_str).collect::<Vec<_>>(),
        ));
    }
    let run_order = |pairs: &[(Skill, Manifest)], requirements: &[String]| {
        resolve(
            &pairs.iter().map(|pair| pair.0.clone()).collect::<Vec<_>>(),
            &pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>(),
            &BTreeMap::new(),
            requirements,
            &BTreeMap::new(),
            &[],
            true,
        )
        .unwrap()
    };
    let original = run_order(&pairs, &requirements);
    pairs.reverse();
    let reversed_requirements = requirements.iter().cloned().rev().collect::<Vec<_>>();
    let reversed = run_order(&pairs, &reversed_requirements);
    assert_eq!(
        serde_json::to_value(original).unwrap(),
        serde_json::to_value(reversed).unwrap()
    );
}

#[test]
fn exact_cover_matches_an_independent_minimum_cover_oracle_on_small_registries() {
    fn oracle_minimum(masks: &[u16], target: u16, slots: usize) -> Option<usize> {
        (0..=slots).find(|count| {
            (0u64..(1u64 << masks.len())).any(|subset| {
                subset.count_ones() as usize == *count
                    && masks
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| subset & (1u64 << index) != 0)
                        .fold(0u16, |cover, (_, mask)| cover | mask)
                        & target
                        == target
            })
        })
    }

    // A small deterministic generator keeps this test reproducible while
    // exercising dense, sparse, duplicate-mask, and impossible registries.
    let mut seed = 0x8a5c_31d7u32;
    for case in 0..160 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let requirement_count = 1 + (seed as usize % 8);
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let candidate_count = 1 + ((seed >> 8) as usize % 9);
        let requirements = (0..requirement_count)
            .map(|i| format!("domain.cap{i}"))
            .collect::<Vec<_>>();
        let full_mask = (1u16 << requirement_count) - 1;
        let mut pairs = Vec::new();
        let mut masks = Vec::new();
        for candidate_index in 0..candidate_count {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mask = (seed as u16) & full_mask;
            masks.push(mask);
            let aliases = requirements
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, id)| id.as_str())
                .collect::<Vec<_>>();
            pairs.push(candidate(&format!("expert.p{candidate_index}"), &aliases));
        }
        let retained = masks[0];
        let expected = oracle_minimum(&masks[1..], full_mask & !retained, 3);
        let result = resolve(
            &pairs.iter().map(|pair| pair.0.clone()).collect::<Vec<_>>(),
            &pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>(),
            &BTreeMap::new(),
            &requirements,
            &BTreeMap::new(),
            &[pairs[0].0.id.clone()],
            true,
        )
        .unwrap();
        if let Some(minimum) = expected {
            assert_eq!(
                result.skills.len(),
                minimum + 1,
                "case {case}: expected retained route plus minimum full cover of {minimum} routes"
            );
            assert!(result.unselected.is_empty(), "case {case}");
            assert!(
                result.bindings.iter().all(|binding| binding.selected),
                "case {case}"
            );
        } else {
            assert!(
                !result.unselected.is_empty() || !result.missing.is_empty(),
                "case {case}: no full cover exists within the remaining three routes"
            );
        }
    }
}

#[test]
fn exact_cover_uses_only_enabled_available_version_compatible_declarations() {
    let mut trap = candidate(
        "a.trap",
        &["domain.cap0", "domain.cap1", "domain.cap2", "domain.cap3"],
    );
    trap.1.capabilities[0].version = "1.9.0".into();
    let ready_a = candidate("b.ready_a", &["domain.cap0", "domain.cap1"]);
    let ready_b = candidate("c.ready_b", &["domain.cap2", "domain.cap3"]);
    let mut disabled = candidate(
        "d.disabled",
        &["domain.cap0", "domain.cap1", "domain.cap2", "domain.cap3"],
    );
    disabled.0.enabled = false;
    let mut unavailable = candidate(
        "e.unavailable",
        &["domain.cap0", "domain.cap1", "domain.cap2", "domain.cap3"],
    );
    unavailable.1.kind = "workflow".into();
    unavailable.1.workflow = Some("Unavailable workflow fixture".into());
    unavailable.1.model = Some(crate::models::Selection {
        id: "fixture.model".into(),
        version: "1.0.0".into(),
    });
    unavailable.1.validate().unwrap();
    let pairs = [trap, ready_a, ready_b, disabled, unavailable];
    let requirements = (0..4).map(|i| format!("domain.cap{i}")).collect::<Vec<_>>();
    let versions = requirements
        .iter()
        .map(|id| (id.clone(), "^2".to_owned()))
        .collect::<BTreeMap<_, _>>();
    let result = resolve(
        &pairs.iter().map(|pair| pair.0.clone()).collect::<Vec<_>>(),
        &pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>(),
        &BTreeMap::new(),
        &requirements,
        &versions,
        &[],
        true,
    )
    .unwrap();

    // A mixed provider remains eligible for its compatible declarations.
    // It cannot provide cap0, but can combine with ready_a for a two-route cover.
    assert_eq!(result.skills, vec!["a.trap", "b.ready_a"]);
    assert!(!result.skills.contains(&"d.disabled".to_owned()));
    assert!(!result.skills.contains(&"e.unavailable".to_owned()));
    assert!(result.unselected.is_empty());
    assert!(result.incompatible.is_empty());
    assert!(result.bindings.iter().all(|binding| {
        binding.selected && binding.version_compatible && !binding.permissions_granted
    }));
    assert_eq!(result.bindings[0].provider.as_deref(), Some("b.ready_a"));
    assert_eq!(
        result.bindings[0].version_requirement.as_deref(),
        Some("^2")
    );

    let without_refinement = resolve(
        &pairs.iter().map(|pair| pair.0.clone()).collect::<Vec<_>>(),
        &pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>(),
        &BTreeMap::new(),
        &requirements,
        &versions,
        &[],
        false,
    )
    .unwrap();
    assert!(without_refinement.skills.is_empty());
    assert_eq!(without_refinement.unselected.len(), 4);
    assert!(without_refinement
        .bindings
        .iter()
        .all(|binding| !binding.selected && !binding.permissions_granted));
}

#[test]
fn retained_unavailable_route_consumes_capacity_but_never_counts_as_coverage() {
    let retained = candidate("a.retained", &["domain.cap0"]);
    let mut unavailable = candidate("b.unavailable", &["domain.cap1"]);
    unavailable.1.kind = "workflow".into();
    unavailable.1.workflow = Some("Unavailable workflow fixture".into());
    unavailable.1.model = Some(crate::models::Selection {
        id: "fixture.model".into(),
        version: "1.0.0".into(),
    });
    unavailable.1.validate().unwrap();
    let rest = candidate("c.rest", &["domain.cap1", "domain.cap2", "domain.cap3"]);
    let pairs = [retained, unavailable, rest];
    let result = resolve(
        &pairs.iter().map(|pair| pair.0.clone()).collect::<Vec<_>>(),
        &pairs.iter().map(|pair| pair.1.clone()).collect::<Vec<_>>(),
        &BTreeMap::new(),
        &[
            "domain.cap0".into(),
            "domain.cap1".into(),
            "domain.cap2".into(),
            "domain.cap3".into(),
        ],
        &BTreeMap::new(),
        &["a.retained".into(), "b.unavailable".into()],
        true,
    )
    .unwrap();

    assert_eq!(result.skills, vec!["a.retained", "b.unavailable", "c.rest"]);
    assert!(result.unselected.is_empty());
    assert!(result.unavailable.is_empty());
    assert_eq!(result.bindings[1].provider.as_deref(), Some("c.rest"));
    assert!(result.bindings[1].available);
    assert!(result.bindings[1].selected);
    assert!(result
        .bindings
        .iter()
        .all(|binding| !binding.permissions_granted));
}
