//! Public finite-domain labels checked by a small independent exhaustive oracle.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    input: String,
    verification: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Objective {
    variables: Vec<Variable>,
    constraints: Vec<Constraint>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variable {
    id: String,
    domain: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Constraint {
    Linear {
        terms: Vec<Term>,
        relation: String,
        rhs: i64,
    },
    AllDifferent {
        variables: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Term {
    variable: String,
    coefficient: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    dataset: String,
    dataset_sha256: String,
    case_count: usize,
    license: String,
    source: String,
    oracle: String,
    author_bias: String,
    independent_human_review: bool,
    held_out: bool,
    training_contamination_checked: bool,
    model_training_use: bool,
    live_model_evaluated: bool,
    live_evidence: String,
    independent_live_study_completed: bool,
    oracle_scope: String,
    cases: Vec<Metadata>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    id: String,
    line: usize,
    objective_sha256: String,
    categories: Vec<String>,
    satisfiable: bool,
    solution_count: usize,
    sample_witnesses: Vec<BTreeMap<String, i64>>,
}

fn enumerate(
    objective: &Objective,
    variable_index: usize,
    assignment: &mut BTreeMap<String, i64>,
    solutions: &mut Vec<BTreeMap<String, i64>>,
) {
    if variable_index < objective.variables.len() {
        let variable = &objective.variables[variable_index];
        for value in &variable.domain {
            assignment.insert(variable.id.clone(), *value);
            enumerate(objective, variable_index + 1, assignment, solutions);
        }
        assignment.remove(&variable.id);
        return;
    }

    for constraint in &objective.constraints {
        let accepted = match constraint {
            Constraint::Linear {
                terms,
                relation,
                rhs,
            } => {
                // Shape checks cap this at 16 terms and |coefficient| <= 1,000,000;
                // multiplying an i64 value then summing 16 terms fits comfortably in i128.
                let sum: i128 = terms
                    .iter()
                    .map(|term| {
                        i128::from(term.coefficient) * i128::from(assignment[&term.variable])
                    })
                    .sum();
                let rhs = i128::from(*rhs);
                match relation.as_str() {
                    "eq" => sum == rhs,
                    "ne" => sum != rhs,
                    "le" => sum <= rhs,
                    "ge" => sum >= rhs,
                    other => panic!("unsupported independent-oracle relation: {other}"),
                }
            }
            Constraint::AllDifferent { variables } => {
                let values: BTreeSet<i64> = variables.iter().map(|id| assignment[id]).collect();
                values.len() == variables.len()
            }
        };
        if !accepted {
            return;
        }
    }
    solutions.push(assignment.clone());
}

fn parse_objective(input: &str) -> Objective {
    let json = input.strip_prefix("csp:").expect("explicit csp objective");
    serde_json::from_str(json).expect("valid independent objective shape")
}

#[test]
fn public_finite_domain_labels_match_exhaustive_i128_oracle() {
    let raw = include_str!("../evaluation/finite_domain_cases.jsonl");
    let manifest: Manifest =
        serde_json::from_str(include_str!("../evaluation/finite_domain_manifest.json"))
            .expect("valid versioned manifest");
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.dataset, "finite_domain_cases.jsonl");
    assert_eq!(
        manifest.dataset_sha256,
        cogfabric::store::sha256(raw.as_bytes())
    );
    assert_eq!(manifest.case_count, 17);
    assert_eq!(manifest.license, "CC0-1.0");
    assert!(manifest.source.contains("Original synthetic"));
    assert!(manifest.oracle.contains("exhaustive"));
    assert!(manifest.author_bias.contains("implementer"));
    assert!(!manifest.independent_human_review);
    assert!(!manifest.held_out);
    assert!(!manifest.training_contamination_checked);
    assert!(!manifest.model_training_use);
    assert!(manifest.live_model_evaluated);
    assert_eq!(
        manifest.live_evidence,
        "results/qwen3-8b-progress-ablation-summary.json"
    );
    assert!(!manifest.independent_live_study_completed);
    assert!(manifest
        .oracle_scope
        .contains("does not measure Jev-R progress-policy benefit"));

    let cases: Vec<Case> = raw
        .lines()
        .map(|line| serde_json::from_str(line).expect("strict comparison row"))
        .collect();
    assert_eq!(cases.len(), manifest.cases.len());
    let mut ids = BTreeSet::new();
    let mut objectives = BTreeSet::new();
    let mut all_categories = BTreeSet::new();
    let mut unsatisfiable = 0;
    let mut multiple_witness_tasks = 0;
    let mut linear_relations = BTreeSet::<String>::new();
    let mut saw_negative_coefficient = false;
    let mut saw_i64_boundary = false;

    for (index, (case, metadata)) in cases.iter().zip(&manifest.cases).enumerate() {
        assert_eq!(case.verification, "finite_domain_v1");
        assert_eq!(metadata.line, index + 1);
        assert!(ids.insert(&metadata.id), "duplicate id {}", metadata.id);
        assert!(objectives.insert(&case.input), "duplicate objective");
        assert_eq!(
            metadata.objective_sha256,
            cogfabric::store::sha256(case.input.as_bytes())
        );
        for category in &metadata.categories {
            all_categories.insert(category.as_str());
        }
        let objective = parse_objective(&case.input);
        assert!((1..=4).contains(&objective.variables.len()));
        assert!((1..=128).contains(&objective.constraints.len()));
        let mut variable_ids = BTreeSet::new();
        for variable in &objective.variables {
            assert!(!variable.id.is_empty() && variable.id.len() <= 32);
            assert!(variable
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
            assert!(variable_ids.insert(variable.id.as_str()));
            assert!((1..=4).contains(&variable.domain.len()));
            assert_eq!(
                variable.domain.iter().collect::<BTreeSet<_>>().len(),
                variable.domain.len()
            );
            if variable.domain.contains(&i64::MIN) || variable.domain.contains(&i64::MAX) {
                saw_i64_boundary = true;
            }
        }
        for constraint in &objective.constraints {
            match constraint {
                Constraint::Linear {
                    terms, relation, ..
                } => {
                    assert!((1..=16).contains(&terms.len()));
                    assert!(matches!(relation.as_str(), "eq" | "ne" | "le" | "ge"));
                    linear_relations.insert(relation.clone());
                    let mut term_variables = BTreeSet::new();
                    for term in terms {
                        assert!(variable_ids.contains(term.variable.as_str()));
                        assert!(term_variables.insert(term.variable.as_str()));
                        assert_ne!(term.coefficient, 0);
                        assert!((-1_000_000..=1_000_000).contains(&term.coefficient));
                        saw_negative_coefficient |= term.coefficient < 0;
                    }
                }
                Constraint::AllDifferent { variables } => {
                    assert!((2..=32).contains(&variables.len()));
                    let mut distinct_variables = BTreeSet::new();
                    for variable in variables {
                        assert!(variable_ids.contains(variable.as_str()));
                        assert!(distinct_variables.insert(variable.as_str()));
                    }
                }
            }
        }

        let mut solutions = Vec::new();
        enumerate(&objective, 0, &mut BTreeMap::new(), &mut solutions);
        assert_eq!(
            metadata.satisfiable,
            !solutions.is_empty(),
            "{}",
            metadata.id
        );
        assert_eq!(metadata.solution_count, solutions.len(), "{}", metadata.id);
        assert!(metadata.sample_witnesses.len() <= 3);
        assert_eq!(
            metadata.sample_witnesses,
            solutions.iter().take(3).cloned().collect::<Vec<_>>()
        );
        for witness in &metadata.sample_witnesses {
            assert!(
                solutions.contains(witness),
                "invalid sample witness for {}",
                metadata.id
            );
        }
        if solutions.is_empty() {
            unsatisfiable += 1;
        }
        if solutions.len() > 1 {
            multiple_witness_tasks += 1;
        }
    }
    assert!(all_categories.contains("linear_eq"));
    assert!(all_categories.contains("linear_ne"));
    assert!(all_categories.contains("linear_le"));
    assert!(all_categories.contains("linear_ge"));
    assert!(all_categories.contains("negative_coefficient"));
    assert!(all_categories.contains("all_different"));
    assert!(all_categories.contains("i64_boundary"));
    assert!(all_categories.contains("unsatisfiable"));
    assert!(all_categories.contains("multiple_witnesses"));
    assert_eq!(
        linear_relations,
        BTreeSet::from([
            "eq".to_owned(),
            "ne".to_owned(),
            "le".to_owned(),
            "ge".to_owned()
        ])
    );
    assert!(saw_negative_coefficient && saw_i64_boundary);
    assert!(unsatisfiable >= 3);
    assert!(multiple_witness_tasks >= 3);
}

// Separate flat assignment parser: no production parser or verifier is invoked.
struct Assignment(BTreeMap<String, i64>);
impl<'de> Deserialize<'de> for Assignment {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Flat;
        impl<'de> serde::de::Visitor<'de> for Flat {
            type Value = Assignment;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("one flat integer assignment")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Assignment, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, i64>()? {
                    if values.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate assignment variable"));
                    }
                }
                Ok(Assignment(values))
            }
        }
        deserializer.deserialize_map(Flat)
    }
}

#[test]
fn retained_qwen_progress_pairs_match_the_separate_exhaustive_oracle() {
    use serde_json::Value;
    let raw = include_str!("../evaluation/results/qwen3-8b-progress-ablation.json");
    let report: Value = serde_json::from_str(raw).unwrap();
    let summary: Value = serde_json::from_str(include_str!(
        "../evaluation/results/qwen3-8b-progress-ablation-summary.json"
    ))
    .unwrap();
    let dataset = include_str!("../evaluation/finite_domain_cases.jsonl");
    assert_eq!(
        summary["raw_report_sha256"],
        cogfabric::store::sha256(raw.as_bytes())
    );
    assert_eq!(
        report["dataset_sha256"],
        cogfabric::store::sha256(dataset.as_bytes())
    );
    assert_eq!(
        report["code_revision_operator_attested"],
        "08f621797cca1b6eafd682038a5c4ff6833e10f1"
    );
    assert_eq!(report["complete"], true);
    assert_eq!(report["complete_cases"], 17);
    assert_eq!(report["held_out_operator_attested"], false);
    assert_eq!(report["content_included"], true);
    assert_eq!(summary["provider_weights_attested"], false);
    assert_eq!(summary["independent_human_review"], false);
    let cases: Vec<Case> = dataset
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let rows = report["cases"].as_array().unwrap();
    assert_eq!(rows.len(), cases.len());
    let mut passes = [0u64; 2];
    let mut calls = [0u64; 2];
    let mut latency = [0.0; 2];
    let mut verified = 0;
    for (index, (case, row)) in cases.iter().zip(rows).enumerate() {
        assert_eq!(row["case_index"], index);
        assert_eq!(row["input"], case.input);
        assert_eq!(row["paired"], true);
        let objective = parse_objective(&case.input);
        let mut solutions = Vec::new();
        enumerate(&objective, 0, &mut BTreeMap::new(), &mut solutions);
        assert_eq!(
            summary["cases"][index]["satisfiable"],
            !solutions.is_empty()
        );
        for (arm, label) in ["baseline", "progress_stopping"].iter().enumerate() {
            let result = &row[*label];
            let answer = result["answer"].as_str().unwrap();
            assert!(answer.len() <= 8192);
            assert_eq!(result["answer_truncated"], false);
            assert_eq!(
                result["answer_sha256"],
                cogfabric::store::sha256(answer.as_bytes())
            );
            let assignment = serde_json::from_str::<Assignment>(answer).ok();
            let eligible = matches!(result["status"].as_str(), Some("completed" | "unverified"));
            let expected = eligible && assignment.is_some_and(|v| solutions.contains(&v.0));
            assert_eq!(
                result["passed_independent_finite_domain_check"], expected,
                "case {index} arm {arm}"
            );
            assert_eq!(row[format!("{label}_passed")], expected);
            passes[arm] += u64::from(expected);
            let count = result["calls"].as_u64().unwrap();
            calls[arm] += count;
            latency[arm] += result["elapsed_ms"].as_f64().unwrap();
            assert_eq!(result["resources"]["calls"], count);
            assert_eq!(summary["cases"][index][*label]["calls"], count);
            assert_eq!(summary["cases"][index][*label]["passed"], expected);
            verified += 1;
        }
    }
    assert_eq!(verified, 34);
    assert_eq!(passes, [14, 13]);
    assert_eq!(calls, [85, 78]);
    for arm in 0..2 {
        assert_eq!(report["arms"][arm]["passed"], passes[arm]);
        assert_eq!(report["arms"][arm]["resources"]["model_calls"], calls[arm]);
        assert!(
            (report["arms"][arm]["resources"]["latency_ms"]
                .as_f64()
                .unwrap()
                - latency[arm])
                .abs()
                < 0.001
        );
    }
    let stopped: Vec<usize> = summary["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            c["progress_stopping"]["progress_assessments"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["stop"] == true)
                .then_some(i)
        })
        .collect();
    assert_eq!(stopped, vec![16]);
    assert_eq!(summary["cases"][16]["satisfiable"], false);
    assert_eq!(
        report["paired_outcomes"]["progress_stopping_regressions"],
        serde_json::json!([4])
    );
    assert!(
        summary["cases"][4]["progress_stopping"]["progress_assessments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["stop"] == false)
    );
}
