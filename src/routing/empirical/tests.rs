use super::*;
use crate::routing::WorkflowCandidate;
fn measurement(verified: bool, extra: u32) -> Measurement {
    Measurement {
        task_id: uuid::Uuid::new_v4().to_string(),
        verified,
        model_calls: if extra == 0 { 1 } else { 2 },
        reserved_output_tokens: 384 + extra,
        prompt_bytes: 100 + u32::from(extra > 0) * 100,
        tool_calls: 0,
        elapsed_ms: 10 + u64::from(extra > 0) * 10,
    }
}
fn study(id: &str, sha: &str, wins: usize, losses: usize, extra: u32) -> Study {
    Study {
        workflow_id: id.into(),
        workflow_version: "1.0.0".into(),
        workflow_sha256: sha.repeat(64),
        pairs: (0..4)
            .map(|i| Pair {
                case_sha256: crate::store::sha256(format!("{id}-{i}").as_bytes()),
                baseline: measurement(i >= wins, 0),
                expert: measurement(i < wins || i >= wins + losses, extra),
            })
            .collect(),
    }
}
fn thresholds() -> Thresholds {
    Thresholds {
        format_version: 1,
        min_pairs: 4,
        min_net_successes: 2,
        min_net_gain_per_mille: 500,
        max_extra_output_tokens_per_net_success: 384,
        main_solver_reserve_ms: 100,
    }
}
fn evidence() -> Evidence {
    Evidence {
        format_version: 1,
        mode: "deep".into(),
        criterion: "integer_sort/v1".into(),
        configuration_sha256: "a".repeat(64),
        role_prompts_sha256: "b".repeat(64),
        runtime_sources_sha256: "c".repeat(64),
        pinned_evidence_sha256: "e".repeat(64),
        dataset_sha256: "d".repeat(64),
        source: "Synthetic regression fixture; not a live study".into(),
        license: "CC0-1.0".into(),
        reviewer: "fixture.reviewer".into(),
        held_out_operator_attested: true,
        independent_outcomes_operator_attested: true,
        includes_failed_attempts_operator_attested: true,
        studies: vec![study("sort.one", "1", 3, 1, 192)],
    }
}
fn request() -> WorkflowRequest {
    WorkflowRequest {
        mode: "deep".into(),
        criterion: Some("integer_sort/v1".into()),
        configuration_sha256: "a".repeat(64),
        role_prompts_sha256: "b".repeat(64),
        runtime_sources_sha256: "c".repeat(64),
        evidence_sha256: "e".repeat(64),
        remaining_calls: 2,
        remaining_output_tokens: 576,
        remaining_prompt_bytes: 200,
        remaining_tool_calls: 1,
        remaining_ms: 110,
        main_prompt_bytes: 100,
        candidates: vec![WorkflowCandidate {
            id: "sort.one".into(),
            version: "1.0.0".into(),
            snapshot_sha256: "1".repeat(64),
            required_calls: 1,
            required_output_tokens: 192,
        }],
    }
}
fn policy(evidence: &Evidence, t: Thresholds) -> EvidenceRouter {
    EvidenceRouter::from_json(&serde_json::to_vec(evidence).unwrap(), t).unwrap()
}
#[test]
fn all_failures_and_regressions_count_in_exact_net_benefit_and_cost_edges() {
    let data = evidence();
    let router = policy(&data, thresholds());
    let summary = &router.summaries[0];
    assert_eq!(
        (
            summary.wins,
            summary.regressions,
            summary.net(),
            summary.extra_output_tokens
        ),
        (3, 1, 2, 768)
    );
    assert_eq!(router.rank_workflows(&request()).unwrap(), vec!["sort.one"]);
    for variant in 0..4 {
        let mut t = thresholds();
        match variant {
            0 => t.min_pairs = 5,
            1 => t.min_net_successes = 3,
            2 => t.min_net_gain_per_mille = 501,
            _ => t.max_extra_output_tokens_per_net_success = 383,
        };
        assert!(policy(&data, t)
            .rank_workflows(&request())
            .unwrap()
            .is_empty());
    }
    let mut negative = evidence();
    negative.studies[0] = study("sort.one", "1", 1, 3, 192);
    assert!(policy(&negative, thresholds())
        .rank_workflows(&request())
        .unwrap()
        .is_empty());
}
#[test]
fn remaining_original_budgets_and_main_solver_reserve_are_never_enlarged() {
    let router = policy(&evidence(), thresholds());
    for variant in 0..5 {
        let mut r = request();
        match variant {
            0 => r.remaining_calls = 1,
            1 => r.remaining_output_tokens = 575,
            2 => r.remaining_prompt_bytes = 199,
            3 => r.remaining_tool_calls = 0,
            _ => r.remaining_ms = 109,
        };
        assert!(
            router.rank_workflows(&r).unwrap().is_empty(),
            "edge {variant}"
        );
    }
    let mut r = request();
    r.candidates[0].required_calls = 2;
    r.candidates[0].required_output_tokens = 384;
    assert!(router.rank_workflows(&r).unwrap().is_empty());
}
#[test]
fn scope_or_pinned_version_drift_declines_instead_of_using_unmatched_evidence() {
    let router = policy(&evidence(), thresholds());
    for variant in 0..8 {
        let mut r = request();
        match variant {
            0 => r.mode = "research".into(),
            1 => r.criterion = None,
            2 => r.configuration_sha256 = "f".repeat(64),
            3 => r.role_prompts_sha256 = "f".repeat(64),
            4 => r.runtime_sources_sha256 = "f".repeat(64),
            5 => r.evidence_sha256 = "f".repeat(64),
            6 => r.candidates[0].version = "2.0.0".into(),
            _ => r.candidates[0].snapshot_sha256 = "2".repeat(64),
        };
        assert!(router.rank_workflows(&r).unwrap().is_empty());
    }
}
#[test]
fn benefit_per_conservative_cost_ranking_is_order_independent_and_identity_is_immutable() {
    let mut data = evidence();
    data.studies.push(study("sort.two", "2", 3, 0, 384));
    let mut t = thresholds();
    t.max_extra_output_tokens_per_net_success = 1000;
    let router = policy(&data, t.clone());
    let mut r = request();
    r.remaining_calls = 10;
    r.remaining_output_tokens = 1600;
    r.remaining_prompt_bytes = 1000;
    r.candidates.push(WorkflowCandidate {
        id: "sort.two".into(),
        version: "1.0.0".into(),
        snapshot_sha256: "2".repeat(64),
        required_calls: 1,
        required_output_tokens: 192,
    });
    assert_eq!(
        router.rank_workflows(&r).unwrap(),
        vec!["sort.one", "sort.two"]
    );
    r.candidates.reverse();
    data.studies.reverse();
    for study in &mut data.studies {
        study.pairs.reverse();
    }
    let reordered = policy(&data, t.clone());
    assert_eq!(router.descriptor(), reordered.descriptor());
    assert_eq!(
        reordered.rank_workflows(&r).unwrap(),
        vec!["sort.one", "sort.two"]
    );
    t.min_pairs = 3;
    assert_ne!(router.descriptor(), policy(&data, t).descriptor());
}
#[test]
fn rejects_unreviewed_ambiguous_duplicate_or_impossible_measurements() {
    for variant in 0..9 {
        let mut data = evidence();
        match variant {
            0 => data.includes_failed_attempts_operator_attested = false,
            1 => data.independent_outcomes_operator_attested = false,
            2 => data.held_out_operator_attested = false,
            3 => {
                data.studies[0].pairs[0].expert.task_id =
                    data.studies[0].pairs[0].baseline.task_id.clone()
            }
            4 => {
                let pair = data.studies[0].pairs[0].clone();
                data.studies[0].pairs.push(pair);
            }
            5 => data.studies.push(data.studies[0].clone()),
            6 => data.studies[0].pairs[0].expert.model_calls = 65,
            7 => data.criterion = "model-confidence".into(),
            _ => data.dataset_sha256 = "unscoped".into(),
        };
        assert!(
            EvidenceRouter::from_json(&serde_json::to_vec(&data).unwrap(), thresholds()).is_err()
        );
    }
    assert!(
        EvidenceRouter::from_json(br#"{"format_version":1,"format_version":1}"#, thresholds())
            .is_err()
    );
    assert!(crate::security::authorize("jev-plus", "evaluation.routing").is_err());
}
