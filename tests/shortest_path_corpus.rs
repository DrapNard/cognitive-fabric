//! Frozen public graph labels use exhaustive enumeration, separately from runtime BFS.
use serde::Deserialize;
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    verification: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    nodes: Vec<String>,
    edges: Vec<[String; 2]>,
    start: String,
    goal: String,
}
fn enumerate(graph: &Graph, path: Vec<String>, found: &mut Vec<Vec<String>>) {
    if path.last() == Some(&graph.goal) {
        found.push(path);
        return;
    }
    for [from, to] in &graph.edges {
        if path.last() == Some(from) && !path.contains(to) {
            let mut next = path.clone();
            next.push(to.clone());
            enumerate(graph, next, found);
        }
    }
}
#[test]
fn public_graph_labels_match_independent_simple_path_enumeration() {
    let raw = include_str!("../evaluation/shortest_path_cases.jsonl");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../evaluation/shortest_path_manifest.json")).unwrap();
    assert_eq!(
        manifest["dataset_sha256"],
        cogfabric::store::sha256(raw.as_bytes())
    );
    assert_eq!(manifest["case_count"], 16);
    for field in [
        "held_out",
        "independent_human_review",
        "training_contamination_checked",
        "independent_live_study_completed",
    ] {
        assert_eq!(manifest[field], false);
    }
    assert_eq!(manifest["live_model_evaluated"], true);
    assert_eq!(manifest["live_model_evaluation_complete"], false);
    let cases: Vec<Case> = raw
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let metadata = manifest["cases"].as_array().unwrap();
    assert_eq!(cases.len(), metadata.len());
    let mut ids = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    let mut unreachable = 0;
    let mut identities = 0;
    let mut alternatives = 0;
    for (index, (case, meta)) in cases.iter().zip(metadata).enumerate() {
        assert_eq!(case.verification, "shortest_path_v1");
        assert_eq!(meta["line"], index + 1);
        assert!(ids.insert(meta["id"].as_str().unwrap()));
        assert!(inputs.insert(&case.input));
        assert_eq!(
            meta["objective_sha256"],
            cogfabric::store::sha256(case.input.as_bytes())
        );
        let graph: Graph = serde_json::from_str(case.input.strip_prefix("path:").unwrap()).unwrap();
        // This intentionally small diagnostic oracle has a tighter bound than runtime graphs.
        assert!(graph.nodes.len() <= 6);
        assert!(graph.edges.len() <= 8);
        let mut paths = Vec::new();
        enumerate(&graph, vec![graph.start.clone()], &mut paths);
        let minimum = paths.iter().map(|path| path.len() - 1).min();
        assert_eq!(
            meta["minimum_edges"],
            serde_json::to_value(minimum).unwrap()
        );
        let supplied: Vec<String> =
            serde_json::from_value(meta["one_optimal_path"].clone()).unwrap();
        if let Some(distance) = minimum {
            assert!(paths.contains(&supplied));
            assert_eq!(supplied.len(), distance + 1);
            if paths
                .iter()
                .filter(|path| path.len() == distance + 1)
                .count()
                > 1
            {
                alternatives += 1;
            }
        } else {
            unreachable += 1;
            assert!(supplied.is_empty());
        }
        if graph.start == graph.goal {
            identities += 1;
        }
    }
    assert_eq!(unreachable, 4);
    assert_eq!(identities, 2);
    assert!(alternatives >= 2);
}

fn oracle_accepts(graph: &Graph, answer: &str) -> bool {
    let Ok(actual) = serde_json::from_str::<Vec<String>>(answer) else {
        return false;
    };
    let mut paths = Vec::new();
    enumerate(graph, vec![graph.start.clone()], &mut paths);
    let Some(minimum) = paths.iter().map(Vec::len).min() else {
        return actual.is_empty();
    };
    actual.len() == minimum && paths.contains(&actual)
}

#[test]
fn frozen_partial_live_report_matches_independent_oracle_and_resource_totals() {
    let raw = include_str!("../evaluation/results/qwen3-8b-shortest-path-deep-partial.json");
    let report: serde_json::Value = serde_json::from_str(raw).unwrap();
    let summary: serde_json::Value = serde_json::from_str(include_str!(
        "../evaluation/results/qwen3-8b-shortest-path-deep-partial-summary.json"
    ))
    .unwrap();
    assert_eq!(
        summary["report_sha256"],
        cogfabric::store::sha256(raw.as_bytes())
    );
    assert_eq!(
        summary["workflow_file_sha256"],
        cogfabric::store::sha256(include_bytes!("../evaluation/shortest_path_workflow.json"))
    );
    assert_eq!(
        report["dataset_checksum"],
        cogfabric::store::sha256(include_bytes!("../evaluation/shortest_path_cases.jsonl"))
    );
    assert_eq!(report["complete"], false);
    assert_eq!(report["stop_reason"], "run_deadline");
    assert_eq!(report["held_out_run"], false);
    assert_eq!(summary["held_out"], false);
    assert_eq!(summary["weights_attested"], false);
    assert_eq!(summary["independent_human_review"], false);
    assert_eq!(
        report["code_revision_operator_attested"],
        summary["runtime_source_revision"]
    );
    let cases: Vec<Case> = include_str!("../evaluation/shortest_path_cases.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let details = report["details"].as_array().unwrap();
    assert_eq!(
        details.len() as u64,
        summary["executed_tasks"].as_u64().unwrap()
    );
    assert_eq!(
        summary["planned_tasks"].as_u64().unwrap(),
        summary["executed_tasks"].as_u64().unwrap() + summary["unexecuted_tasks"].as_u64().unwrap()
    );
    let mut identities = BTreeSet::new();
    for detail in details {
        let index = detail["case_index"].as_u64().unwrap() as usize;
        let arm = detail["arm_index"].as_u64().unwrap() as usize;
        assert!(index < cases.len() && arm < 6);
        assert!(identities.insert((index, arm)));
        assert_eq!(detail["input"], cases[index].input);
        assert_eq!(detail["verification"], "shortest_path_v1");
        assert!(detail["expected"].is_null());
        let graph: Graph =
            serde_json::from_str(cases[index].input.strip_prefix("path:").unwrap()).unwrap();
        let outcome = &detail["outcome"];
        let eligible = matches!(outcome["status"].as_str(), Some("completed" | "unverified"));
        let valid = oracle_accepts(&graph, outcome["answer"].as_str().unwrap());
        assert_eq!(detail["passed"], eligible && valid);
        if outcome["status"] == "completed" {
            assert!(valid, "Invalid path marked completed");
        }
    }
    for (index, arm) in report["arms"].as_array().unwrap().iter().enumerate() {
        let rows: Vec<_> = details.iter().filter(|d| d["arm_index"] == index).collect();
        let matched: Vec<_> = rows
            .iter()
            .filter(|d| {
                summary["common_completed_case_indices"]
                    .as_array()
                    .unwrap()
                    .contains(&d["case_index"])
            })
            .collect();
        let passed = rows.iter().filter(|d| d["passed"] == true).count();
        assert_eq!(arm["executed"], rows.len());
        assert_eq!(arm["passed"], passed);
        assert_eq!(summary["arms"][index]["executed"], rows.len());
        assert_eq!(summary["arms"][index]["passed"], passed);
        assert_eq!(summary["arms"][index]["common_cases"], matched.len());
        assert_eq!(
            summary["arms"][index]["passed_common_cases"],
            matched.iter().filter(|d| d["passed"] == true).count()
        );
        let calls: u64 = rows
            .iter()
            .map(|d| d["outcome"]["calls"].as_u64().unwrap())
            .sum();
        let tools: u64 = rows
            .iter()
            .map(|d| d["outcome"]["resources"]["tool_calls"].as_u64().unwrap())
            .sum();
        assert_eq!(arm["resources"]["model_calls"], calls);
        assert_eq!(summary["arms"][index]["model_calls"], calls);
        assert_eq!(arm["resources"]["tool_calls"], tools);
        assert_eq!(summary["arms"][index]["tool_calls"], tools);
    }
    let workflow: cogfabric::registry::Manifest =
        serde_json::from_str(include_str!("../evaluation/shortest_path_workflow.json")).unwrap();
    workflow.validate().unwrap();
}

#[test]
fn frozen_advice_diagnostics_match_paths_scores_and_charged_calls() {
    let raw = include_str!("../evaluation/results/qwen3-8b-advice-diagnostics-four-cases.json");
    let report: serde_json::Value = serde_json::from_str(raw).unwrap();
    let summary: serde_json::Value = serde_json::from_str(include_str!(
        "../evaluation/results/qwen3-8b-advice-diagnostics-four-cases-summary.json"
    ))
    .unwrap();
    assert_eq!(
        summary["report_sha256"],
        cogfabric::store::sha256(raw.as_bytes())
    );
    assert_eq!(summary["held_out"], false);
    assert_eq!(summary["independent_human_review"], false);
    assert_eq!(summary["weights_attested"], false);
    assert_eq!(report["held_out_run"], false);
    assert_eq!(report["case_count"], 4);
    assert_eq!(
        report["code_revision_operator_attested"],
        summary["runtime_source_revision"]
    );
    let dataset = include_str!("../evaluation/shortest_path_cases.jsonl")
        .lines()
        .take(4)
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert_eq!(
        report["dataset_checksum"],
        cogfabric::store::sha256(dataset.as_bytes())
    );
    let cases: Vec<Case> = dataset
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let details = report["details"].as_array().unwrap();
    let mut seen = BTreeSet::new();
    assert_eq!(summary["executed_tasks"], details.len());
    for detail in details {
        let index = detail["case_index"].as_u64().unwrap() as usize;
        let arm = detail["arm_index"].as_u64().unwrap() as usize;
        assert!(index < 4 && arm < 6 && seen.insert((index, arm)));
        assert_eq!(detail["input"], cases[index].input);
        let graph: Graph =
            serde_json::from_str(cases[index].input.strip_prefix("path:").unwrap()).unwrap();
        let outcome = &detail["outcome"];
        let valid = oracle_accepts(&graph, outcome["answer"].as_str().unwrap());
        let eligible = matches!(outcome["status"].as_str(), Some("completed" | "unverified"));
        assert_eq!(detail["passed"], eligible && valid);
        if outcome["status"] == "completed" {
            assert!(valid);
        }
    }
    for (index, arm) in report["arms"].as_array().unwrap().iter().enumerate() {
        let rows: Vec<_> = details.iter().filter(|d| d["arm_index"] == index).collect();
        assert_eq!(arm["executed"], rows.len());
        assert_eq!(
            arm["passed"],
            rows.iter().filter(|d| d["passed"] == true).count()
        );
        let calls: u64 = rows
            .iter()
            .map(|d| d["outcome"]["calls"].as_u64().unwrap())
            .sum();
        assert_eq!(arm["resources"]["model_calls"], calls);
        assert_eq!(summary["arms"][index]["model_calls"], calls);
        for metric in [
            "phases_started",
            "fallback_events",
            "inference_timeout_fallbacks",
            "unclassified_fallbacks",
        ] {
            let count: u64 = rows
                .iter()
                .map(|d| d["expert_advice"][metric].as_u64().unwrap())
                .sum();
            assert_eq!(summary["arms"][index]["diagnostics"][metric], count);
        }
    }
}
