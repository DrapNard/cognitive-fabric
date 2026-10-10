//! Evaluation is independent of model-produced success claims.
use crate::{
    engine::Engine,
    security,
    store::{self, Store},
};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, time::Instant};

fn read_capped(path: &Path, max: usize) -> Result<String> {
    Ok(String::from_utf8(security::read_regular_bytes(path, max)?)?)
}

#[derive(Deserialize)]
struct Case {
    input: String,
    expected: String,
}

pub async fn benchmark(engine: &Engine, file: &Path, mode: &str) -> Result<Value> {
    let raw = read_capped(file, 2_000_000)?;
    let cases = raw
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(serde_json::from_str::<Case>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if cases.is_empty() || cases.len() > 1000 {
        bail!("Benchmark must contain 1-1000 cases")
    }
    let began = Instant::now();
    let mut passed = 0;
    let mut calls = 0;
    let mut details = Vec::new();
    for case in &cases {
        if case.input.len() > 12000 || case.expected.len() > 16000 {
            bail!("Benchmark case exceeds limits")
        }
        let out = engine.run(&case.input, mode).await?;
        let ok = matches!(out.status.as_str(), "completed" | "unverified")
            && out
                .answer
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
                == case
                    .expected
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
        if ok {
            passed += 1
        }
        calls += out.calls;
        details.push(json!({"task_id":out.task_id,"passed":ok,"input":case.input,
            "expected":case.expected,"answer":out.answer,"latency_ms":out.elapsed_ms,"model_calls":out.calls,
            "verification":"expected_output_exact_match","runtime_status":out.status,"resources":out.resources,"stop_reason":out.stop_reason}));
    }
    Ok(
        json!({"total":cases.len(),"passed":passed,"accuracy":passed as f64/cases.len() as f64,
         "elapsed_ms":began.elapsed().as_secs_f64()*1000.0,"model_calls":calls,"details":details}),
    )
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetrievalCase {
    query: String,
    expected_excerpt: String,
}

#[derive(Clone, Debug)]
pub struct DatasetReview {
    pub source: String,
    pub license: String,
    pub held_out: bool,
}

fn retrieval_cases(raw: &str) -> Result<Vec<RetrievalCase>> {
    let cases = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<RetrievalCase>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !(3..=1000).contains(&cases.len()) {
        bail!("Quality suite requires 3-1000 cases")
    }
    let mut seen = std::collections::BTreeSet::new();
    for case in &cases {
        let normalized = case
            .query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if normalized.is_empty()
            || case.query.len() > 500
            || case.expected_excerpt.trim().is_empty()
            || case.expected_excerpt.len() > 200
            || !seen.insert(normalized)
        {
            bail!("Invalid or duplicate held-out query")
        }
    }
    Ok(cases)
}

fn retrieval_results(
    skill: &str,
    body: &str,
    source: &str,
    license: &str,
    cases: &[RetrievalCase],
) -> Result<Vec<bool>> {
    let staging = Store::in_memory()?;
    staging.ensure_skill(skill, "isolated reference", &["test".into()])?;
    staging.add_document(&store::Document {
        id: "candidate".into(),
        skill_id: skill.into(),
        title: "Reference document".into(),
        body: body.into(),
        source: source.into(),
        license: license.into(),
    })?;
    cases
        .iter()
        .map(|case| {
            let hits = staging.search(&case.query, Some(skill), 1)?;
            Ok(hits.first().is_some_and(|document| {
                document
                    .body
                    .to_lowercase()
                    .contains(&case.expected_excerpt.to_lowercase())
            }))
        })
        .collect()
}

pub fn evaluate(store: &Store, id: &str, suite: &str, dataset: Option<&Path>) -> Result<Value> {
    evaluate_reviewed(store, id, suite, dataset, None)
}

pub fn evaluate_reviewed(
    store: &Store,
    id: &str,
    suite: &str,
    dataset: Option<&Path>,
    review: Option<&DatasetReview>,
) -> Result<Value> {
    security::authorize("human-cli", "candidate.evaluate")?;
    let candidate = store.candidate(id)?;
    if candidate.kind != "rag" {
        bail!("Neural candidate evaluation requires an independent neural evaluator")
    }
    let bytes = read_capped(Path::new(&candidate.artifact), 256_000)?;
    let integrity = store::sha256(bytes.as_bytes()) == candidate.checksum;
    if suite == "integrity" {
        store.evaluation(
            id,
            "integrity",
            integrity,
            if integrity { 1.0 } else { 0.0 },
            json!({"checksum_matches":integrity,"candidate_checksum":candidate.checksum}),
        )?;
        return Ok(
            json!({"checksum_matches":integrity,"warning":"Integrity is not behavioral safety"}),
        );
    }
    if suite != "quality" {
        bail!("Unknown suite")
    }
    if !integrity {
        bail!("Candidate failed checksum verification")
    }
    let review = review.ok_or_else(|| {
        anyhow::anyhow!("Quality requires dataset source, license and explicit held-out review")
    })?;
    security::validate_provenance(&review.source, &review.license)?;
    if !review.held_out {
        bail!("Operator must attest that evaluation data is held out from training")
    }
    let dataset = dataset.ok_or_else(|| anyhow::anyhow!("Quality requires --dataset"))?;
    let raw = read_capped(dataset, 2_000_000)?;
    let cases = retrieval_cases(&raw)?;
    let checksum = store::sha256(raw.as_bytes());
    if checksum == candidate.checksum {
        bail!("Artifact and held-out dataset must differ")
    }
    let results = retrieval_results(
        &candidate.skill_id,
        &bytes,
        &candidate.source,
        &candidate.license,
        &cases,
    )?;
    let passed = results.iter().filter(|result| **result).count();
    let accuracy = passed as f64 / cases.len() as f64;
    // Re-reviewing an already approved immutable artifact can initialize legacy provenance.
    // It never promotes that artifact again or changes its active version.
    let baseline = if candidate.status == "approved" {
        Vec::new()
    } else {
        store.rag_baselines(&candidate.skill_id)?
    };
    let mut regression_total = 0usize;
    let mut regression_passed = 0usize;
    let baseline_version = {
        store
            .skills()?
            .into_iter()
            .find(|skill| skill.id == candidate.skill_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown skill"))?
            .active_version
    };
    let mut baseline_ids = Vec::new();
    let mut historical_cases = 0usize;
    for baseline in baseline {
        baseline_ids.push((baseline.candidate_id, baseline.version));
        let previous_cases = retrieval_cases(&baseline.dataset_content)?;
        historical_cases += previous_cases.len();
        if historical_cases > 8192 {
            bail!("Historical RAG evaluation exceeds 8192 cases")
        }
        let previous_results = retrieval_results(
            &candidate.skill_id,
            &baseline.body,
            &baseline.source,
            &baseline.license,
            &previous_cases,
        )?;
        let candidate_results = retrieval_results(
            &candidate.skill_id,
            &bytes,
            &candidate.source,
            &candidate.license,
            &previous_cases,
        )?;
        for (previous, current) in previous_results.into_iter().zip(candidate_results) {
            if previous {
                regression_total += 1;
                if current {
                    regression_passed += 1;
                }
            }
        }
    }
    let regression_ok = regression_total == regression_passed;
    let registered = store.record_dataset(&raw, &review.source, &review.license)?;
    debug_assert_eq!(registered, checksum);
    store.evaluation(id,"quality",accuracy>=0.9,accuracy,json!({"dataset_checksum":checksum,"source":review.source,"license":review.license,"held_out":true,
        "candidate_checksum":candidate.checksum,"cases":cases.len(),"accuracy":accuracy,"evaluator":"isolated-fts5/v1"}))?;
    let regression_score = if regression_total == 0 {
        1.0
    } else {
        regression_passed as f64 / regression_total as f64
    };
    store.evaluation(id,"regression",regression_ok,regression_score,json!({"baseline_version":baseline_version,"baseline_ids":baseline_ids,
        "candidate_checksum":candidate.checksum,"candidate_dataset_checksum":checksum,"total":regression_total,"passed":regression_passed}))?;
    Ok(
        json!({"total":cases.len(),"passed":passed,"accuracy":accuracy,"quality_gate":accuracy>=0.9,"regression_gate":regression_ok,
        "gate_passed":accuracy>=0.9 && regression_ok,"regression_total":regression_total,"regression_passed":regression_passed,"dataset_checksum":checksum,
        "details":cases.iter().zip(results).map(|(case,result)|json!({"query":case.query,"passed":result})).collect::<Vec<_>>(),
        "warning":"Retrieval and regression only; held-out independence is operator-attested, not automatically proven"}),
    )
}

pub fn promote(store: &Store, id: &str, human_approve: bool) -> Result<Value> {
    security::authorize("human-cli", "candidate.promote")?;
    if !human_approve {
        bail!("Explicit --human-approve required")
    }
    let candidate = store.candidate(id)?;
    if candidate.kind != "rag" {
        bail!("Neural weight promotion remains disabled")
    }
    let bytes = read_capped(Path::new(&candidate.artifact), 256_000)?;
    if store::sha256(bytes.as_bytes()) != candidate.checksum {
        bail!("Artifact modified since evaluation")
    }
    Ok(serde_json::to_value(store.promote_rag(id, &bytes)?)?)
}

pub fn rollback(store: &Store, id: &str, human_approve: bool) -> Result<Value> {
    security::authorize("human-cli", "candidate.rollback")?;
    if !human_approve {
        bail!("Explicit --human-approve required")
    }
    store.rollback_rag(id)?;
    Ok(json!({"candidate_id":id,"rolled_back":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    #[test]
    fn held_out_queries_require_distinct_cases() {
        let repeated = "{\"query\":\"Rust async\",\"expected_excerpt\":\"Rust\"}\n{\"query\":\" RUST  async \",\"expected_excerpt\":\"Rust\"}\n{\"query\":\"mutex\",\"expected_excerpt\":\"mutex\"}";
        assert!(retrieval_cases(repeated).is_err());
    }

    #[test]
    fn promotion_rejects_regressions_and_stale_baselines() {
        let root =
            std::env::temp_dir().join(format!("cogfabric-regression-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let store = Store::in_memory().unwrap();
        store::bootstrap(&store).unwrap();
        let review = DatasetReview {
            source: "test-held-out".into(),
            license: "CC0-1.0".into(),
            held_out: true,
        };
        let make = |name: &str, body: &str| {
            let path = root.join(name);
            fs::write(&path, body).unwrap();
            store
                .create_candidate(
                    "rust.s1",
                    "rag",
                    path.to_str().unwrap(),
                    &store::sha256(body.as_bytes()),
                    "test",
                    "CC0-1.0",
                )
                .unwrap()
        };
        let dataset = |name: &str, words: &[&str]| {
            let path = root.join(name);
            let raw = words
                .iter()
                .map(|word| json!({"query":word,"expected_excerpt":word}).to_string())
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(&path, raw).unwrap();
            path
        };
        let old = dataset("old.jsonl", &["alpha", "beta", "gamma"]);
        let new = dataset("new.jsonl", &["delta", "epsilon", "zeta"]);
        let first = make("first.txt", "alpha beta gamma");
        assert!(evaluate(&store, &first.id, "quality", Some(&old)).is_err());
        let unreviewed = DatasetReview {
            held_out: false,
            ..review.clone()
        };
        assert!(
            evaluate_reviewed(&store, &first.id, "quality", Some(&old), Some(&unreviewed)).is_err()
        );
        evaluate(&store, &first.id, "integrity", None).unwrap();
        evaluate_reviewed(&store, &first.id, "quality", Some(&old), Some(&review)).unwrap();
        promote(&store, &first.id, true).unwrap();
        assert!(
            evaluate_reviewed(&store, &first.id, "quality", Some(&new), Some(&review))
                .unwrap_err()
                .to_string()
                .contains("immutable")
        );
        let broken = make("broken.txt", "delta epsilon zeta");
        evaluate(&store, &broken.id, "integrity", None).unwrap();
        let result =
            evaluate_reviewed(&store, &broken.id, "quality", Some(&new), Some(&review)).unwrap();
        assert_eq!(result["quality_gate"], true);
        assert_eq!(result["regression_gate"], false);
        assert_eq!(result["regression_total"], 3);
        assert!(promote(&store, &broken.id, true).is_err());
        let a = make("a.txt", "alpha beta gamma delta epsilon zeta");
        let b = make("b.txt", "alpha beta gamma delta epsilon zeta eta");
        for candidate in [&a, &b] {
            evaluate(&store, &candidate.id, "integrity", None).unwrap();
            let result =
                evaluate_reviewed(&store, &candidate.id, "quality", Some(&new), Some(&review))
                    .unwrap();
            assert_eq!(result["gate_passed"], true);
        }
        promote(&store, &a.id, true).unwrap();
        assert!(promote(&store, &b.id, true)
            .unwrap_err()
            .to_string()
            .contains("changed"));
        evaluate_reviewed(&store, &b.id, "quality", Some(&new), Some(&review)).unwrap();
        promote(&store, &b.id, true).unwrap();
        let loses_history = make("loses-history.txt", "delta epsilon zeta eta theta");
        let result = evaluate_reviewed(
            &store,
            &loses_history.id,
            "quality",
            Some(&new),
            Some(&review),
        )
        .unwrap();
        assert_eq!(result["quality_gate"], true);
        assert_eq!(result["regression_gate"], false);
        assert_eq!(result["regression_total"], 9);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn candidate_requires_tests_and_human_approval() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let document = root.join("examples/skill_document.md");
        let bench = root.join("examples/retrieval_bench.jsonl");
        let db = std::env::temp_dir().join(format!("cogfabric-gate-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&db).unwrap();
        store::bootstrap(&store).unwrap();
        let bytes = fs::read(&document).unwrap();
        let artifact = document.canonicalize().unwrap();
        let candidate = store
            .create_candidate(
                "rust.s1",
                "rag",
                artifact.to_str().unwrap(),
                &store::sha256(&bytes),
                "project-test",
                "CC0-1.0",
            )
            .unwrap();
        assert!(promote(&store, &candidate.id, true).is_err());
        let integrity = evaluate(&store, &candidate.id, "integrity", None).unwrap();
        assert_eq!(integrity["checksum_matches"], true);
        let quality = evaluate_reviewed(
            &store,
            &candidate.id,
            "quality",
            Some(&bench),
            Some(&DatasetReview {
                source: "project-test".into(),
                license: "CC0-1.0".into(),
                held_out: true,
            }),
        )
        .unwrap();
        assert_eq!(quality["gate_passed"], true);
        assert!(promote(&store, &candidate.id, false).is_err());
        promote(&store, &candidate.id, true).unwrap();
        assert!(!store
            .search("Rust async mutex", Some("rust.s1"), 1)
            .unwrap()
            .is_empty());
        drop(store);
        let _ = fs::remove_file(&db);
    }
}
