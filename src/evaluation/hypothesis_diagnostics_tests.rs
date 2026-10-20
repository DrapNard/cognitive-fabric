use super::*;
fn base() -> Value {
    json!({"format_version":1,"source_sha256":"a".repeat(64),"verified":false,"permissions_granted":false})
}
fn accepted() -> Value {
    let mut v = base();
    v["policy"] = json!("canonical-alternatives/v1");
    v["utility_calibrated"] = json!(false);
    v["alternative_ids"] = json!(["a", "b"]);
    v
}
fn selection(exhausted: bool) -> Value {
    let mut v = base();
    v["policy"] = json!("canonical-alternatives/v1");
    v["utility_calibrated"] = json!(false);
    v["cycle"] = json!(2);
    v["portfolio_exhausted"] = json!(exhausted);
    v["selected_id"] = if exhausted { Value::Null } else { json!("b") };
    v["strategy_sha256"] = if exhausted {
        Value::Null
    } else {
        json!("b".repeat(64))
    };
    v
}
#[test]
fn fixed_label_counts_are_scoped_content_minimized_and_additive() {
    let store = Store::in_memory().unwrap();
    let task = store
        .create_task("Hypothesis metadata", "research")
        .unwrap();
    let other = store.create_task("Other", "research").unwrap();
    let mut valid = accepted();
    valid["private"] = json!("private_strategy_never_exported");
    store
        .event(&task, "exploration_portfolio", "jev-r", valid)
        .unwrap();
    for status in ["legacy_advice", "invalid_portfolio", "future_status"] {
        let mut v = base();
        v["status"] = json!(status);
        store
            .event(&task, "exploration_portfolio", "jev-r", v)
            .unwrap();
    }
    for exhausted in [false, true] {
        store
            .event(&task, "hypothesis_selection", "jev-r", selection(exhausted))
            .unwrap();
    }
    store
        .event(&other, "exploration_portfolio", "jev-r", accepted())
        .unwrap();
    let counts = read(&store.lock().unwrap(), &task).unwrap();
    let value = serde_json::to_value(&counts).unwrap();
    assert_eq!(
        value,
        json!({"accepted_portfolio_events":1,"legacy_advice_events":1,"invalid_portfolio_events":1,"selected_hypothesis_events":1,"exhausted_selection_events":1,"unclassified_events":1})
    );
    assert!(!value.to_string().contains("private"));
    assert!(!value.to_string().contains(&"a".repeat(64)));
    let mut total = HypothesisDiagnostics::default();
    total.add(&counts);
    total.add(&counts);
    let total = serde_json::to_value(total).unwrap();
    for (key, value) in value.as_object().unwrap() {
        assert_eq!(total[key], value.as_u64().unwrap() * 2);
    }
    let empty = store.create_task("No history", "research").unwrap();
    assert_eq!(
        serde_json::to_value(read(&store.lock().unwrap(), &empty).unwrap()).unwrap(),
        serde_json::to_value(HypothesisDiagnostics::default()).unwrap()
    );
}
#[test]
fn unknown_versions_sources_authority_flags_and_malformed_shapes_are_unclassified() {
    let store = Store::in_memory().unwrap();
    let task = store.create_task("Unknown metadata", "research").unwrap();
    let mut cases = Vec::new();
    for (key, value) in [
        ("format_version", json!(2)),
        ("format_version", json!(1.0)),
        ("verified", json!(true)),
        ("permissions_granted", json!(true)),
        ("source_sha256", json!("A".repeat(64))),
        ("policy", json!("future_policy")),
        ("utility_calibrated", json!(true)),
        ("alternative_ids", json!(["a", "a"])),
        ("alternative_ids", json!(["a"])),
        ("alternative_ids", json!(["a", "bad/id"])),
        ("alternative_ids", json!(["a", 12])),
    ] {
        let mut v = accepted();
        v[key] = value;
        cases.push(("exploration_portfolio", "jev-r", v));
    }
    for (key, value) in [
        ("cycle", json!(0)),
        ("cycle", json!(17)),
        ("cycle", json!(2.5)),
        ("selected_id", json!("bad/id")),
        ("strategy_sha256", json!("not-hash")),
        ("portfolio_exhausted", json!(true)),
    ] {
        let mut v = selection(false);
        v[key] = value;
        cases.push(("hypothesis_selection", "jev-r", v));
    }
    cases.push(("hypothesis_selection", "untrusted", selection(false)));
    cases.push(("exploration_portfolio", "unknown", accepted()));
    let expected = cases.len();
    for (kind, source, value) in cases {
        store.event(&task, kind, source, value).unwrap();
    }
    let counts = serde_json::to_value(read(&store.lock().unwrap(), &task).unwrap()).unwrap();
    assert_eq!(counts["unclassified_events"], expected);
    assert_eq!(counts["accepted_portfolio_events"], 0);
    assert_eq!(counts["selected_hypothesis_events"], 0);
    assert_eq!(counts["exhausted_selection_events"], 0);
}
#[test]
fn diagnostic_event_and_byte_ceilings_fail_before_exporting_payloads() {
    let store = Store::in_memory().unwrap();
    for mode in ["oversized", "overflow", "malformed"] {
        let task = store.create_task("Bounded metadata", "research").unwrap();
        match mode {
            "oversized" => {
                let mut v = accepted();
                v["private"] = json!("x".repeat(17000));
                store
                    .event(&task, "exploration_portfolio", "jev-r", v)
                    .unwrap();
            }
            "overflow" => {
                for _ in 0..129 {
                    store
                        .event(&task, "hypothesis_selection", "jev-r", selection(false))
                        .unwrap();
                }
            }
            _ => {
                store.lock().unwrap().execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'exploration_portfolio','jev-r','not JSON',unixepoch())",[&task]).unwrap();
            }
        }
        assert!(read(&store.lock().unwrap(), &task).is_err(), "{mode}");
    }
}
