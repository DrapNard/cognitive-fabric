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
    expected: String,
}

pub struct Review {
    pub source: String,
    pub license: String,
    pub code_revision: String,
    pub held_out: bool,
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
    let raw = std::str::from_utf8(&bytes)?;
    let cases = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Case>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !(1..=200).contains(&cases.len())
        || cases.iter().any(|case| {
            case.input.trim().is_empty()
                || case.input.len() > 12_000
                || case.expected.trim().is_empty()
                || case.expected.len() > 16_000
        })
    {
        bail!("Comparison requires 1-200 bounded cases")
    }
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
            let passed = matches!(outcome.status.as_str(), "completed" | "unverified")
                && normalize(&outcome.answer) == normalize(&case.expected);
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
            details.push(json!({"case_index":case_index,"arm_index":arm_index,"execution_position":position,"profile":profile,"configured_model":model,
                "reported_models":models,"passed":passed,"expected":case.expected,"input":case.input,"outcome":outcome,"verification":"expected_output_exact_match"}));
            if cancelled || tokio::time::Instant::now() >= deadline {
                stopped = true;
                break 'cases;
            }
        }
    }
    Ok(
        json!({"format_version":1,"dataset_checksum":store::sha256(&bytes),"dataset_source":review.source,"dataset_license":review.license,"held_out_operator_attested":true,
        "code_revision_operator_attested":review.code_revision,"package_version":env!("CARGO_PKG_VERSION"),"configuration":common,"mode":mode,"corpus_snapshot":corpus.metadata,"snapshot_elapsed_ms":snapshot_ms,
        "case_count":cases.len(),"complete":!stopped,"stop_reason":if cancelled {"cancelled"} else if stopped {"run_deadline"} else {"completed"},"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"larger_baseline_configured":larger.is_some(),
        "arms":arms.iter().enumerate().map(|(index,(profile,model,_))| json!({"index":index,"profile":profile,"model":model,"executed":counts[index],"passed":scores[index],"resources":totals[index],"accuracy":if counts[index]>0 {Some(scores[index] as f64/counts[index] as f64)} else {None}})).collect::<Vec<_>>(),
        "details":details,"unavailable_metrics":["peak_memory","gpu_memory","energy","model_load_cost","general_security_failure_rate"],
        "limitations":["Held-out independence and code revision are operator-attested","Backend model labels do not attest immutable weights","Exact-match criteria do not establish broad reasoning quality","Retrieval uses one read-only SQLite snapshot; checksum includes unrelated stored history","Snapshot bounds are 64 MB and temporarily require an additional serialization buffer","Live model availability, weight identity and server state are not frozen","Different arms consume different amounts within the same operator ceilings; inspect actual usage"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
            max_run_seconds: 10,
        };
        let report = compare(&cfg, store.clone(), &dataset, "deep", None, &review)
            .await
            .unwrap();
        assert_eq!(report["complete"], true);
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
