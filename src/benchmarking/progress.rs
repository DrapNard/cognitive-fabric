//! Controlled public ablation of independently scored structured progress stopping.
//!
//! Both arms execute the same Jev+ collective profile with one Engine, backend,
//! frozen corpus, and configured resource pool. The treatment changes only the
//! opt-in Jev-R progress setting.

use crate::{
    config::Config,
    engine::{Engine, ExecutionProfile, Outcome},
    reasoning::progress::Settings,
    security,
    store::{self, Store},
};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path, sync::Arc, time::Duration};

const MAX_DATASET_BYTES: usize = 2_000_000;
const MAX_CASES: usize = 200;
const CRITERIA: &[&str] = &["finite_domain/v1", "shortest_path/v1"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    verification: String,
}

#[derive(Clone, Default, serde::Serialize)]
struct Resources {
    executed: usize,
    passed: usize,
    failed_tasks: usize,
    model_calls: u64,
    cycles: u64,
    tool_calls: u64,
    prompt_bytes: u64,
    reserved_output_tokens: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    latency_ms: f64,
}

impl Resources {
    fn add(&mut self, outcome: &Outcome, passed: bool) {
        let first = self.executed == 0;
        self.executed += 1;
        self.passed += usize::from(passed);
        self.failed_tasks += usize::from(matches!(
            outcome.status.as_str(),
            "failed" | "rejected" | "cancelled" | "exhausted"
        ));
        self.model_calls += u64::from(outcome.calls);
        self.cycles += u64::from(outcome.cycles);
        self.tool_calls += u64::from(outcome.resources.tool_calls);
        self.prompt_bytes += outcome.resources.prompt_bytes as u64;
        self.reserved_output_tokens += outcome.resources.reserved_output_tokens as u64;
        self.latency_ms += outcome.elapsed_ms;
        self.input_tokens = sum_optional(self.input_tokens, outcome.resources.input_tokens, first);
        self.output_tokens =
            sum_optional(self.output_tokens, outcome.resources.output_tokens, first);
    }

    fn cost_view(&self) -> Value {
        json!({
            "model_calls": self.model_calls,
            "cycles": self.cycles,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "reserved_output_tokens": self.reserved_output_tokens,
            "prompt_bytes": self.prompt_bytes,
            "tool_calls": self.tool_calls,
            "latency_ms": self.latency_ms
        })
    }
}

fn sum_optional(current: Option<u64>, next: Option<u64>, first: bool) -> Option<u64> {
    if first {
        next
    } else {
        current
            .zip(next)
            .map(|(left, right)| left.saturating_add(right))
    }
}

fn difference(baseline: Option<u64>, treatment: Option<u64>) -> Option<i128> {
    baseline
        .zip(treatment)
        .map(|(baseline, treatment)| i128::from(treatment) - i128::from(baseline))
}

fn delta(baseline: &Resources, treatment: &Resources) -> Value {
    json!({
        "model_calls": i128::from(treatment.model_calls) - i128::from(baseline.model_calls),
        "cycles": i128::from(treatment.cycles) - i128::from(baseline.cycles),
        "input_tokens": difference(baseline.input_tokens, treatment.input_tokens),
        "output_tokens": difference(baseline.output_tokens, treatment.output_tokens),
        "reserved_output_tokens": i128::from(treatment.reserved_output_tokens) - i128::from(baseline.reserved_output_tokens),
        "prompt_bytes": i128::from(treatment.prompt_bytes) - i128::from(baseline.prompt_bytes),
        "tool_calls": i128::from(treatment.tool_calls) - i128::from(baseline.tool_calls),
        "latency_ms": treatment.latency_ms - baseline.latency_ms
    })
}

fn validate_review(review: &crate::benchmarking::Review) -> Result<()> {
    security::validate_provenance(&review.source, &review.license)?;
    if review.held_out == review.workflow_check
        || review.code_revision.len() != 40
        || !review
            .code_revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !(1..=86_400).contains(&review.max_run_seconds)
    {
        bail!("Comparison requires exactly one of held-out review or workflow check, a full code revision and a bounded run duration")
    }
    Ok(())
}

fn read_cases(dataset: &Path, review: &crate::benchmarking::Review) -> Result<(Vec<Case>, String)> {
    let bytes = security::read_regular_bytes(dataset, MAX_DATASET_BYTES)?;
    let checksum = store::sha256(&bytes);
    if let Some(expected) = &review.expected_dataset_sha256 {
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("Reviewed dataset checksum must be 64 lowercase hexadecimal digits")
        }
        if expected != &checksum {
            bail!("Dataset differs from the operator-pinned reviewed checksum")
        }
    }
    let raw = std::str::from_utf8(&bytes)?;
    let mut cases = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        if cases.len() == MAX_CASES {
            bail!("Progress comparison requires 1-200 bounded cases")
        }
        let value = crate::strict_json::value(line.as_bytes(), MAX_DATASET_BYTES)?;
        cases.push(serde_json::from_value::<Case>(value)?);
    }
    if cases.is_empty()
        || cases
            .iter()
            .any(|case| case.input.trim().is_empty() || case.input.len() > 12_000)
    {
        bail!("Progress comparison requires 1-200 bounded cases")
    }
    let mut seen = BTreeSet::new();
    for case in &cases {
        let (prefix, expected) = match case.verification.as_str() {
            "finite_domain_v1" => ("csp:", "finite_domain/v1"),
            "shortest_path_v1" => ("path:", "shortest_path/v1"),
            _ => bail!(
                "Progress comparison requires finite_domain_v1 or shortest_path_v1 verification"
            ),
        };
        if !case.input.starts_with(prefix) || !seen.insert(case.input.clone()) {
            bail!("Progress comparison requires unique explicit structured objectives")
        }
        let criterion = crate::verification::Criterion::from_query(&case.input)?
            .ok_or_else(|| anyhow::anyhow!("Progress comparison requires structured objectives"))?;
        if criterion.name() != expected {
            bail!("Progress comparison criterion differs from declared verification")
        }
    }
    Ok((cases, checksum))
}

fn check(answer: &str, query: &str, status: &str) -> bool {
    if !matches!(status, "completed" | "unverified") {
        return false;
    }
    crate::verification::Criterion::from_query(query)
        .ok()
        .flatten()
        .is_some_and(|criterion| {
            CRITERIA.contains(&criterion.name()) && criterion.check(answer).is_ok()
        })
}

fn bounded_answer(answer: &str) -> (Option<String>, bool) {
    const MAX_REPORTED_ANSWER_BYTES: usize = 96_000;
    if answer.len() <= MAX_REPORTED_ANSWER_BYTES {
        return (Some(answer.to_owned()), false);
    }
    let mut end = MAX_REPORTED_ANSWER_BYTES;
    while !answer.is_char_boundary(end) {
        end -= 1;
    }
    (Some(answer[..end].to_owned()), true)
}

/// Compare the same Jev+ Collective workflow with structured Jev-R progress
/// stopping disabled and enabled. The independently compiled original criterion scores
/// every returned answer; the heuristic never certifies unsatisfiability.
pub async fn compare_progress_until<F>(
    cfg: &Config,
    store: Arc<Store>,
    dataset: &Path,
    mode: &str,
    settings: Settings,
    review: &crate::benchmarking::Review,
    cancel: F,
) -> Result<Value>
where
    F: std::future::Future<Output = ()>,
{
    validate_review(review)?;
    settings.validate()?;
    if !matches!(mode, "deep" | "research") {
        bail!("Progress comparison requires deep or research mode")
    }
    if cfg.advice_guard.is_some() {
        bail!("Progress comparison refuses enabled adaptive advice history")
    }
    if cfg.finite_search.is_some() {
        bail!("Progress comparison refuses enabled pre-model finite search")
    }
    cfg.validate()?;
    let (cases, dataset_checksum) = read_cases(dataset, review)?;

    // Keep every effective setting fixed except the progress policy. Clearing
    // these existing comparison confounds matches the standard benchmark.
    let mut common = cfg.clone();
    common.role_profiles.clear();
    common.synthesize_results = false;
    common.advice_guard = None;
    common.progress_stopping = None;
    common.validate()?;
    let baseline_fingerprint = crate::escalation::empirical::configuration_fingerprint(&common)?;
    let mut treatment_config = common.clone();
    treatment_config.progress_stopping = Some(settings.clone());
    treatment_config.validate()?;
    let treatment_fingerprint =
        crate::escalation::empirical::configuration_fingerprint(&treatment_config)?;

    let started = tokio::time::Instant::now();
    let deadline = started + Duration::from_secs(review.max_run_seconds);
    let corpus = Arc::new(store.freeze_corpus()?);
    let snapshot_ms = started.elapsed().as_secs_f64() * 1000.0;

    // One Engine retains a single configured resource pool and its reservation
    // counters across both sequential arms. Its backend and frozen evidence view
    // are also identical; only the Rust task configuration changes at each arm.
    let mut engine = Engine::new(common.clone(), store.clone()).with_frozen_corpus(corpus.clone());
    let mut arm_resources = [Resources::default(), Resources::default()];
    let mut outcomes: Vec<[Option<Value>; 2]> = vec![[None, None]; cases.len()];
    let mut paired_results: Vec<[Option<bool>; 2]> = vec![[None, None]; cases.len()];
    let mut paired_resources = [Resources::default(), Resources::default()];
    let mut paired_count = 0usize;
    let mut unmatched_cases = Vec::new();
    let mut stopped = false;
    let mut cancelled = false;
    let mut deadline_reached = false;
    tokio::pin!(cancel);

    'cases: for (case_index, case) in cases.iter().enumerate() {
        for execution_position in 0..2 {
            // A ready callback cancels before this arm can create a task or
            // dispatch inference. Biased ordering makes that guarantee clear.
            tokio::select! {
                biased;
                _ = &mut cancel => {
                    cancelled = true;
                    stopped = true;
                    break 'cases;
                }
                _ = tokio::time::sleep_until(deadline) => {
                    stopped = true;
                    deadline_reached = true;
                    break 'cases;
                }
                _ = std::future::ready(()) => {}
            }
            if tokio::time::Instant::now() >= deadline {
                stopped = true;
                deadline_reached = true;
                break 'cases;
            }
            let arm_index = (case_index + execution_position) % 2;
            engine.cfg = if arm_index == 0 {
                common.clone()
            } else {
                treatment_config.clone()
            };
            let mut arm_cancelled = false;
            let mut arm_deadline = false;
            let outcome = engine
                .run_profile_until(&case.input, mode, ExecutionProfile::Collective, async {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => { arm_deadline = true; },
                        _ = &mut cancel => { arm_cancelled = true; },
                    }
                })
                .await?;
            let passed = check(&outcome.answer, &case.input, &outcome.status);
            arm_resources[arm_index].add(&outcome, passed);
            let (reported_answer, answer_truncated) = bounded_answer(&outcome.answer);
            let identities = crate::evaluation::model_metadata::read(&store, &outcome.task_id)?;
            let trace = json!({
                "arm_index": arm_index,
                "profile": "collective",
                "execution_position": execution_position,
                "task_id": outcome.task_id,
                "status": outcome.status,
                "stop_reason": outcome.stop_reason,
                "passed_independent_criterion_check": passed,
                "passed_independent_finite_domain_check": if case.verification == "finite_domain_v1" { Some(passed) } else { None },
                "answer_sha256": store::sha256(outcome.answer.as_bytes()),
                "answer": if review.workflow_check { reported_answer } else { None },
                "answer_truncated": review.workflow_check && answer_truncated,
                "reported_models": identities.reported_models,
                "primary_model_snapshot": identities.primary_model_snapshot,
                "resource_route_sha256": identities.resource_route_sha256,
                "resource_routes": identities.resource_routes,
                "calls": outcome.calls,
                "cycles": outcome.cycles,
                "resources": outcome.resources,
                "elapsed_ms": outcome.elapsed_ms
            });
            outcomes[case_index][arm_index] = Some(trace);
            let interrupted = outcome.status == "cancelled";
            let cancellation_triggered = arm_cancelled || (interrupted && !arm_deadline);
            let deadline_triggered =
                arm_deadline || (!arm_cancelled && tokio::time::Instant::now() >= deadline);
            if cancellation_triggered || deadline_triggered {
                cancelled = cancellation_triggered;
                deadline_reached = deadline_triggered;
                stopped = true;
                break 'cases;
            }
            paired_results[case_index][arm_index] = Some(passed);
        }

        let [Some(_), Some(_)] = &outcomes[case_index] else {
            continue;
        };
        paired_count += 1;
        for arm_index in 0..2 {
            let case_trace = outcomes[case_index][arm_index].as_ref().unwrap();
            let resources: crate::reasoning::Resources =
                serde_json::from_value(case_trace["resources"].clone())?;
            let outcome = Outcome {
                task_id: case_trace["task_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                answer: String::new(),
                status: case_trace["status"].as_str().unwrap_or_default().to_owned(),
                calls: case_trace["calls"].as_u64().unwrap_or_default() as u32,
                cycles: case_trace["cycles"].as_u64().unwrap_or_default() as u32,
                elapsed_ms: case_trace["elapsed_ms"].as_f64().unwrap_or_default(),
                evidence: Vec::new(),
                skills: Vec::new(),
                resources,
                stop_reason: case_trace["stop_reason"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            };
            let passed = paired_results[case_index][arm_index].unwrap_or(false);
            paired_resources[arm_index].add(&outcome, passed);
        }
    }

    for (case_index, pair) in outcomes.iter().enumerate() {
        if pair[0].is_none()
            || pair[1].is_none()
            || paired_results[case_index].iter().any(Option::is_none)
        {
            unmatched_cases.push(case_index);
        }
    }
    let mut both_passed = Vec::new();
    let mut both_failed = Vec::new();
    let mut treatment_wins = Vec::new();
    let mut regressions = Vec::new();
    for (index, pair) in paired_results.iter().enumerate() {
        match (pair[0], pair[1]) {
            (Some(true), Some(true)) => both_passed.push(index),
            (Some(false), Some(false)) => both_failed.push(index),
            (Some(false), Some(true)) => treatment_wins.push(index),
            (Some(true), Some(false)) => regressions.push(index),
            _ => {}
        }
    }
    let case_reports = cases
        .iter()
        .enumerate()
        .map(|(index, _)| json!({
            "case_index": index,
            "input": if review.workflow_check { Some(&cases[index].input) } else { None },
            "verification": cases[index].verification,
            "baseline": outcomes[index][0],
            "progress_stopping": outcomes[index][1],
            "paired": paired_results[index][0].is_some() && paired_results[index][1].is_some(),
            "baseline_passed": paired_results[index][0],
            "progress_stopping_passed": paired_results[index][1],
            "paired_resources": if paired_results[index][0].is_some() && paired_results[index][1].is_some() {
                json!({"baseline": outcomes[index][0].as_ref().unwrap()["resources"],
                    "progress_stopping": outcomes[index][1].as_ref().unwrap()["resources"]})
            } else { Value::Null }
        }))
        .collect::<Vec<_>>();

    let provenance = json!({
        "format_version": 1,
        "experiment": if cases.iter().all(|case| case.verification == "finite_domain_v1") {
            "finite_domain_progress_stopping/v1"
        } else { "structured_progress_stopping/v2" },
        "dataset_sha256": dataset_checksum,
        "dataset_source": review.source,
        "dataset_license": review.license,
        "dataset_scope": "caller-provided; may contain sensitive inputs",
        "content_included": review.workflow_check,
        "dataset_checksum_operator_pinned": review.expected_dataset_sha256.is_some(),
        "reviewed_dataset_checksum_operator_attested": if review.held_out { review.expected_dataset_sha256.as_ref() } else { None },
        "held_out_operator_attested": review.held_out,
        "run_purpose": if review.workflow_check { "workflow_check" } else { "held_out_comparison" },
        "code_revision_operator_attested": review.code_revision,
        "package_version": env!("CARGO_PKG_VERSION"),
        "mode": mode,
        "criterion": if cases.iter().all(|case| case.verification == "finite_domain_v1") {
            "finite_domain/v1"
        } else if cases.iter().all(|case| case.verification == "shortest_path_v1") {
            "shortest_path/v1"
        } else { "mixed-structured/v1" },
        "execution_profile": "collective",
        "configured_model": common.model
    });
    let controls = json!({
        "controls": {
            "same_execution_profile": true,
            "same_runtime_budgets": true,
            "same_backend_and_frozen_corpus": true,
            "role_profiles_cleared": true,
            "result_synthesis_disabled": true,
            "adaptive_advice_history_rejected": true,
            "case_start_arm_rotates": true
        },
        "runtime_sources_sha256": crate::escalation::empirical::runtime_sources_fingerprint(),
        "compiled_role_prompts_sha256": crate::backend::runtime_role_prompts_sha256()?,
        "baseline_configuration_sha256": baseline_fingerprint,
        "progress_configuration_sha256": treatment_fingerprint,
        "progress_settings": settings,
        "baseline_configuration": common,
        "progress_configuration": treatment_config,
        "progress_stopping_default_configuration_changed": false,
        "adaptive_advice_history_frozen": true,
        "corpus_snapshot": corpus.metadata,
        "snapshot_elapsed_ms": snapshot_ms,
        "configured_resource_pool": common.resource_admission.is_some(),
        "configured_resource_pool_shared_across_arms": common.resource_admission.as_ref().map(|_| true)
    });
    let completion = json!({
        "case_count": cases.len(),
        "complete_cases": paired_count,
        "complete": !stopped && paired_count == cases.len(),
        "stop_reason": if cancelled { "cancelled" } else if deadline_reached { "run_deadline" } else if stopped { "incomplete" } else { "completed" },
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0
    });
    let paired_outcomes = json!({
        "paired_cases": paired_count,
        "unmatched_cases": unmatched_cases,
        "progress_stopping_wins": treatment_wins,
        "progress_stopping_regressions": regressions,
        "both_passed": both_passed,
        "both_failed": both_failed,
        "observed_regressions_with_baseline_witness": regressions.iter().filter(|index| {
            outcomes[**index][0].as_ref().is_some_and(|trace| trace["passed_independent_criterion_check"] == true)
                && outcomes[**index][1].as_ref().is_some_and(|trace| trace["passed_independent_criterion_check"] == false)
        }).count()
    });
    let arms = json!([
        {"index":0,"name":"progress_disabled","profile":"collective","configuration":common,
            "executed":arm_resources[0].executed,"passed":arm_resources[0].passed,"resources":arm_resources[0]},
        {"index":1,"name":"progress_enabled","profile":"collective","configuration":treatment_config,
            "executed":arm_resources[1].executed,"passed":arm_resources[1].passed,"resources":arm_resources[1]}
    ]);
    let paired_resources = json!({
        "baseline": paired_resources[0].cost_view(),
        "progress_stopping": paired_resources[1].cost_view(),
        "delta_progress_minus_baseline": delta(&paired_resources[0], &paired_resources[1])
    });
    let limitations = json!([
        "Constraint satisfaction is the only scored quality outcome; the experiment does not test general reasoning.",
        "A failed case does not establish unsatisfiability or prove the search had no useful continuation.",
        "A baseline-only witness in a paired regression is observed, but the comparison cannot prove that progress stopping caused a later search to find it.",
        "Results are conditional on this exact corpus, configuration, runtime source, compiled prompts, provider state and execution order.",
        "A configured resource pool enforces its declared logical reservations only; it does not measure or cap physical device usage.",
        "Model labels and token counts are provider-reported and do not attest immutable weights or physical compute.",
        "Held-out independence, workflow scope and code revision are operator-attested."
    ]);
    let mut report = serde_json::Map::new();
    for fragment in [provenance, controls, completion] {
        let Value::Object(fields) = fragment else {
            bail!("Invalid comparison report fragment")
        };
        report.extend(fields);
    }
    report.insert("paired_outcomes".into(), paired_outcomes);
    report.insert("arms".into(), arms);
    report.insert("paired_resource_totals".into(), paired_resources);
    report.insert("cases".into(), json!(case_reports));
    report.insert("automatic_policy_installation".into(), json!(false));
    report.insert("candidate_promotion".into(), json!(false));
    report.insert("permissions_granted_by_experiment".into(), json!(false));
    report.insert(
        "unavailable_metrics".into(),
        json!([
            "gpu_memory",
            "energy",
            "physical_model_load_cost",
            "weight_identity",
            "general_security_failure_rate"
        ]),
    );
    report.insert("limitations".into(), limitations);
    Ok(Value::Object(report))
}

#[cfg(test)]
mod criterion_family_tests {
    use super::*;
    const PATH: &str = r#"path:{"nodes":["s","g"],"edges":[["s","g"]],"start":"s","goal":"g"}"#;
    const CSP: &str = r#"csp:{"variables":[{"id":"x","domain":[1]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]}"#;

    fn review() -> crate::benchmarking::Review {
        crate::benchmarking::Review {
            source: "Public synthetic regression fixture".into(),
            license: "CC0-1.0".into(),
            code_revision: "fixture".into(),
            held_out: false,
            workflow_check: true,
            expected_dataset_sha256: None,
            max_run_seconds: 30,
        }
    }

    fn read(rows: &[Value]) -> Result<(Vec<Case>, String)> {
        let path = std::env::temp_dir().join(format!(
            "path-progress-corpus-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        let data = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, data).unwrap();
        let result = read_cases(&path, &review());
        std::fs::remove_file(path).unwrap();
        result
    }

    #[test]
    fn comparisons_accept_exact_path_and_mixed_structured_criteria() {
        let path = json!({"input":PATH,"verification":"shortest_path_v1"});
        let finite = json!({"input":CSP,"verification":"finite_domain_v1"});
        assert_eq!(read(std::slice::from_ref(&path)).unwrap().0.len(), 1);
        assert_eq!(read(&[finite, path]).unwrap().0.len(), 2);
        assert!(check(r#"["s","g"]"#, PATH, "completed"));
        assert!(!check(r#"["s"]"#, PATH, "completed"));
        assert!(!check(r#"["s","g"]"#, PATH, "failed"));
    }

    #[test]
    fn comparisons_reject_mismatched_labels_duplicates_and_unsupported_criteria() {
        for rows in [
            vec![json!({"input":PATH,"verification":"finite_domain_v1"})],
            vec![json!({"input":CSP,"verification":"shortest_path_v1"})],
            vec![json!({"input":PATH,"verification":"expected_output_exact_match"})],
            vec![json!({"input":PATH,"verification":"shortest_path_v1"}); 2],
            vec![json!({"input":"path:{}","verification":"shortest_path_v1"})],
        ] {
            assert!(read(&rows).is_err());
        }
    }
}
