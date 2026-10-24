use super::*;
use crate::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::{Engine, ExecutionProfile},
    store::Store,
};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

fn thresholds() -> Thresholds {
    Thresholds {
        min_observations: 8,
        min_success_basis_points: 7500,
        max_output_tokens_per_success: 512,
        max_prompt_bytes_per_success: 50_000,
        max_milliseconds_per_success: 160,
    }
}
fn study(cfg: &Config) -> RetryStudy {
    RetryStudy {
        format_version: 1,
        mode: "deep".into(),
        criterion: "integer_sort/v1".into(),
        cycle: 1,
        configuration_sha256: configuration_fingerprint(cfg).unwrap(),
        role_prompts_sha256: crate::backend::runtime_role_prompts_sha256().unwrap(),
        runtime_sources_sha256: runtime_sources_fingerprint().into(),
        dataset_sha256: "a".repeat(64),
        source: "Synthetic policy regression fixtures, not a quality study".into(),
        license: "CC0-1.0".into(),
        reviewer: "fixture.operator".into(),
        held_out_operator_attested: true,
        independent_outcomes_operator_attested: true,
        includes_failed_attempts_operator_attested: true,
        observations: (0..8)
            .map(|i| Observation {
                task_id: uuid::Uuid::new_v4().to_string(),
                verified: i < 6,
                model_calls: 1,
                reserved_output_tokens: 384,
                tool_calls: u32::from(i < 7),
                prompt_bytes: 32_000,
                elapsed_ms: 120,
            })
            .collect(),
    }
}
fn policy(s: &RetryStudy, t: Thresholds) -> EmpiricalPolicy {
    EmpiricalPolicy::from_json(&serde_json::to_vec(s).unwrap(), t).unwrap()
}
fn request(s: &RetryStudy) -> Request {
    Request {
        reason: Reason::IndependentRejection,
        mode: s.mode.clone(),
        cycle: s.cycle,
        candidate_sha256: "b".repeat(64),
        verification_criterion: Some(s.criterion.clone()),
        configuration_sha256: s.configuration_sha256.clone(),
        role_prompts_sha256: s.role_prompts_sha256.clone(),
        runtime_sources_sha256: s.runtime_sources_sha256.clone(),
        remaining_calls: 3,
        remaining_output_tokens: 1500,
        remaining_prompt_bytes: 100_000,
        remaining_ms: 1000,
        remaining_tool_calls: 2,
        required_tool_calls: 1,
        required_calls: 1,
        required_output_tokens: 384,
        required_prompt_bytes: 1000,
    }
}

#[test]
fn evaluator_counts_failures_and_exact_threshold_edges() {
    let s = study(&Config::default());
    let p = policy(&s, thresholds());
    assert_eq!(p.summary().observations, 8);
    assert_eq!(p.summary().verified_retries, 6);
    assert_eq!(p.summary().observed_success_basis_points, 7500);
    assert_eq!(p.summary().total_reserved_output_tokens, 3072);
    assert_eq!(p.summary().total_tool_calls, 7);
    assert_eq!(p.summary().total_elapsed_ms, 960);
    assert!(!p.summary().calibrated_probability);
    assert!(p.decide(&request(&s)).unwrap().allow);
    for field in 0..5 {
        let mut t = thresholds();
        match field {
            0 => t.min_observations = 9,
            1 => t.min_success_basis_points = 7501,
            2 => t.max_output_tokens_per_success = 511,
            3 => t.max_prompt_bytes_per_success = 42_666,
            _ => t.max_milliseconds_per_success = 159,
        }
        assert!(!policy(&s, t).decide(&request(&s)).unwrap().allow);
    }
}
#[test]
fn unknown_scope_and_unverified_advice_never_use_empirical_retry_evidence() {
    let s = study(&Config::default());
    let p = policy(&s, thresholds());
    for field in 0..8 {
        let mut r = request(&s);
        match field {
            0 => r.reason = Reason::UnverifiedAnswer,
            1 => r.mode = "research".into(),
            2 => r.cycle = 2,
            3 => r.configuration_sha256 = "c".repeat(64),
            4 => r.role_prompts_sha256 = "d".repeat(64),
            5 => r.verification_criterion = Some("shortest_path/v1".into()),
            6 => r.runtime_sources_sha256 = "e".repeat(64),
            _ => r.verification_criterion = None,
        }
        assert!(!p.decide(&r).unwrap().allow);
    }
}
#[test]
fn empirical_policy_cannot_buy_unsupported_or_unaffordable_work() {
    let s = study(&Config::default());
    let p = policy(&s, thresholds());
    for field in 0..9 {
        let mut r = request(&s);
        match field {
            0 => r.required_calls = 3,
            1 => r.required_output_tokens = 128,
            2 => r.required_tool_calls = 0,
            3 => r.remaining_calls = 0,
            4 => r.remaining_output_tokens = 383,
            5 => r.remaining_prompt_bytes = 999,
            6 => r.remaining_tool_calls = 0,
            7 => r.required_prompt_bytes = 32_001,
            _ => r.remaining_ms = 119,
        }
        assert!(!p.decide(&r).unwrap().allow);
    }
    let mut r = request(&s);
    r.remaining_ms = 120;
    assert!(p.decide(&r).unwrap().allow);
}
#[test]
fn no_verified_improvements_never_admit_and_costs_include_timeouts() {
    let mut s = study(&Config::default());
    for r in &mut s.observations {
        r.verified = false;
        r.tool_calls = 0;
    }
    let p = policy(&s, thresholds());
    assert_eq!(p.summary().total_model_calls, 8);
    assert_eq!(p.summary().total_reserved_output_tokens, 3072);
    assert!(!p.decide(&request(&s)).unwrap().allow);
    s.observations[0].elapsed_ms = 3_600_000;
    assert!(!policy(&s, thresholds()).decide(&request(&s)).unwrap().allow);
}
#[test]
fn policy_identity_pins_all_study_content_and_operator_thresholds() {
    let mut s = study(&Config::default());
    let original = policy(&s, thresholds());
    assert_eq!(original.descriptor(), policy(&s, thresholds()).descriptor());
    let mut t = thresholds();
    t.min_observations = 9;
    assert_ne!(original.descriptor(), policy(&s, t).descriptor());
    s.observations[7].elapsed_ms += 1;
    assert_ne!(original.descriptor(), policy(&s, thresholds()).descriptor());
    let home_only = Config {
        home: "/tmp/irrelevant-home".into(),
        ..Config::default()
    };
    assert_eq!(
        configuration_fingerprint(&home_only).unwrap(),
        configuration_fingerprint(&Config::default()).unwrap()
    );
    let changed = Config {
        temperature: 0.2,
        ..Config::default()
    };
    assert_ne!(
        configuration_fingerprint(&changed).unwrap(),
        configuration_fingerprint(&Config::default()).unwrap()
    );
}
#[test]
fn invalid_studies_thresholds_and_ambiguous_json_fail_closed() {
    let s = study(&Config::default());
    let value = serde_json::to_value(&s).unwrap();
    for (field, v) in [
        ("format_version", serde_json::json!(2)),
        ("held_out_operator_attested", serde_json::json!(false)),
        (
            "independent_outcomes_operator_attested",
            serde_json::json!(false),
        ),
        (
            "includes_failed_attempts_operator_attested",
            serde_json::json!(false),
        ),
        ("criterion", serde_json::json!("self_reported_confidence")),
        ("cycle", serde_json::json!(0)),
        ("mode", serde_json::json!("fast")),
        ("configuration_sha256", serde_json::json!("A".repeat(64))),
        ("reviewer", serde_json::json!("bad/reviewer")),
        ("source", serde_json::json!("")),
        ("extra", serde_json::json!("ignored")),
    ] {
        let mut bad = value.clone();
        bad[field] = v;
        assert!(
            EmpiricalPolicy::from_json(&serde_json::to_vec(&bad).unwrap(), thresholds()).is_err(),
            "Accepted {field}"
        );
    }
    for field in 0..7 {
        let mut bad = value.clone();
        match field {
            0 => bad["observations"][1]["task_id"] = bad["observations"][0]["task_id"].clone(),
            1 => bad["observations"][0]["tool_calls"] = serde_json::json!(0),
            2 => bad["observations"][0]["model_calls"] = serde_json::json!(2),
            3 => bad["observations"][0]["reserved_output_tokens"] = serde_json::json!(383),
            4 => bad["observations"][0]["prompt_bytes"] = serde_json::json!(32_001),
            5 => bad["observations"] = serde_json::json!([]),
            _ => bad["observations"][0]["elapsed_ms"] = serde_json::json!(3_600_001u64),
        }
        assert!(
            EmpiricalPolicy::from_json(&serde_json::to_vec(&bad).unwrap(), thresholds()).is_err()
        );
    }
    let raw = serde_json::to_string(&s).unwrap();
    let duplicate = raw.replacen(
        "\"format_version\":1",
        "\"format_version\":1,\"format_version\":1",
        1,
    );
    assert!(EmpiricalPolicy::from_json(duplicate.as_bytes(), thresholds()).is_err());
    let duplicate = raw.replacen(
        "\"verified\":true",
        "\"verified\":false,\"verified\":true",
        1,
    );
    assert!(EmpiricalPolicy::from_json(duplicate.as_bytes(), thresholds()).is_err());
    assert!(EmpiricalPolicy::from_json(&vec![b' '; MAX_STUDY_BYTES + 1], thresholds()).is_err());
    let mut t = thresholds();
    t.max_milliseconds_per_success = u64::MAX;
    assert!(EmpiricalPolicy::from_json(raw.as_bytes(), t).is_err());
}

struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("Extra inference must not be bought");
        assert_eq!(expected, role);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture/v1".into(),
                input_tokens: Some(10),
                output_tokens: Some(10),
                elapsed_ms: 1.0,
                queue_wait_ms: None,
                provider_metrics: None,
            })
        })
    }
}
#[tokio::test]
async fn measured_retry_runs_through_engine_and_preserves_independent_verification() {
    for allow in [false, true] {
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            max_tokens: 3000,
            ..Config::default()
        };
        let store = Arc::new(Store::in_memory().unwrap());
        let s = study(&cfg);
        let mut t = thresholds();
        if !allow {
            t.min_success_basis_points = 7501;
        }
        let policy = Arc::new(policy(&s, t));
        let descriptor = policy.descriptor();
        let script = if allow {
            vec![
                ("planner", "invalid plan"),
                ("solver", "[3,2,1]"),
                ("solver", "[1,2,3]"),
            ]
        } else {
            vec![("planner", "invalid plan"), ("solver", "[3,2,1]")]
        };
        let engine = Engine::with_backend(
            cfg,
            store.clone(),
            Arc::new(Script(Mutex::new(script.into()))),
        )
        .with_escalation_policy(policy)
        .unwrap();
        let outcome = engine
            .run_profile(
                r#"sort:{"values":[3,1,2]}"#,
                "deep",
                ExecutionProfile::Collective,
            )
            .await
            .unwrap();
        assert_eq!(outcome.calls, if allow { 3 } else { 2 });
        assert_eq!(outcome.status, if allow { "completed" } else { "rejected" });
        assert_eq!(
            outcome.stop_reason,
            if allow {
                "verified"
            } else {
                "escalation_declined"
            }
        );
        let trace = store.task(&outcome.task_id).unwrap();
        let events = trace["events"].as_array().unwrap();
        let intent = events
            .iter()
            .find(|e| e["kind"] == "escalation_intent")
            .unwrap();
        assert_eq!(
            intent["payload"]["request"]["verification_criterion"],
            "integer_sort/v1"
        );
        assert_eq!(
            intent["payload"]["request"]["configuration_sha256"],
            s.configuration_sha256
        );
        assert_eq!(
            intent["payload"]["policy"],
            serde_json::to_value(&descriptor).unwrap()
        );
        assert_eq!(outcome.resources.tool_calls, if allow { 2 } else { 1 });
    }
}

#[test]
fn sample_ceiling_and_zero_duration_do_not_bypass_prompt_or_time_admission() {
    let mut s = study(&Config::default());
    s.observations = (0..MAX_OBSERVATIONS)
        .map(|_| Observation {
            task_id: uuid::Uuid::new_v4().to_string(),
            verified: true,
            model_calls: 1,
            reserved_output_tokens: 384,
            tool_calls: 1,
            prompt_bytes: 1000,
            elapsed_ms: 0,
        })
        .collect();
    let t = Thresholds {
        min_observations: MAX_OBSERVATIONS as u32,
        min_success_basis_points: 10_000,
        max_output_tokens_per_success: 384,
        max_prompt_bytes_per_success: 1000,
        max_milliseconds_per_success: 1,
    };
    let p = policy(&s, t.clone());
    assert_eq!(p.summary().observations, MAX_OBSERVATIONS as u32);
    let mut r = request(&s);
    r.remaining_ms = 1;
    assert!(p.decide(&r).unwrap().allow);
    r.required_prompt_bytes = 1001;
    assert!(!p.decide(&r).unwrap().allow);
    r.required_prompt_bytes = 1000;
    r.remaining_ms = 0;
    assert!(!p.decide(&r).unwrap().allow);
    s.observations.push(Observation {
        task_id: uuid::Uuid::new_v4().to_string(),
        verified: true,
        model_calls: 1,
        reserved_output_tokens: 384,
        tool_calls: 1,
        prompt_bytes: 1000,
        elapsed_ms: 0,
    });
    let bytes = serde_json::to_vec(&s).unwrap();
    assert!(bytes.len() < MAX_STUDY_BYTES);
    assert!(EmpiricalPolicy::from_json(&bytes, t).is_err());
}
