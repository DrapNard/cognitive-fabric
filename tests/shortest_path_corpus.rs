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
        "live_model_evaluated",
        "independent_live_study_completed",
    ] {
        assert_eq!(manifest[field], false);
    }
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
