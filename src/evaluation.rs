//! Evaluation is independent of model-produced success claims.
use crate::{
    engine::Engine,
    security,
    store::{self, Store},
};
use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc, time::Instant};

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

#[derive(Deserialize)]
struct RetrievalCase {
    query: String,
    expected_excerpt: String,
}

pub fn evaluate(store: &Store, id: &str, suite: &str, dataset: Option<&Path>) -> Result<Value> {
    security::authorize("human-cli", "candidate.evaluate")?;
    let candidate = store.candidate(id)?;
    if candidate.kind != "rag" {
        bail!("Neural candidate evaluation requires a separate neural evaluator")
    };
    let path = Path::new(&candidate.artifact);
    let bytes = read_capped(path, 256_000)?;
    let integrity = store::sha256(bytes.as_bytes()) == candidate.checksum;
    if suite == "integrity" {
        store.evaluation(
            id,
            "integrity",
            integrity,
            integrity as u8 as f64,
            json!({"checksum_matches":integrity}),
        )?;
        return Ok(
            json!({"checksum_matches":integrity,"warning":"Integrity is not a behavioral safety test"}),
        );
    }
    if suite != "quality" {
        bail!("Unknown suite")
    }
    if !integrity {
        bail!("Candidate failed checksum verification")
    }
    let dataset = dataset.ok_or_else(|| anyhow::anyhow!("Quality suite requires --dataset"))?;
    let raw = read_capped(dataset, 2_000_000)?;
    let cases = raw
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(serde_json::from_str::<RetrievalCase>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if cases.len() < 3 || cases.len() > 1000 {
        bail!("Quality suite requires 3-1000 cases")
    }
    // Candidate-only isolated corpus. Never use unrelated production documents to qualify a skill.
    let tmp = std::env::temp_dir().join(format!("fabric-eval-{}.db", uuid::Uuid::new_v4()));
    let staging = Arc::new(Store::open(&tmp)?);
    staging.ensure_skill(&candidate.skill_id, "candidate", &["test".into()])?;
    staging.add_document(&store::Document {
        id: "candidate".into(),
        skill_id: candidate.skill_id.clone(),
        title: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("candidate")
            .into(),
        body: bytes.clone(),
        source: candidate.source.clone(),
        license: candidate.license.clone(),
    })?;
    let mut passes = 0usize;
    let mut details = Vec::new();
    for case in &cases {
        if case.query.len() > 500
            || case.query.trim().is_empty()
            || case.expected_excerpt.len() > 200
            || case.expected_excerpt.is_empty()
        {
            bail!("Invalid quality case")
        }
        let hits = staging.search(&case.query, Some(&candidate.skill_id), 1)?;
        let ok = hits.first().is_some_and(|doc| {
            doc.body
                .to_lowercase()
                .contains(&case.expected_excerpt.to_lowercase())
        });
        if ok {
            passes += 1
        }
        details.push(json!({"query":case.query,"passed":ok}));
    }
    drop(staging);
    let _ = fs::remove_file(&tmp);
    let _ = fs::remove_file(format!("{}-wal", tmp.display()));
    let _ = fs::remove_file(format!("{}-shm", tmp.display()));
    let accuracy = passes as f64 / cases.len() as f64;
    store.evaluation(
        id,
        "quality",
        accuracy >= 0.9,
        accuracy,
        json!({"dataset":dataset.display().to_string(),"accuracy":accuracy}),
    )?;
    Ok(
        json!({"total":cases.len(),"passed":passes,"accuracy":accuracy,"gate_passed":accuracy>=0.9,"details":details,
        "warning":"Retrieval quality only; no neural or safety claims"}),
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
    if candidate.kind != "rag" {
        bail!("Neural candidate evaluation requires a separate neural evaluator")
    };
    let path = Path::new(&candidate.artifact);
    let bytes = read_capped(path, 256_000)?;
    if store::sha256(bytes.as_bytes()) != candidate.checksum {
        bail!("Artifact modified since evaluation")
    }
    let approved = store.promote_rag(id, &bytes)?;
    Ok(serde_json::to_value(approved)?)
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
    use std::path::PathBuf;
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
        let quality = evaluate(&store, &candidate.id, "quality", Some(&bench)).unwrap();
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
