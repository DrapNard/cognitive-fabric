//! Integration and differential coverage for bounded finite-domain partial pruning.
use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    reasoning::finite_search::Settings,
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Clone, Serialize, Deserialize)]
struct Objective {
    variables: Vec<Variable>,
    constraints: Vec<Constraint>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Variable {
    id: String,
    domain: Vec<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Constraint {
    Linear {
        terms: Vec<Term>,
        relation: Relation,
        rhs: i64,
    },
    AllDifferent {
        variables: Vec<String>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
struct Term {
    variable: String,
    coefficient: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Relation {
    Eq,
    Ne,
    Le,
    Ge,
}

struct Solver(Mutex<VecDeque<&'static str>>);

impl Solver {
    fn for_fallback() -> Self {
        Self(Mutex::new(VecDeque::from([r#"{"x":1,"y":0,"z":0}"#])))
    }
}

impl InferenceBackend for Solver {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        assert_eq!(role, "solver");
        let text = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected model call");
        Box::pin(async move {
            Ok(Generation {
                text: text.to_owned(),
                model: "finite-search-pruning-test".to_owned(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

fn setup(max_nodes: usize) -> (Config, Engine, Arc<Store>) {
    let cfg = Config {
        home: std::env::temp_dir().join(format!(
            "fabric-finite-search-pruning-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 8,
        max_cycles: 4,
        max_tokens: 2400,
        max_tool_calls: 8,
        finite_search: Some(Settings { max_nodes }),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::with_backend(
        cfg.clone(),
        store.clone(),
        Arc::new(Solver(Mutex::new(VecDeque::new()))),
    );
    (cfg, engine, store)
}

fn event<'a>(trace: &'a Value, kind: &str) -> &'a Value {
    let matches = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "expected one {kind} event");
    matches[0]
}

fn input(objective: &Objective) -> String {
    format!("csp:{}", serde_json::to_string(objective).unwrap())
}

fn independently_satisfiable(objective: &Objective) -> bool {
    fn visit(objective: &Objective, index: usize, assignment: &mut BTreeMap<String, i64>) -> bool {
        if index < objective.variables.len() {
            let variable = &objective.variables[index];
            for value in &variable.domain {
                assignment.insert(variable.id.clone(), *value);
                if visit(objective, index + 1, assignment) {
                    return true;
                }
            }
            assignment.remove(&variable.id);
            return false;
        }

        objective
            .constraints
            .iter()
            .all(|constraint| match constraint {
                Constraint::Linear {
                    terms,
                    relation,
                    rhs,
                } => {
                    let sum = terms.iter().fold(0_i128, |sum, term| {
                        sum + i128::from(term.coefficient) * i128::from(assignment[&term.variable])
                    });
                    let rhs = i128::from(*rhs);
                    match relation {
                        Relation::Eq => sum == rhs,
                        Relation::Ne => sum != rhs,
                        Relation::Le => sum <= rhs,
                        Relation::Ge => sum >= rhs,
                    }
                }
                Constraint::AllDifferent { variables } => {
                    let mut observed = std::collections::BTreeSet::new();
                    variables
                        .iter()
                        .all(|variable| observed.insert(assignment[variable]))
                }
            })
    }

    visit(objective, 0, &mut BTreeMap::new())
}

fn independently_valid_witness(objective: &Objective, answer: &str) -> bool {
    let Ok(assignment) = serde_json::from_str::<BTreeMap<String, i64>>(answer) else {
        return false;
    };
    if assignment.len() != objective.variables.len()
        || objective.variables.iter().any(|variable| {
            assignment
                .get(&variable.id)
                .is_none_or(|value| !variable.domain.contains(value))
        })
    {
        return false;
    }
    objective
        .constraints
        .iter()
        .all(|constraint| match constraint {
            Constraint::Linear {
                terms,
                relation,
                rhs,
            } => {
                let sum = terms.iter().fold(0_i128, |sum, term| {
                    sum + i128::from(term.coefficient) * i128::from(assignment[&term.variable])
                });
                let rhs = i128::from(*rhs);
                match relation {
                    Relation::Eq => sum == rhs,
                    Relation::Ne => sum != rhs,
                    Relation::Le => sum <= rhs,
                    Relation::Ge => sum >= rhs,
                }
            }
            Constraint::AllDifferent { variables } => {
                let mut observed = std::collections::BTreeSet::new();
                variables
                    .iter()
                    .all(|variable| observed.insert(assignment[variable]))
            }
        })
}

fn legacy_assigned_only_node_count(objective: &Objective) -> usize {
    fn accepts(constraint: &Constraint, assignment: &BTreeMap<String, i64>) -> bool {
        match constraint {
            Constraint::Linear {
                terms,
                relation,
                rhs,
            } => {
                let sum = terms.iter().fold(0_i128, |sum, term| {
                    sum + i128::from(term.coefficient) * i128::from(assignment[&term.variable])
                });
                let rhs = i128::from(*rhs);
                match relation {
                    Relation::Eq => sum == rhs,
                    Relation::Ne => sum != rhs,
                    Relation::Le => sum <= rhs,
                    Relation::Ge => sum >= rhs,
                }
            }
            Constraint::AllDifferent { variables } => {
                let mut observed = std::collections::BTreeSet::new();
                variables
                    .iter()
                    .all(|variable| observed.insert(assignment[variable]))
            }
        }
    }

    fn visit(objective: &Objective, index: usize, assignment: &mut BTreeMap<String, i64>) -> usize {
        let mut visits = 1;
        let fully_assigned_failure = objective.constraints.iter().any(|constraint| {
            let complete = match constraint {
                Constraint::Linear { terms, .. } => terms
                    .iter()
                    .all(|term| assignment.contains_key(&term.variable)),
                Constraint::AllDifferent { variables } => variables
                    .iter()
                    .all(|variable| assignment.contains_key(variable)),
            };
            complete && !accepts(constraint, assignment)
        });
        if fully_assigned_failure || index == objective.variables.len() {
            return visits;
        }

        let variable = &objective.variables[index];
        for value in &variable.domain {
            assignment.insert(variable.id.clone(), *value);
            visits += visit(objective, index + 1, assignment);
            assignment.remove(&variable.id);
        }
        visits
    }

    visit(objective, 0, &mut BTreeMap::new())
}

fn remove_home(cfg: &Config) {
    let _ = std::fs::remove_dir_all(&cfg.home);
}

#[derive(Clone)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, upper: usize) -> usize {
        (self.next() as usize) % upper
    }
}

fn generated_objective(rng: &mut Rng, case_index: usize) -> Objective {
    let variable_count = 1 + rng.below(5);
    let mut variables = Vec::with_capacity(variable_count);
    let source_values = [-19, -7, -2, 0, 3, 8, 23];
    for index in 0..variable_count {
        let domain_size = 2 + rng.below(3);
        let mut candidates = source_values.to_vec();
        for end in (1..candidates.len()).rev() {
            candidates.swap(end, rng.below(end + 1));
        }
        let mut domain = candidates.into_iter().take(domain_size).collect::<Vec<_>>();
        if case_index.is_multiple_of(3) {
            domain.reverse();
        }
        variables.push(Variable {
            id: format!("v{index}"),
            domain,
        });
    }

    let mut constraints = Vec::new();
    let constraint_count = 2 + rng.below(4);
    let coefficients = [-11, -3, -1, 1, 2, 7];
    let relations = [Relation::Eq, Relation::Ne, Relation::Le, Relation::Ge];
    for constraint_index in 0..constraint_count {
        if variable_count >= 2 && (constraint_index + case_index).is_multiple_of(4) {
            let mut selected = (0..variable_count).collect::<Vec<_>>();
            for end in (1..selected.len()).rev() {
                selected.swap(end, rng.below(end + 1));
            }
            let take = 2 + rng.below(variable_count - 1);
            constraints.push(Constraint::AllDifferent {
                variables: selected
                    .into_iter()
                    .take(take)
                    .map(|index| format!("v{index}"))
                    .collect(),
            });
        } else {
            let term_count = 1 + rng.below(variable_count.min(4));
            let mut selected = (0..variable_count).collect::<Vec<_>>();
            for end in (1..selected.len()).rev() {
                selected.swap(end, rng.below(end + 1));
            }
            let terms = selected
                .into_iter()
                .take(term_count)
                .map(|index| Term {
                    variable: format!("v{index}"),
                    coefficient: coefficients[rng.below(coefficients.len())],
                })
                .collect();
            constraints.push(Constraint::Linear {
                terms,
                relation: relations[rng.below(relations.len())].clone(),
                rhs: (rng.below(81) as i64) - 40,
            });
        }
    }
    Objective {
        variables,
        constraints,
    }
}

#[tokio::test]
async fn partial_search_matches_independent_exhaustive_oracle_on_generated_cases() {
    let mut rng = Rng(0xC0FF_EE12_3456_789A);
    let cases = (0..72)
        .map(|index| generated_objective(&mut rng, index))
        .collect::<Vec<_>>();
    let (cfg, engine, store) = setup(4096);

    let mut satisfiable_count = 0;
    let mut unsatisfiable_count = 0;
    for (index, objective) in cases.iter().enumerate() {
        let expected = independently_satisfiable(objective);
        let outcome = engine.run(&input(objective), "fast").await.unwrap();
        let trace = store.task(&outcome.task_id).unwrap();
        let search = event(&trace, "finite_search");
        let reported = search["payload"]["outcome"].as_str().unwrap();
        assert_ne!(reported, "limit_reached", "generated case {index}");
        if expected {
            satisfiable_count += 1;
            assert_eq!(reported, "witness", "generated case {index}");
            assert_eq!(outcome.status, "completed", "generated case {index}");
            assert!(independently_valid_witness(objective, &outcome.answer));
        } else {
            unsatisfiable_count += 1;
            assert_eq!(reported, "unsatisfiable", "generated case {index}");
            assert_eq!(outcome.status, "rejected", "generated case {index}");
            assert_eq!(outcome.stop_reason, "proven_unsatisfiable");
        }
        assert!(search["payload"]["explored_nodes"].as_u64().unwrap() <= 4096);
        assert_eq!(outcome.calls, 0, "generated case {index}");
    }

    assert!(satisfiable_count > 0);
    assert!(unsatisfiable_count > 0);

    remove_home(&cfg);
}

#[tokio::test]
async fn interval_bounds_prune_sparse_signed_impossibility_before_full_assignment() {
    // The interval for x + y spans zero at the root, but once x is selected every
    // remaining y value is too small or too large. Sparse domains make this a
    // useful check that the implementation reasons safely from extrema.
    let objective = Objective {
        variables: vec![
            Variable {
                id: "x".into(),
                domain: vec![-9, -3, 7],
            },
            Variable {
                id: "y".into(),
                domain: vec![-1, 1],
            },
        ],
        constraints: vec![Constraint::Linear {
            terms: vec![
                Term {
                    variable: "x".into(),
                    coefficient: 1,
                },
                Term {
                    variable: "y".into(),
                    coefficient: 1,
                },
            ],
            relation: Relation::Eq,
            rhs: 0,
        }],
    };
    assert!(!independently_satisfiable(&objective));

    let (cfg, engine, store) = setup(4096);
    let outcome = engine.run(&input(&objective), "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = event(&trace, "finite_search");
    let explored = search["payload"]["explored_nodes"].as_u64().unwrap() as usize;
    let legacy_explored = legacy_assigned_only_node_count(&objective);

    assert_eq!(outcome.stop_reason, "proven_unsatisfiable");
    assert_eq!(search["payload"]["outcome"], "unsatisfiable");
    assert_eq!(explored, 4);
    assert_eq!(legacy_explored, 10);
    assert!(explored < legacy_explored);
    remove_home(&cfg);
}

#[tokio::test]
async fn early_duplicate_pruning_proves_pigeonhole_unsatisfiable() {
    let objective = Objective {
        variables: (0..4)
            .map(|index| Variable {
                id: format!("v{index}"),
                domain: vec![0, 1],
            })
            .collect(),
        constraints: vec![Constraint::AllDifferent {
            variables: (0..4).map(|index| format!("v{index}")).collect(),
        }],
    };
    assert!(!independently_satisfiable(&objective));

    let (cfg, engine, store) = setup(4096);
    let outcome = engine.run(&input(&objective), "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = event(&trace, "finite_search");
    let explored = search["payload"]["explored_nodes"].as_u64().unwrap() as usize;
    let legacy_explored = legacy_assigned_only_node_count(&objective);

    assert_eq!(search["payload"]["outcome"], "unsatisfiable");
    assert_eq!(outcome.stop_reason, "proven_unsatisfiable");
    assert_eq!(legacy_explored, 31);
    assert_eq!(explored, 11);
    assert!(explored < legacy_explored);
    remove_home(&cfg);
}

#[tokio::test]
async fn signed_extreme_values_and_negative_coefficients_do_not_overflow_bounds() {
    let objective = Objective {
        variables: vec![
            Variable {
                id: "x".into(),
                domain: vec![i64::MAX, i64::MIN],
            },
            Variable {
                id: "y".into(),
                domain: vec![i64::MAX, i64::MIN],
            },
        ],
        constraints: vec![Constraint::Linear {
            terms: vec![
                Term {
                    variable: "x".into(),
                    coefficient: 1_000_000,
                },
                Term {
                    variable: "y".into(),
                    coefficient: -1_000_000,
                },
            ],
            relation: Relation::Eq,
            rhs: 0,
        }],
    };
    assert!(independently_satisfiable(&objective));

    let (cfg, engine, store) = setup(4096);
    let outcome = engine.run(&input(&objective), "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = event(&trace, "finite_search");

    assert_eq!(search["payload"]["outcome"], "witness");
    assert!(independently_valid_witness(&objective, &outcome.answer));
    remove_home(&cfg);
}

#[tokio::test]
async fn pruned_branches_leave_enough_of_the_node_cap_to_find_a_witness() {
    let objective = Objective {
        variables: ["x", "y", "z"]
            .into_iter()
            .map(|id| Variable {
                id: id.into(),
                domain: vec![0, 1],
            })
            .collect(),
        constraints: vec![Constraint::Linear {
            terms: ["x", "y", "z"]
                .into_iter()
                .map(|variable| Term {
                    variable: variable.into(),
                    coefficient: 1,
                })
                .collect(),
            relation: Relation::Eq,
            rhs: 3,
        }],
    };
    let (cfg, engine, store) = setup(7);
    let outcome = engine.run(&input(&objective), "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = event(&trace, "finite_search");

    assert_eq!(search["payload"]["outcome"], "witness");
    assert_eq!(search["payload"]["explored_nodes"], 7);
    assert_eq!(outcome.calls, 0);
    assert_eq!(outcome.resources.tool_calls, 3); // knowledge search, solve, verify
    let verification = event(&trace, "verification");
    assert_eq!(verification["payload"]["verified"], true);
    assert_eq!(verification["payload"]["origin"], "finite_search");
    assert!(independently_valid_witness(&objective, &outcome.answer));
    remove_home(&cfg);
}

#[tokio::test]
async fn node_cap_still_counts_the_empty_root_and_falls_back_after_one_visit() {
    let objective = Objective {
        variables: ["x", "y", "z"]
            .into_iter()
            .map(|id| Variable {
                id: id.into(),
                domain: vec![0, 1],
            })
            .collect(),
        constraints: vec![Constraint::Linear {
            terms: vec![Term {
                variable: "x".into(),
                coefficient: 1,
            }],
            relation: Relation::Eq,
            rhs: 1,
        }],
    };
    let cfg = Config {
        home: std::env::temp_dir().join(format!(
            "fabric-finite-search-pruning-cap-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        interpret_goals: false,
        max_calls: 8,
        max_cycles: 4,
        max_tokens: 2400,
        max_tool_calls: 8,
        finite_search: Some(Settings { max_nodes: 1 }),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let engine = Engine::with_backend(cfg.clone(), store.clone(), Arc::new(Solver::for_fallback()));
    let outcome = engine.run(&input(&objective), "fast").await.unwrap();
    let trace = store.task(&outcome.task_id).unwrap();
    let search = event(&trace, "finite_search");

    assert_eq!(search["payload"]["outcome"], "limit_reached");
    assert_eq!(search["payload"]["explored_nodes"], 1);
    assert_eq!(outcome.calls, 1);
    assert!(independently_valid_witness(&objective, &outcome.answer));
    remove_home(&cfg);
}
