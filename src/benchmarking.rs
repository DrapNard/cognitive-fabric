//! Controlled runtime ablations with independently supplied expected outputs.
use crate::{
    config::Config,
    engine::{Engine, ExecutionProfile},
    security,
    store::{self, Store},
};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    expected: Option<String>,
    #[serde(default)]
    verification: Criterion,
}

#[derive(Clone, Copy, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Criterion {
    #[default]
    ExpectedOutputExactMatch,
    DependencyOrderV1,
    SqlResultJsonV1,
}

#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct SqlResult {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}
impl SqlResult {
    fn parse(text: &str) -> Result<Self> {
        if text.len() > 16384 {
            bail!("SQL expected/result JSON exceeds bounds")
        }
        let result: Self = serde_json::from_str(text)?;
        if result.columns.is_empty()
            || result.columns.len() > 16
            || result.columns.iter().any(|label| label.len() > 128)
            || result.rows.len() > 64
            || result.rows.iter().any(|row| {
                row.len() != result.columns.len()
                    || row.iter().any(|cell| {
                        !matches!(cell, Value::Null | Value::Number(_) | Value::String(_))
                    })
            })
        {
            bail!("Invalid SQL expected/result shape")
        }
        Ok(result)
    }
}

impl Case {
    fn prepare(&self) -> Result<Option<crate::verification::Ordering>> {
        match self.verification {
            Criterion::ExpectedOutputExactMatch => {
                if self
                    .expected
                    .as_ref()
                    .is_none_or(|value| value.trim().is_empty() || value.len() > 16_000)
                {
                    bail!("Exact-match case requires a bounded expected output")
                }
                Ok(None)
            }
            Criterion::SqlResultJsonV1 => {
                if !self.input.starts_with("sql:") {
                    bail!("SQL result criterion requires an explicit sql: objective")
                }
                SqlResult::parse(self.expected.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("SQL result criterion requires expected JSON")
                })?)?;
                Ok(None)
            }
            Criterion::DependencyOrderV1 => {
                if self.expected.is_some() {
                    bail!("Dependency-order case must omit expected output")
                }
                let criterion = crate::verification::Ordering::from_query(&self.input)?;
                if criterion.is_none() {
                    bail!("Dependency-order case requires an explicit order: objective")
                }
                Ok(criterion)
            }
        }
    }

    fn score(
        &self,
        criterion: Option<&crate::verification::Ordering>,
        status: &str,
        answer: &str,
    ) -> bool {
        if !matches!(status, "completed" | "unverified") {
            return false;
        }
        if matches!(self.verification, Criterion::SqlResultJsonV1) {
            return self
                .expected
                .as_deref()
                .and_then(|text| SqlResult::parse(text).ok())
                .zip(SqlResult::parse(answer).ok())
                .is_some_and(|(expected, actual)| expected == actual);
        }
        match criterion {
            Some(criterion) => criterion.check(answer).is_ok(),
            None => self
                .expected
                .as_ref()
                .is_some_and(|expected| normalize(answer) == normalize(expected)),
        }
    }
}

pub struct Review {
    pub source: String,
    pub license: String,
    pub code_revision: String,
    pub held_out: bool,
    pub expected_dataset_sha256: Option<String>,
    pub max_run_seconds: u64,
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Clone, Default, serde::Serialize)]
struct Totals {
    model_calls: u64,
    tool_calls: u64,
    latency_ms: f64,
    inference_ms: f64,
    queue_wait_ms: Option<f64>,
    max_sampled_process_rss_bytes: Option<u64>,
    process_lifetime_peak_rss_bytes: Option<u64>,
    prompt_bytes: usize,
    reserved_output_tokens: usize,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    failed_tasks: usize,
}

impl Totals {
    fn add(&mut self, outcome: &crate::engine::Outcome, first: bool) {
        self.model_calls += outcome.calls as u64;
        self.tool_calls += outcome.resources.tool_calls as u64;
        self.latency_ms += outcome.elapsed_ms;
        self.inference_ms += outcome.resources.inference_ms;
        self.max_sampled_process_rss_bytes = if first {
            outcome.resources.process_rss_bytes
        } else {
            self.max_sampled_process_rss_bytes
                .zip(outcome.resources.process_rss_bytes)
                .map(|(left, right)| left.max(right))
        };
        self.process_lifetime_peak_rss_bytes = if first {
            outcome.resources.process_lifetime_peak_rss_bytes
        } else {
            self.process_lifetime_peak_rss_bytes
                .zip(outcome.resources.process_lifetime_peak_rss_bytes)
                .map(|(left, right)| left.max(right))
        };
        self.queue_wait_ms = if first {
            outcome.resources.queue_wait_ms
        } else {
            self.queue_wait_ms
                .zip(outcome.resources.queue_wait_ms)
                .map(|(left, right)| left + right)
        };
        self.prompt_bytes += outcome.resources.prompt_bytes;
        self.reserved_output_tokens += outcome.resources.reserved_output_tokens;
        self.failed_tasks += usize::from(matches!(
            outcome.status.as_str(),
            "failed" | "rejected" | "cancelled" | "exhausted"
        ));
        self.input_tokens = if first {
            outcome.resources.input_tokens
        } else {
            self.input_tokens
                .zip(outcome.resources.input_tokens)
                .map(|(left, right)| left.saturating_add(right))
        };
        self.output_tokens = if first {
            outcome.resources.output_tokens
        } else {
            self.output_tokens
                .zip(outcome.resources.output_tokens)
                .map(|(left, right)| left.saturating_add(right))
        };
    }
}

pub async fn compare(
    cfg: &Config,
    store: Arc<Store>,
    dataset: &Path,
    mode: &str,
    larger: Option<&str>,
    review: &Review,
) -> Result<Value> {
    compare_until(
        cfg,
        store,
        dataset,
        mode,
        larger,
        review,
        std::future::pending(),
    )
    .await
}

pub async fn compare_until<F>(
    cfg: &Config,
    store: Arc<Store>,
    dataset: &Path,
    mode: &str,
    larger: Option<&str>,
    review: &Review,
    cancel: F,
) -> Result<Value>
where
    F: std::future::Future<Output = ()>,
{
    security::validate_provenance(&review.source, &review.license)?;
    if !review.held_out
        || review.code_revision.len() != 40
        || !review
            .code_revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !(1..=86400).contains(&review.max_run_seconds)
    {
        bail!(
            "Comparison requires held-out review, a full code revision and a bounded run duration"
        )
    }
    if !matches!(mode, "deep" | "research") {
        bail!("Comparison requires deep or research mode to exercise recursive arms")
    }
    if larger
        .is_some_and(|model| model.trim().is_empty() || model.len() > 256 || model == cfg.model)
    {
        bail!("Larger baseline requires a distinct bounded model identifier")
    }
    let bytes = security::read_regular_bytes(dataset, 2_000_000)?;
    let dataset_checksum = store::sha256(&bytes);
    if let Some(expected) = &review.expected_dataset_sha256 {
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("Reviewed dataset checksum must be 64 lowercase hexadecimal digits")
        }
        if expected != &dataset_checksum {
            bail!("Dataset differs from the operator-pinned reviewed checksum")
        }
    }
    let raw = std::str::from_utf8(&bytes)?;
    let cases = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Case>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !(1..=200).contains(&cases.len())
        || cases
            .iter()
            .any(|case| case.input.trim().is_empty() || case.input.len() > 12_000)
    {
        bail!("Comparison requires 1-200 bounded cases")
    }
    // Validate every criterion before starting any arm; expected labels never enter execution.
    let criteria = cases
        .iter()
        .map(Case::prepare)
        .collect::<Result<Vec<_>>>()?;
    let mut inputs = std::collections::BTreeSet::new();
    if cases
        .iter()
        .any(|case| !inputs.insert(normalize(&case.input)))
    {
        bail!("Duplicate comparison inputs")
    }
    let mut common = cfg.clone();
    // Role-specific model choices would confound the same-model comparison.
    common.role_profiles.clear();
    common.synthesize_results = false;
    common.validate()?;
    let started = tokio::time::Instant::now();
    let corpus = Arc::new(store.freeze_corpus()?);
    let snapshot_ms = started.elapsed().as_secs_f64() * 1000.0;
    let profiles = [
        ExecutionProfile::Alone,
        ExecutionProfile::Tools,
        ExecutionProfile::Recursive,
        ExecutionProfile::Collective,
        ExecutionProfile::Memory,
        ExecutionProfile::Experts,
    ];
    let mut arms = profiles
        .into_iter()
        .map(|profile| {
            (
                profile,
                common.model.clone(),
                Engine::new(common.clone(), store.clone()).with_frozen_corpus(corpus.clone()),
            )
        })
        .collect::<Vec<_>>();
    if let Some(model) = larger {
        let mut baseline = common.clone();
        baseline.model = model.into();
        baseline.model_registration = None;
        arms.push((
            ExecutionProfile::Alone,
            model.into(),
            Engine::new(baseline, store.clone()).with_frozen_corpus(corpus.clone()),
        ));
    }
    let deadline = started + Duration::from_secs(review.max_run_seconds);
    let mut details = Vec::new();
    let mut scores = vec![0usize; arms.len()];
    let mut counts = vec![0usize; arms.len()];
    let mut totals = vec![Totals::default(); arms.len()];
    let mut stopped = false;
    let mut cancelled = false;
    tokio::pin!(cancel);
    'cases: for (case_index, case) in cases.iter().enumerate() {
        // Rotate first position to reduce fixed warmup/order bias, without hiding order.
        for position in 0..arms.len() {
            if tokio::time::Instant::now() >= deadline {
                stopped = true;
                break 'cases;
            }
            let arm_index = (case_index + position) % arms.len();
            let (profile, model, engine) = &arms[arm_index];
            let outcome = engine
                .run_profile_until(&case.input, mode, *profile, async {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => {},
                        _ = &mut cancel => { cancelled = true; },
                    }
                })
                .await?;
            let passed = case.score(
                criteria[case_index].as_ref(),
                &outcome.status,
                &outcome.answer,
            );
            scores[arm_index] += usize::from(passed);
            totals[arm_index].add(&outcome, counts[arm_index] == 0);
            counts[arm_index] += 1;
            let trace = store.task(&outcome.task_id)?;
            let models = trace["events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|event| {
                    if event["kind"] == "reasoning" {
                        Some(event["payload"]["node"]["model"].clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let expert_models = trace["events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|event| event["kind"] == "expert_model_snapshot")
                .map(|event| event["payload"].clone())
                .collect::<Vec<_>>();
            details.push(json!({"case_index":case_index,"arm_index":arm_index,"execution_position":position,"profile":profile,"configured_model":model,
                "reported_models":models,"bound_expert_models":expert_models,"passed":passed,"expected":case.expected,"input":case.input,"outcome":outcome,"verification":case.verification}));
            if cancelled || tokio::time::Instant::now() >= deadline {
                stopped = true;
                break 'cases;
            }
        }
    }
    Ok(
        json!({"format_version":1,"dataset_checksum":dataset_checksum,"reviewed_dataset_checksum_operator_attested":review.expected_dataset_sha256,"dataset_checksum_operator_pinned":review.expected_dataset_sha256.is_some(),"dataset_source":review.source,"dataset_license":review.license,"held_out_operator_attested":true,
        "code_revision_operator_attested":review.code_revision,"package_version":env!("CARGO_PKG_VERSION"),"configuration":common,"mode":mode,"corpus_snapshot":corpus.metadata,"snapshot_elapsed_ms":snapshot_ms,
        "small_model_ablation_configuration_controlled":!details.iter().any(|detail| detail["bound_expert_models"].as_array().is_some_and(|models| !models.is_empty())),
        "case_count":cases.len(),"complete":!stopped,"stop_reason":if cancelled {"cancelled"} else if stopped {"run_deadline"} else {"completed"},"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"larger_baseline_configured":larger.is_some(),
        "arms":arms.iter().enumerate().map(|(index,(profile,model,_))| json!({"index":index,"profile":profile,"model":model,"executed":counts[index],"passed":scores[index],"resources":totals[index],"accuracy":if counts[index]>0 {Some(scores[index] as f64/counts[index] as f64)} else {None}})).collect::<Vec<_>>(),
        "details":details,"unavailable_metrics":["peak_memory","gpu_memory","energy","model_load_cost","general_security_failure_rate"],
        "limitations":["Held-out independence and code revision are operator-attested","Backend model labels do not attest immutable weights","A bound expert model configuration invalidates the same-model ablation control, even if advice is refused or fails; inspect per-case bound_expert_models","Exact-match and dependency-order criteria do not establish broad reasoning quality","Retrieval uses one read-only SQLite snapshot; checksum includes unrelated stored history","Snapshot bounds are 64 MB and temporarily require an additional serialization buffer","Live model availability, weight identity and server state are not frozen","Different arms consume different amounts within the same operator ceilings; inspect actual usage"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bound_expert_models_disclose_configuration_confounding_even_on_failure() {
        let root =
            std::env::temp_dir().join(format!("fabric-expert-compare-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let dataset = root.join("cases.jsonl");
        std::fs::write(
            &dataset,
            r#"{"input":"Explain ownership","expected":"held-out answer"}"#,
        )
        .unwrap();
        let model_file = root.join("model.json");
        std::fs::write(&model_file, r#"{"id":"domain.local","version":"1.0.0","model":"domain-model","api_base":"http://127.0.0.1:9/v1","source":"fixture","license":"CC0","context_tokens":4096,"max_output_tokens":512}"#).unwrap();
        let store = Arc::new(Store::in_memory().unwrap());
        let registered = store.register_model(&model_file).unwrap();
        let manifest = serde_json::from_value(json!({"id":"rust.model.s1","version":"1.0.0","kind":"workflow","description":"Reviewed advice","tags":["ownership"],"source":"fixture","license":"CC0","workflow":"Inspect ownership","model":{"id":"domain.local","version":"1.0.0"}})).unwrap();
        crate::registry::register(&store, &manifest).unwrap();
        crate::registry::activate(&store, "rust.model.s1", "1.0.0").unwrap();
        let cfg = Config {
            backend: "demo".into(),
            interpret_goals: false,
            max_calls: 4,
            max_cycles: 1,
            ..Config::default()
        };
        let review = Review {
            source: "fixture".into(),
            license: "CC0".into(),
            code_revision: "a".repeat(40),
            held_out: true,
            expected_dataset_sha256: None,
            max_run_seconds: 10,
        };
        let report = compare(&cfg, store, &dataset, "deep", None, &review)
            .await
            .unwrap();
        assert_eq!(report["complete"], true);
        assert_eq!(
            report["small_model_ablation_configuration_controlled"],
            false
        );
        for detail in report["details"].as_array().unwrap() {
            let models = detail["bound_expert_models"].as_array().unwrap();
            if detail["profile"] == "experts" {
                assert_eq!(models.len(), 1);
                assert_eq!(models[0]["requested_model"], "domain-model");
                assert_eq!(models[0]["model_checksum"], registered.checksum);
                assert_eq!(models[0]["weights_attested"], false);
            } else {
                assert!(models.is_empty());
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn sql_comparison_scores_structured_results_without_trusting_status() {
        let input = r#"sql:{"tables":[{"name":"items","columns":["amount"],"rows":[[2],[3]]}],"query":"SELECT sum(amount) AS total FROM items"}"#;
        let expected = r#"{"rows":[[5]],"columns":["total"]}"#;
        let case = Case {
            input: input.into(),
            expected: Some(expected.into()),
            verification: Criterion::SqlResultJsonV1,
        };
        assert!(case.prepare().unwrap().is_none());
        assert!(case.score(
            None,
            "unverified",
            r#"{ "columns": ["total"], "rows": [[5]] }"#
        ));
        for answer in [
            r#"{"columns":["total"],"rows":[[6]]}"#,
            r#"{"columns":["total"],"rows":[["5"]]}"#,
            r#"{"columns":["total"],"rows":[[5]],"extra":true}"#,
            r#"{"columns":["total"],"rows":[[5]],"rows":[[6]]}"#,
        ] {
            assert!(!case.score(None, "completed", answer));
        }
        assert!(!case.score(None, "demo", expected));
        assert!(!case.score(None, "failed", expected));
        let root =
            std::env::temp_dir().join(format!("fabric-sql-compare-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let dataset = root.join("cases.jsonl");
        std::fs::write(
            &dataset,
            json!({"input":input,"expected":expected,"verification":"sql_result_json_v1"})
                .to_string(),
        )
        .unwrap();
        let cfg = Config {
            backend: "demo".into(),
            ..Config::default()
        };
        let review = Review {
            source: "fixture".into(),
            license: "CC0".into(),
            code_revision: "a".repeat(40),
            held_out: true,
            expected_dataset_sha256: None,
            max_run_seconds: 10,
        };
        let report = compare(
            &cfg,
            Arc::new(Store::in_memory().unwrap()),
            &dataset,
            "deep",
            None,
            &review,
        )
        .await
        .unwrap();
        assert_eq!(report["complete"], true);
        assert_eq!(report["arms"][0]["passed"], 0);
        for arm in &report["arms"].as_array().unwrap()[1..] {
            assert_eq!(arm["passed"], 1);
            assert_eq!(arm["resources"]["model_calls"], 0);
            let expected_tools = if matches!(arm["profile"].as_str(), Some("memory" | "experts")) {
                2
            } else {
                1
            };
            assert_eq!(arm["resources"]["tool_calls"], expected_tools);
        }
        for invalid in [
            json!({"input":"plain goal","expected":expected,"verification":"sql_result_json_v1"}),
            json!({"input":input,"verification":"sql_result_json_v1"}),
            json!({"input":input,"expected":"{}","verification":"sql_result_json_v1"}),
        ] {
            assert!(serde_json::from_value::<Case>(invalid)
                .unwrap()
                .prepare()
                .is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn authored_v1_suite_has_bounded_unique_cases_and_checked_tool_labels() {
        let bytes = include_bytes!("../evaluation/v1_cases.jsonl");
        let manifest: Value =
            serde_json::from_str(include_str!("../evaluation/v1_manifest.json")).unwrap();
        assert_eq!(manifest["sha256"], store::sha256(bytes));
        assert_eq!(manifest["case_count"], 100);
        assert_eq!(manifest["independent_review_completed"], false);
        assert_eq!(manifest["held_out_independence_verified"], false);
        assert_eq!(manifest["live_model_evaluation_completed"], false);
        let entries = manifest["cases"].as_array().unwrap();
        let cases = std::str::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Case>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 100);
        assert_eq!(entries.len(), cases.len());
        let mut unique = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        let mut categories = std::collections::BTreeMap::<String, usize>::new();
        let mut checked = 0;
        for (index, (case, entry)) in cases.iter().zip(entries).enumerate() {
            assert!(case.input.len() <= 12000);
            assert!(unique.insert(normalize(&case.input)));
            assert!(ids.insert(entry["id"].as_str().unwrap()));
            assert_eq!(entry["line"], index + 1);
            *categories
                .entry(entry["category"].as_str().unwrap().to_string())
                .or_default() += 1;
            let criterion = case.prepare().unwrap();
            if let Some(expression) = crate::math::extract(&case.input) {
                let answer = crate::math::calculate(expression).unwrap();
                assert!(
                    case.score(None, "completed", &answer),
                    "Arithmetic case {}",
                    index + 1
                );
                checked += 1;
            } else if let Some(raw) = case.input.strip_prefix("sql:") {
                let answer = crate::sql::execute(raw.to_string())
                    .await
                    .unwrap_or_else(|error| panic!("SQL case {}: {error:#}", index + 1));
                assert!(
                    case.score(None, "completed", &answer),
                    "SQL case {}",
                    index + 1
                );
                checked += 1;
            } else if let Some(criterion) = criterion {
                let witness = serde_json::to_string(&entry["valid_order_example"]).unwrap();
                assert!(case.score(Some(&criterion), "unverified", &witness));
                checked += 1;
            }
        }
        assert_eq!(checked, 30);
        assert_eq!(categories.len(), 10);
        assert!(categories.values().all(|count| *count == 10));
    }

    #[tokio::test]
    async fn reviewed_dataset_hash_pins_bytes_and_rejects_changes_before_tasks() {
        let root =
            std::env::temp_dir().join(format!("fabric-pinned-dataset-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("cases.jsonl");
        let data = r#"{"input":"calc: 7 * 6","expected":"42"}"#;
        std::fs::write(&file, data).unwrap();
        let checksum = store::sha256(data.as_bytes());
        let mut review = Review {
            source: "fixture".into(),
            license: "CC0".into(),
            code_revision: "a".repeat(40),
            held_out: true,
            expected_dataset_sha256: Some(checksum.clone()),
            max_run_seconds: 10,
        };
        let cfg = Config {
            backend: "demo".into(),
            ..Config::default()
        };
        let store = Arc::new(Store::in_memory().unwrap());
        let report = compare(&cfg, store.clone(), &file, "deep", None, &review)
            .await
            .unwrap();
        assert_eq!(report["dataset_checksum"], checksum);
        assert_eq!(
            report["reviewed_dataset_checksum_operator_attested"],
            checksum
        );
        assert_eq!(report["dataset_checksum_operator_pinned"], true);
        let count = || {
            store
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM tasks", [], |row| row.get::<_, u64>(0))
                .unwrap()
        };
        let original_count = count();
        assert_eq!(original_count, 6);
        for invalid in ["A".repeat(64), "0".repeat(63), "0".repeat(64)] {
            review.expected_dataset_sha256 = Some(invalid);
            assert!(compare(&cfg, store.clone(), &file, "deep", None, &review)
                .await
                .is_err());
            assert_eq!(count(), original_count);
        }
        review.expected_dataset_sha256 = Some(checksum);
        std::fs::write(&file, format!("{data}\n")).unwrap();
        assert!(compare(&cfg, store.clone(), &file, "deep", None, &review)
            .await
            .unwrap_err()
            .to_string()
            .contains("differs"));
        assert_eq!(count(), original_count);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_totals_use_maxima_and_preserve_unavailable_samples() {
        let mut outcome = crate::engine::Outcome {
            task_id: "sample".into(),
            answer: String::new(),
            status: "unverified".into(),
            calls: 0,
            cycles: 0,
            elapsed_ms: 0.0,
            evidence: vec![],
            skills: vec![],
            stop_reason: String::new(),
            resources: crate::reasoning::Resources::default(),
        };
        outcome.resources.process_rss_bytes = Some(100);
        outcome.resources.process_lifetime_peak_rss_bytes = Some(200);
        let mut totals = Totals::default();
        totals.add(&outcome, true);
        outcome.resources.process_rss_bytes = Some(80);
        outcome.resources.process_lifetime_peak_rss_bytes = Some(240);
        totals.add(&outcome, false);
        assert_eq!(totals.max_sampled_process_rss_bytes, Some(100));
        assert_eq!(totals.process_lifetime_peak_rss_bytes, Some(240));
        outcome.resources.process_rss_bytes = None;
        outcome.resources.process_lifetime_peak_rss_bytes = None;
        totals.add(&outcome, false);
        assert!(totals.max_sampled_process_rss_bytes.is_none());
        assert!(totals.process_lifetime_peak_rss_bytes.is_none());
    }
    #[tokio::test]
    async fn ordering_comparison_scores_every_arm_through_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cfg = Config {
            backend: "openai-local".into(),
            interpret_goals: false,
            api_base: format!("http://{}/v1", listener.local_addr().unwrap()),
            ..Config::default()
        };
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0u8; 4096];
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                    assert!(request.len() < 40_000);
                    if let Some(end) = request.windows(4).position(|s| s == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let body = json!({"model":"fixture/v1","choices":[{"message":{"content":r#"["b","a","c"]"#}}]}).to_string();
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
            }
        });
        let dataset = std::env::temp_dir().join(format!(
            "fabric-order-compare-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&dataset, serde_json::to_string(&json!({"input":r#"order:{"nodes":["a","b","c"],"before":[["a","c"]]}"#,"verification":"dependency_order_v1"})).unwrap()).unwrap();
        let store = Arc::new(Store::in_memory().unwrap());
        let review = Review {
            source: "test".into(),
            license: "CC0-1.0".into(),
            code_revision: "a".repeat(40),
            held_out: true,
            expected_dataset_sha256: None,
            max_run_seconds: 10,
        };
        let report = compare(&cfg, store, &dataset, "deep", None, &review)
            .await
            .unwrap();
        assert_eq!(report["complete"], true);
        for arm in report["arms"].as_array().unwrap() {
            assert_eq!(arm["passed"], 1);
        }
        for detail in report["details"].as_array().unwrap() {
            assert_eq!(detail["verification"], "dependency_order_v1");
            assert!(detail["expected"].is_null());
        }
        assert_eq!(report["details"][0]["outcome"]["status"], "unverified");
        server.abort();
        let _ = server.await;
        std::fs::remove_file(dataset).unwrap();
    }

    #[test]
    fn ordering_scoring_accepts_alternatives_without_trusting_runtime_verdicts() {
        let case: Case = serde_json::from_value(json!({"input":r#"order:{"nodes":["a","b","c"],"before":[["a","c"]]}"#,"verification":"dependency_order_v1"})).unwrap();
        let criterion = case.prepare().unwrap().unwrap();
        for answer in [r#"["a","b","c"]"#, r#"["b","a","c"]"#] {
            assert!(case.score(Some(&criterion), "unverified", answer));
            assert!(case.score(Some(&criterion), "completed", answer));
            assert!(!case.score(Some(&criterion), "demo", answer));
            assert!(!case.score(Some(&criterion), "rejected", answer));
        }
        assert!(!case.score(Some(&criterion), "completed", r#"["c","a","b"]"#));
        for value in [
            json!({"input":"Explain ordering","verification":"dependency_order_v1"}),
            json!({"input":r#"order:{"nodes":["a"],"before":[]}"#,"verification":"dependency_order_v1","expected":"a"}),
            json!({"input":"Explain ordering"}),
        ] {
            assert!(serde_json::from_value::<Case>(value)
                .unwrap()
                .prepare()
                .is_err());
        }
        assert!(serde_json::from_value::<Case>(
            json!({"input":"a","verification":"model_confidence"})
        )
        .is_err());
        let legacy: Case =
            serde_json::from_value(json!({"input":"Question","expected":" Answer "})).unwrap();
        assert!(legacy.prepare().unwrap().is_none());
        assert!(legacy.score(None, "unverified", "answer"));
    }

    #[tokio::test]
    async fn malformed_comparison_criteria_fail_before_creating_tasks() {
        let dataset = std::env::temp_dir().join(format!(
            "fabric-invalid-criterion-{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&dataset, serde_json::to_string(&json!({"input":r#"order:{"nodes":["a"],"before":[["a","a"]]}"#,"verification":"dependency_order_v1"})).unwrap()).unwrap();
        let store = Arc::new(Store::in_memory().unwrap());
        let review = Review {
            source: "test".into(),
            license: "CC0-1.0".into(),
            code_revision: "a".repeat(40),
            held_out: true,
            expected_dataset_sha256: None,
            max_run_seconds: 10,
        };
        assert!(compare(
            &Config::default(),
            store.clone(),
            &dataset,
            "deep",
            None,
            &review
        )
        .await
        .is_err());
        assert_eq!(
            store
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM tasks", [], |row| row.get::<_, u64>(0))
                .unwrap(),
            0
        );
        std::fs::remove_file(dataset).unwrap();
    }

    #[tokio::test]
    async fn comparison_deadline_cancels_and_persists_the_active_task() {
        let root =
            std::env::temp_dir().join(format!("fabric-compare-deadline-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let dataset = root.join("case.jsonl");
        std::fs::write(
            &dataset,
            "{\"input\":\"Explain borrowing\",\"expected\":\"expected answer\"}",
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut cfg = Config::load(Some(root.clone()), Some("openai-local".into())).unwrap();
        cfg.api_base = format!("http://{}/v1", listener.local_addr().unwrap());
        let stalled = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
            drop(stream);
        });
        let store = Arc::new(Store::in_memory().unwrap());
        let review = Review {
            source: "test".into(),
            license: "CC0-1.0".into(),
            code_revision: "b".repeat(40),
            held_out: true,
            expected_dataset_sha256: None,
            max_run_seconds: 1,
        };
        let report = compare(&cfg, store.clone(), &dataset, "deep", None, &review)
            .await
            .unwrap();
        assert_eq!(report["complete"], false);
        assert_eq!(report["details"].as_array().unwrap().len(), 1);
        let task = report["details"][0]["outcome"]["task_id"].as_str().unwrap();
        assert_eq!(store.task(task).unwrap()["task"]["status"], "cancelled");
        assert_eq!(report["arms"][0]["resources"]["model_calls"], 1);
        assert_eq!(report["arms"][0]["resources"]["input_tokens"], Value::Null);
        stalled.abort();
        let _ = stalled.await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn comparison_ablates_tools_and_keeps_expected_answers_outside_execution() {
        let root = std::env::temp_dir().join(format!("fabric-compare-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let dataset = root.join("cases.jsonl");
        std::fs::write(&dataset, "{\"input\":\"calc: 6 * 7\",\"expected\":\"42\"}\n{\"input\":\"calc: 1 + 2\",\"expected\":\"withheld sentinel\"}").unwrap();
        let cfg = Config::load(Some(root.clone()), Some("demo".into())).unwrap();
        let store = Arc::new(Store::in_memory().unwrap());
        store::bootstrap(&store).unwrap();
        let review = Review {
            source: "test".into(),
            license: "CC0-1.0".into(),
            code_revision: "a".repeat(40),
            held_out: true,
            expected_dataset_sha256: None,
            max_run_seconds: 10,
        };
        let report = compare(&cfg, store.clone(), &dataset, "deep", None, &review)
            .await
            .unwrap();
        assert_eq!(report["complete"], true);
        assert_eq!(
            report["small_model_ablation_configuration_controlled"],
            true
        );
        assert_eq!(report["corpus_snapshot"]["frozen"], true);
        assert_eq!(
            report["corpus_snapshot"]["database_snapshot_checksum"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(report["arms"].as_array().unwrap().len(), 6);
        assert_eq!(report["details"].as_array().unwrap().len(), 12);
        assert_eq!(report["arms"][0]["passed"], 0);
        assert_eq!(report["arms"][1]["passed"], 1);
        for detail in report["details"].as_array().unwrap() {
            let task = detail["outcome"]["task_id"].as_str().unwrap();
            let trace = store.task(task).unwrap();
            assert!(trace["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "evaluation_corpus"
                    && event["payload"] == report["corpus_snapshot"]));
            assert!(!store
                .task(task)
                .unwrap()
                .to_string()
                .contains("withheld sentinel"));
        }
        let cancelled = compare_until(
            &cfg,
            store.clone(),
            &dataset,
            "deep",
            None,
            &review,
            std::future::ready(()),
        )
        .await
        .unwrap();
        assert_eq!(cancelled["complete"], false);
        assert_eq!(cancelled["stop_reason"], "cancelled");
        assert_eq!(cancelled["details"][0]["outcome"]["status"], "cancelled");
        assert_eq!(cancelled["details"][0]["outcome"]["calls"], 0);
        let duplicates = root.join("duplicates.jsonl");
        std::fs::write(&duplicates, "{\"input\":\"Rust ownership\",\"expected\":\"a\"}\n{\"input\":\" rust  OWNERSHIP \",\"expected\":\"b\"}").unwrap();
        assert!(
            compare(&cfg, store.clone(), &duplicates, "deep", None, &review)
                .await
                .unwrap_err()
                .to_string()
                .contains("Duplicate")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
