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
    assert_eq!(manifest["live_model_evaluation_complete"], true);
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
    check_advice_report(
        include_str!("../evaluation/results/qwen3-8b-advice-diagnostics-four-cases.json"),
        include_str!("../evaluation/results/qwen3-8b-advice-diagnostics-four-cases-summary.json"),
        4,
    );
}

#[test]
fn frozen_concise_advice_matches_independent_scores_and_diagnostics() {
    let raw = include_str!("../evaluation/results/qwen3-8b-concise-advice-four-cases.json");
    check_advice_report(
        raw,
        include_str!("../evaluation/results/qwen3-8b-concise-advice-four-cases-summary.json"),
        4,
    );
    let report: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["arms"][5]["passed"], 4);
    for detail in report["details"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["profile"] == "experts")
    {
        assert_eq!(detail["outcome"]["status"], "completed");
        assert_eq!(detail["outcome"]["calls"], 3);
        assert_eq!(detail["expert_advice"]["phases_started"], 1);
        assert_eq!(detail["expert_advice"]["fallback_events"], 0);
    }
}

#[test]
fn full_live_concise_report_matches_independent_oracle_and_resources() {
    let raw = include_str!("../evaluation/results/qwen3-8b-concise-advice-full-corpus.json");
    check_advice_report(
        raw,
        include_str!("../evaluation/results/qwen3-8b-concise-advice-full-corpus-summary.json"),
        16,
    );
    let report: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(report["complete"], true);
    assert_eq!(report["details"].as_array().unwrap().len(), 96);
    let experts = &report["arms"][5];
    assert_eq!(experts["passed"], 12);
    assert_eq!(experts["resources"]["model_calls"], 65);
    for detail in report["details"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["profile"] == "experts")
    {
        assert_eq!(detail["expert_advice"]["phases_started"], 1);
        assert_eq!(detail["expert_advice"]["fallback_events"], 0);
    }
}

#[test]
fn full_research_report_matches_independent_scores_and_all_phase_totals() {
    let raw = include_str!("../evaluation/results/qwen3-8b-research-phase-full-corpus.json");
    check_advice_report(
        raw,
        include_str!("../evaluation/results/qwen3-8b-research-phase-full-corpus-summary.json"),
        16,
    );
    let report: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(report["mode"], "research");
    assert_eq!(report["complete"], true);
    assert_eq!(report["arms"][5]["passed"], 16);
    assert_eq!(report["arms"][5]["resources"]["model_calls"], 69);
    let mut planning_fallbacks = 0;
    for detail in report["details"].as_array().unwrap() {
        planning_fallbacks += detail["planning_advice"]["inference_timeout_fallbacks"]
            .as_u64()
            .unwrap();
        assert_eq!(detail["exploration_advice"]["fallback_events"], 0);
        assert_eq!(detail["expert_advice"]["fallback_events"], 0);
        assert!(detail["outcome"]["elapsed_ms"].as_f64().unwrap() < 30000.0);
        if detail["profile"] == "experts" {
            assert_eq!(detail["outcome"]["status"], "completed");
        }
    }
    assert_eq!(planning_fallbacks, 24);
}

fn check_advice_report(raw: &str, summary_raw: &str, case_count: usize) {
    let report: serde_json::Value = serde_json::from_str(raw).unwrap();
    let summary: serde_json::Value = serde_json::from_str(summary_raw).unwrap();
    assert_eq!(
        summary["report_sha256"],
        cogfabric::store::sha256(raw.as_bytes())
    );
    assert_eq!(summary["held_out"], false);
    assert_eq!(summary["independent_human_review"], false);
    assert_eq!(summary["weights_attested"], false);
    assert_eq!(report["held_out_run"], false);
    assert_eq!(report["case_count"], case_count);
    assert_eq!(
        report["code_revision_operator_attested"],
        summary["runtime_source_revision"]
    );
    let dataset = include_str!("../evaluation/shortest_path_cases.jsonl")
        .lines()
        .take(case_count)
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
    assert_eq!(summary["complete"], report["complete"]);
    assert_eq!(summary["stop_reason"], report["stop_reason"]);
    if report["complete"] == true {
        assert_eq!(details.len(), case_count * 6);
    }
    for detail in details {
        let index = detail["case_index"].as_u64().unwrap() as usize;
        let arm = detail["arm_index"].as_u64().unwrap() as usize;
        assert!(index < case_count && arm < 6 && seen.insert((index, arm)));
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
        let tools: u64 = rows
            .iter()
            .map(|d| d["outcome"]["resources"]["tool_calls"].as_u64().unwrap())
            .sum();
        assert_eq!(arm["resources"]["tool_calls"], tools);
        assert_eq!(summary["arms"][index]["tool_calls"], tools);
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
            let diagnostic = &summary["arms"][index]["diagnostics"];
            if diagnostic.get("expert_advice").is_some() {
                assert_eq!(diagnostic["expert_advice"][metric], count);
                for stage in ["expert_advice", "planning_advice", "exploration_advice"] {
                    let count: u64 = rows
                        .iter()
                        .map(|d| d[stage][metric].as_u64().unwrap())
                        .sum();
                    assert_eq!(diagnostic[stage][metric], count);
                    assert_eq!(arm["resources"][stage][metric], count);
                }
            } else {
                assert_eq!(diagnostic[metric], count);
            }
        }
    }
}

#[test]
fn portfolio_public_runs_match_independent_scores_hashes_and_diagnostic_audits() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dataset = std::fs::read(root.join("evaluation/portfolio_cases.jsonl")).unwrap();
    let original = include_str!("../evaluation/shortest_path_cases.jsonl")
        .lines()
        .collect::<Vec<_>>();
    let expected = [0, 5, 11, 15]
        .iter()
        .map(|i| format!("{}\n", original[*i]))
        .collect::<String>();
    assert_eq!(dataset, expected.as_bytes());
    let summary: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("evaluation/results/qwen3-8b-portfolio-public-summary.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        summary["dataset_sha256"],
        cogfabric::store::sha256(&dataset)
    );
    for flag in [
        "held_out",
        "independent_human_review",
        "weights_attested",
        "physical_provider_stop_proven",
        "causal_before_after_comparison",
    ] {
        assert_eq!(summary[flag], false);
    }
    let metrics = [
        "accepted_portfolio_events",
        "legacy_advice_events",
        "invalid_portfolio_events",
        "selected_hypothesis_events",
        "exhausted_selection_events",
        "unclassified_events",
    ];
    for run in summary["runs"].as_array().unwrap() {
        let raw = std::fs::read(
            root.join("evaluation/results")
                .join(run["report_file"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(run["report_sha256"], cogfabric::store::sha256(&raw));
        let report: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(
            report["code_revision_operator_attested"],
            run["runtime_source_revision"]
        );
        assert_eq!(report["dataset_checksum"], summary["dataset_sha256"]);
        assert_eq!(report["held_out_run"], false);
        assert_eq!(report["complete"], true);
        assert_eq!(report["case_count"], 4);
        let rows = report["details"].as_array().unwrap();
        assert_eq!(rows.len(), 24);
        let mut seen = BTreeSet::new();
        for row in rows {
            assert!(seen.insert((
                row["case_index"].as_u64().unwrap(),
                row["arm_index"].as_u64().unwrap()
            )));
            let graph: Graph = serde_json::from_str(
                row["input"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("path:")
                    .unwrap(),
            )
            .unwrap();
            let outcome = &row["outcome"];
            let eligible = matches!(outcome["status"].as_str(), Some("completed" | "unverified"));
            assert_eq!(
                row["passed"],
                eligible && oracle_accepts(&graph, outcome["answer"].as_str().unwrap())
            );
            assert!(outcome["calls"].as_u64().unwrap() <= 10);
            assert!(
                outcome["resources"]["reserved_output_tokens"]
                    .as_u64()
                    .unwrap()
                    <= 3000
            );
            let audit = run["hypothesis_audits"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["task_id"] == outcome["task_id"])
                .unwrap();
            let mut counts = metrics
                .iter()
                .map(|m| ((*m).to_string(), 0u64))
                .collect::<std::collections::BTreeMap<_, _>>();
            for event in audit["events"].as_array().unwrap() {
                assert_eq!(event["source"], "jev-r");
                let v = &event["payload"];
                assert_eq!(v["verified"], false);
                assert_eq!(v["permissions_granted"], false);
                let key = match event["kind"].as_str().unwrap() {
                    "exploration_portfolio" => match v["status"].as_str() {
                        Some("legacy_advice") => "legacy_advice_events",
                        Some("invalid_portfolio") => "invalid_portfolio_events",
                        None => {
                            assert_eq!(v["policy"], "canonical-alternatives/v1");
                            "accepted_portfolio_events"
                        }
                        _ => "unclassified_events",
                    },
                    "hypothesis_selection" => {
                        assert_eq!(v["policy"], "canonical-alternatives/v1");
                        if v["portfolio_exhausted"] == true {
                            "exhausted_selection_events"
                        } else {
                            "selected_hypothesis_events"
                        }
                    }
                    _ => panic!("Unexpected exported audit kind"),
                };
                *counts.get_mut(key).unwrap() += 1;
            }
            if let Some(diagnostics) = row.get("hypothesis_diagnostics") {
                for (key, value) in counts {
                    assert_eq!(diagnostics[key], value);
                }
            }
        }
        for (index, arm) in report["arms"].as_array().unwrap().iter().enumerate() {
            let selected = rows
                .iter()
                .filter(|r| r["arm_index"] == index)
                .collect::<Vec<_>>();
            let saved = &run["arms"][index];
            assert_eq!(selected.len(), 4);
            assert_eq!(
                arm["passed"],
                selected.iter().filter(|r| r["passed"] == true).count()
            );
            assert_eq!(saved["passed"], arm["passed"]);
            assert_eq!(saved["model_calls"], arm["resources"]["model_calls"]);
            let calls: u64 = selected
                .iter()
                .map(|r| r["outcome"]["calls"].as_u64().unwrap())
                .sum();
            assert_eq!(arm["resources"]["model_calls"], calls);
            let latency: f64 = selected
                .iter()
                .map(|r| r["outcome"]["elapsed_ms"].as_f64().unwrap())
                .sum();
            assert!((latency - arm["resources"]["latency_ms"].as_f64().unwrap()).abs() < 0.001);
            assert!((latency - saved["latency_ms"].as_f64().unwrap()).abs() < 0.001);
            if arm["resources"].get("hypothesis_diagnostics").is_some() {
                for metric in metrics {
                    let total: u64 = selected
                        .iter()
                        .map(|r| r["hypothesis_diagnostics"][metric].as_u64().unwrap())
                        .sum();
                    assert_eq!(arm["resources"]["hypothesis_diagnostics"][metric], total);
                }
                for key in ["runtime_sources_sha256", "compiled_role_prompts_sha256"] {
                    assert_eq!(report[key], run[key]);
                    assert_eq!(report[key].as_str().unwrap().len(), 64);
                }
            }
        }
    }
}
