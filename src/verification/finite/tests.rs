use super::*;
use serde_json::json;
fn sample() -> serde_json::Value {
    json!({"variables":[{"id":"a","domain":[1,2,3]},{"id":"b","domain":[1,2,3]}],"constraints":[{"kind":"all_different","variables":["a","b"]},{"kind":"linear","terms":[{"variable":"a","coefficient":1},{"variable":"b","coefficient":1}],"relation":"eq","rhs":4}]})
}
#[test]
fn checks_every_domain_and_constraint_without_assuming_a_unique_solution() {
    let c = FiniteDomain::parse(&sample().to_string()).unwrap();
    c.check(r#"{"a":1,"b":3}"#).unwrap();
    c.check(r#"{"b":1,"a":3}"#).unwrap();
    for bad in [
        r#"{"a":2,"b":2}"#,
        r#"{"a":1,"b":2}"#,
        r#"{"a":0,"b":4}"#,
        r#"{"a":1}"#,
        r#"{"a":1,"b":3,"verified":1}"#,
        r#"{"a":1,"a":1,"b":3}"#,
        r#"{"a":1.0,"b":3}"#,
        r#"{"a":true,"b":3}"#,
        r#"{"a":1,"b":null}"#,
        "[]",
        "null",
        "unsatisfiable",
    ] {
        assert!(c.check(bad).is_err(), "{bad}");
    }
    let mut impossible = sample();
    impossible["constraints"][1]["rhs"] = json!(9);
    let c = FiniteDomain::parse(&impossible.to_string()).unwrap();
    for a in 1..=3 {
        for b in 1..=3 {
            assert!(c.check(&json!({"a":a,"b":b}).to_string()).is_err());
        }
    }
}
#[test]
fn rejects_ambiguous_unknown_and_excessive_criteria() {
    for variant in 0..15 {
        let mut v = sample();
        match variant {
            0 => v["variables"] = json!([]),
            1 => v["variables"][0]["id"] = json!("../escape"),
            2 => v["variables"][1]["id"] = json!("a"),
            3 => v["variables"][0]["domain"] = json!([]),
            4 => v["variables"][0]["domain"] = json!([1, 1]),
            5 => v["constraints"] = json!([]),
            6 => v["constraints"][0]["variables"] = json!(["a", "missing"]),
            7 => v["constraints"][0]["variables"] = json!(["a", "a"]),
            8 => v["constraints"][1]["terms"][0]["coefficient"] = json!(0),
            9 => v["constraints"][1]["terms"][0]["coefficient"] = json!(1_000_001),
            10 => v["constraints"][1]["terms"][1]["variable"] = json!("a"),
            11 => v["constraints"][1]["relation"] = json!("execute"),
            12 => v["constraints"][0]["permission"] = json!("shell"),
            13 => v["variables"][0]["domain"] = json!((0..65).collect::<Vec<_>>()),
            _ => v["constraints"] = json!(vec![sample()["constraints"][0].clone(); 129]),
        }
        assert!(
            FiniteDomain::parse(&v.to_string()).is_err(),
            "variant {variant}"
        );
    }
    assert!(FiniteDomain::parse(r#"{"variables":[],"variables":[],"constraints":[]}"#).is_err());
    assert!(FiniteDomain::parse(&" ".repeat(10_001)).is_err());
}
#[test]
fn exact_wide_arithmetic_matches_independent_small_domain_enumeration() {
    for relation in ["eq", "ne", "le", "ge"] {
        for rhs in -4..=4 {
            let v = json!({"variables":[{"id":"x","domain":[-2,-1,0,1,2]},{"id":"y","domain":[-2,-1,0,1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":2},{"variable":"y","coefficient":-3}],"relation":relation,"rhs":rhs}]});
            let c = FiniteDomain::parse(&v.to_string()).unwrap();
            for x in -2..=2 {
                for y in -2..=2 {
                    let sum = 2 * x - 3 * y;
                    let expected = match relation {
                        "eq" => sum == rhs,
                        "ne" => sum != rhs,
                        "le" => sum <= rhs,
                        _ => sum >= rhs,
                    };
                    assert_eq!(c.check(&json!({"x":x,"y":y}).to_string()).is_ok(), expected);
                }
            }
        }
    }
    let v = json!({"variables":[{"id":"x","domain":[i64::MIN,i64::MAX]},{"id":"y","domain":[i64::MIN,i64::MAX]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1_000_000},{"variable":"y","coefficient":-1_000_000}],"relation":"eq","rhs":0}]});
    let c = FiniteDomain::parse(&v.to_string()).unwrap();
    c.check(&json!({"x":i64::MIN,"y":i64::MIN}).to_string())
        .unwrap();
    assert!(c
        .check(&json!({"x":i64::MAX,"y":i64::MIN}).to_string())
        .is_err());
}

#[test]
fn accepts_exact_structural_bounds_and_is_invariant_to_input_order() {
    let variables = (0..32)
        .map(|i| json!({"id":format!("v{i}"),"domain":[i]}))
        .collect::<Vec<_>>();
    let constraints = vec![json!({"kind":"all_different","variables":["v0","v31"]}); 128];
    let mut v = json!({"variables":variables,"constraints":constraints});
    let assignment = (0..32)
        .map(|i| (format!("v{i}"), json!(i)))
        .collect::<serde_json::Map<_, _>>();
    let answer = serde_json::Value::Object(assignment).to_string();
    FiniteDomain::parse(&v.to_string())
        .unwrap()
        .check(&answer)
        .unwrap();
    v["variables"].as_array_mut().unwrap().reverse();
    FiniteDomain::parse(&v.to_string())
        .unwrap()
        .check(&answer)
        .unwrap();
    v["variables"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"extra","domain":[0]}));
    assert!(FiniteDomain::parse(&v.to_string()).is_err());
    let mut v = sample();
    v["variables"][0]["domain"] = json!((0..64).collect::<Vec<_>>());
    FiniteDomain::parse(&v.to_string())
        .unwrap()
        .check(r#"{"a":1,"b":3}"#)
        .unwrap();
    assert!(FiniteDomain::parse(&v.to_string())
        .unwrap()
        .check(&format!("{}{}", r#"{"a":1,"b":3}"#, " ".repeat(8192)))
        .is_err());
}

#[test]
fn progress_score_counts_violated_constraints_for_complete_assignments() {
    let c = FiniteDomain::parse(&sample().to_string()).unwrap();
    let mut observed = 0;
    for a in 1..=3 {
        for b in 1..=3 {
            let answer = json!({"a":a,"b":b}).to_string();
            let expected = (if a == b { 1 } else { 0 }) + (if a + b != 4 { 1 } else { 0 });
            assert_eq!(c.progress_score(&answer), Some(expected as u32));
            assert_eq!(c.check(&answer).is_ok(), expected == 0);
            observed += 1;
        }
    }
    assert_eq!(observed, 9);

    let impossible = json!({
        "variables":[{"id":"x","domain":[0,1]},{"id":"y","domain":[0,1]}],
        "constraints":[
            {"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":0},
            {"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1},
            {"kind":"all_different","variables":["x","y"]}
        ]
    });
    let c = FiniteDomain::parse(&impossible.to_string()).unwrap();
    assert_eq!(c.progress_score(r#"{"x":0,"y":1}"#), Some(1));
    assert_eq!(c.progress_score(r#"{"y":0,"x":0}"#), Some(2));
}

#[test]
fn progress_score_is_order_invariant_and_only_scores_comparable_assignments() {
    let mut criterion = sample();
    let original = FiniteDomain::parse(&criterion.to_string()).unwrap();
    let score = original.progress_score(r#"{"a":2,"b":2}"#);
    criterion["variables"].as_array_mut().unwrap().reverse();
    criterion["constraints"].as_array_mut().unwrap().reverse();
    let reordered = FiniteDomain::parse(&criterion.to_string()).unwrap();
    assert_eq!(score, Some(1));
    assert_eq!(reordered.progress_score(r#"{"b":2,"a":2}"#), score);

    for incomparable in [
        r#"{"a":1}"#,
        r#"{"a":1,"b":3,"extra":0}"#,
        r#"{"a":0,"b":3}"#,
        r#"{"a":1,"b":3.0}"#,
        r#"{"a":1,"a":2,"b":3}"#,
        r#"{"a":true,"b":3}"#,
        r#"{"a":1,"b":null}"#,
        "[]",
        "null",
        "not json",
    ] {
        assert_eq!(
            original.progress_score(incomparable),
            None,
            "{incomparable}"
        );
    }
    assert_eq!(original.progress_score(&" ".repeat(8193)), None);

    use crate::verification::Criterion;
    let sorting = Criterion::from_query(r#"sort:{"values":[1]}"#)
        .unwrap()
        .unwrap();
    assert_eq!(sorting.progress_score("[1]"), None);
}

#[test]
fn deterministic_search_matches_small_independent_exhaustive_oracle() {
    for relation in ["eq", "ne", "le", "ge"] {
        for rhs in -3..=3 {
            let raw = json!({
                "variables": [
                    {"id":"x", "domain":[2, -1, 0]},
                    {"id":"y", "domain":[1, -2]}
                ],
                "constraints": [
                    {"kind":"linear", "terms":[
                        {"variable":"x", "coefficient":2},
                        {"variable":"y", "coefficient":-1}
                    ], "relation":relation, "rhs":rhs},
                    {"kind":"all_different", "variables":["x", "y"]}
                ]
            });
            let criterion = FiniteDomain::parse(&raw.to_string()).unwrap();
            let expected = [2, -1, 0].into_iter().find_map(|x| {
                [1, -2].into_iter().find_map(|y| {
                    let value = 2 * x - y;
                    let relation_holds = match relation {
                        "eq" => value == rhs,
                        "ne" => value != rhs,
                        "le" => value <= rhs,
                        _ => value >= rhs,
                    };
                    (relation_holds && x != y).then_some((x, y))
                })
            });

            let outcome = criterion.search(4096, || false).unwrap();
            match (expected, outcome.result) {
                (Some((x, y)), SearchResult::Witness(answer)) => {
                    assert_eq!(
                        serde_json::from_str::<BTreeMap<String, i64>>(&answer).unwrap(),
                        BTreeMap::from([("x".to_owned(), x), ("y".to_owned(), y)])
                    );
                    criterion.check(&answer).unwrap();
                }
                (None, SearchResult::Unsatisfiable) => {}
                (expected, actual) => panic!(
                    "oracle result {expected:?} disagrees with search result {}",
                    match actual {
                        SearchResult::Witness(_) => "witness",
                        SearchResult::Unsatisfiable => "unsatisfiable",
                        SearchResult::LimitReached => "limit reached",
                        SearchResult::Cancelled => "cancelled",
                    }
                ),
            }
        }
    }
}

#[test]
fn search_reports_limits_cancellation_and_only_exhaustive_unsatisfiability() {
    let criterion = FiniteDomain::parse(
        &json!({
            "variables":[{"id":"x","domain":[0,1]}],
            "constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]
        })
        .to_string(),
    )
    .unwrap();

    let limited = criterion.search(2, || false).unwrap();
    assert!(matches!(limited.result, SearchResult::LimitReached));
    assert_eq!(limited.explored_nodes, 2);

    let mut polls = 0;
    let cancelled = criterion
        .search(2, || {
            polls += 1;
            polls == 3
        })
        .unwrap();
    assert!(matches!(cancelled.result, SearchResult::Cancelled));
    assert_eq!(cancelled.explored_nodes, 2);

    let impossible = FiniteDomain::parse(
        &json!({
            "variables":[{"id":"x","domain":[0,1]}],
            "constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":2}]
        })
        .to_string(),
    )
    .unwrap();
    let complete = impossible.search(4096, || false).unwrap();
    assert!(matches!(complete.result, SearchResult::Unsatisfiable));
    assert_eq!(complete.explored_nodes, 3);

    let exact_limit_witness = criterion.search(3, || false).unwrap();
    assert!(matches!(
        exact_limit_witness.result,
        SearchResult::Witness(_)
    ));
    assert_eq!(exact_limit_witness.explored_nodes, 3);

    let exact_limit_unsatisfiable = impossible.search(3, || false).unwrap();
    assert!(matches!(
        exact_limit_unsatisfiable.result,
        SearchResult::Unsatisfiable
    ));
    assert_eq!(exact_limit_unsatisfiable.explored_nodes, 3);
}

#[test]
fn search_rejects_invalid_limits_and_cancellation_precedes_budget() {
    let criterion = FiniteDomain::parse(&sample().to_string()).unwrap();
    assert!(criterion.search(0, || false).is_err());
    assert!(criterion.search(4097, || false).is_err());

    let cancelled = criterion.search(1, || true).unwrap();
    assert!(matches!(cancelled.result, SearchResult::Cancelled));
    assert_eq!(cancelled.explored_nodes, 0);
}

#[test]
fn search_enforces_the_hard_4096_node_ceiling() {
    let variable_ids = (0..12).map(|index| format!("v{index}")).collect::<Vec<_>>();
    let raw = json!({
        "variables": variable_ids
            .iter()
            .map(|id| json!({"id": id, "domain": [0, 1]}))
            .collect::<Vec<_>>(),
        "constraints": [{"kind":"all_different", "variables": variable_ids}]
    });
    let criterion = FiniteDomain::parse(&raw.to_string()).unwrap();

    let outcome = criterion.search(4096, || false).unwrap();
    assert!(matches!(outcome.result, SearchResult::LimitReached));
    assert_eq!(outcome.explored_nodes, 4096);
}

#[test]
fn search_rechecks_stop_after_the_last_node_before_returning_a_proof() {
    let criterion = FiniteDomain::parse(
        &json!({
            "variables":[{"id":"x","domain":[0,1]}],
            "constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]
        })
        .to_string(),
    )
    .unwrap();
    let mut polls = 0;

    // The first three polls visit root, x=0 and the witness x=1. The final poll observes
    // cancellation after the witness was found but before the result is returned.
    let outcome = criterion
        .search(3, || {
            polls += 1;
            polls == 4
        })
        .unwrap();
    assert!(matches!(outcome.result, SearchResult::Cancelled));
    assert_eq!(outcome.explored_nodes, 3);
    assert_eq!(polls, 4);
}
