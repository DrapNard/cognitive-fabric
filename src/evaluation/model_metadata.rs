//! Content-minimized model identity disclosure from bounded task audit records.
use super::*;
#[derive(Default)]
pub(crate) struct ModelMetadata {
    pub reported_models: Vec<String>,
    pub bound_expert_models: Vec<Value>,
    pub primary_model_snapshot: Option<Value>,
    pub expert_advice: AdviceDiagnostics,
    pub planning_advice: AdviceDiagnostics,
    pub exploration_advice: AdviceDiagnostics,
}
/// Audit counts describe attempted advice, never its usefulness or provider stop.
#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct AdviceDiagnostics {
    phases_started: usize,
    fallback_events: usize,
    inference_timeout_fallbacks: usize,
    unclassified_fallbacks: usize,
}
impl AdviceDiagnostics {
    pub(crate) fn add(&mut self, other: &Self) {
        self.phases_started += other.phases_started;
        self.fallback_events += other.fallback_events;
        self.inference_timeout_fallbacks += other.inference_timeout_fallbacks;
        self.unclassified_fallbacks += other.unclassified_fallbacks;
    }
}
fn advice_diagnostics(
    db: &rusqlite::Connection,
    task: &str,
    budget_kind: &str,
    failure_kind: &str,
) -> Result<AdviceDiagnostics> {
    // Copy only bounded type labels, never advice, prompts or error messages.
    let mut statement = db.prepare("SELECT kind, CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN CASE WHEN kind=?2 THEN 'phase' WHEN json_type(payload,'$.failure_kind')='text' AND json_extract(payload,'$.failure_kind')='inference_timeout' THEN 'timeout' ELSE 'unclassified' END END FROM events WHERE task_id=?1 AND kind IN (?2,?3) ORDER BY id LIMIT 129")?;
    let mut rows = statement.query([task, budget_kind, failure_kind])?;
    let mut result = AdviceDiagnostics::default();
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 128 {
            bail!("Advisory phase diagnostic audit exceeds event limit")
        }
        let kind: String = row.get(0)?;
        let label: String = row.get(1)?;
        if kind == budget_kind && label == "phase" {
            result.phases_started += 1;
        } else if kind == failure_kind && matches!(label.as_str(), "timeout" | "unclassified") {
            result.fallback_events += 1;
            if label == "timeout" {
                result.inference_timeout_fallbacks += 1;
            } else {
                result.unclassified_fallbacks += 1;
            }
        } else {
            bail!("Invalid advisory phase diagnostic audit")
        }
    }
    Ok(result)
}
pub(crate) fn read(store: &Store, task: &str) -> Result<ModelMetadata> {
    uuid::Uuid::parse_str(task)?;
    let db = store.lock()?;
    // Project just the label in SQLite; never copy reasoning text into Rust.
    let mut statement=db.prepare("SELECT kind, CASE WHEN kind='reasoning' THEN CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN CASE WHEN json_type(payload,'$.node.model')='text' AND length(CAST(json_extract(payload,'$.node.model') AS BLOB)) BETWEEN 1 AND 256 THEN json_quote(json_extract(payload,'$.node.model')) END END ELSE CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END END FROM events WHERE task_id=?1 AND kind IN ('reasoning','expert_model_snapshot','model_registration') ORDER BY id LIMIT 129")?;
    let mut rows = statement.query([task])?;
    let mut result = ModelMetadata::default();
    let mut count = 0;
    let mut bytes = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 128 {
            bail!("Model identity audit exceeds event limit")
        }
        let kind: String = row.get(0)?;
        let raw: String = row.get(1)?;
        bytes += raw.len();
        if bytes > 262144 {
            bail!("Model identity audit exceeds byte limit")
        }
        let value: Value = serde_json::from_str(&raw)?;
        match kind.as_str() {
            "reasoning" => {
                let model = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid reported model label"))?;
                if model.trim().is_empty() || model.chars().any(char::is_control) {
                    bail!("Invalid reported model label")
                }
                result.reported_models.push(model.into());
            }
            "expert_model_snapshot" => {
                if !value.is_object() {
                    bail!("Invalid expert model snapshot")
                }
                result.bound_expert_models.push(value);
            }
            "model_registration" => {
                if !value.is_object() || result.primary_model_snapshot.is_some() {
                    bail!("Invalid or duplicate primary model snapshot")
                }
                result.primary_model_snapshot = Some(value);
            }
            _ => bail!("Unsupported model identity event"),
        }
    }
    result.expert_advice = advice_diagnostics(&db, task, "expert_phase_budget", "expert_failure")?;
    result.planning_advice =
        advice_diagnostics(&db, task, "planning_phase_budget", "planning_failure")?;
    result.exploration_advice =
        advice_diagnostics(&db, task, "exploration_phase_budget", "exploration_failure")?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn planning_and_exploration_counts_are_task_scoped_and_content_minimized() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("Phase metadata", "research").unwrap();
        let other = store.create_task("Other task", "research").unwrap();
        for (budget, failure) in [
            ("planning_phase_budget", "planning_failure"),
            ("exploration_phase_budget", "exploration_failure"),
        ] {
            store
                .event(&task, budget, "jev-plus", json!({"max_milliseconds":1000}))
                .unwrap();
            store.event(&task, failure, "jev-plus", json!({"failure_kind":"inference_timeout","private":"private_strategy_not_for_reports"})).unwrap();
            store
                .event(
                    &task,
                    failure,
                    "jev-plus",
                    json!({"reason":"Inference timeout"}),
                )
                .unwrap();
            if budget == "planning_phase_budget" {
                store
                    .event(&task, budget, "jev-plus", json!({"max_milliseconds":500}))
                    .unwrap();
            }
            store
                .event(
                    &other,
                    failure,
                    "jev-plus",
                    json!({"failure_kind":"inference_timeout"}),
                )
                .unwrap();
        }
        let metadata = read(&store, &task).unwrap();
        for (diagnostic, phases) in [
            (&metadata.planning_advice, 2),
            (&metadata.exploration_advice, 1),
        ] {
            let value = serde_json::to_value(diagnostic).unwrap();
            assert_eq!(
                value,
                json!({"phases_started":phases,"fallback_events":2,"inference_timeout_fallbacks":1,"unclassified_fallbacks":1})
            );
            assert!(!value.to_string().contains("private_strategy"));
        }
        assert_eq!(
            serde_json::to_value(metadata.expert_advice).unwrap()["phases_started"],
            0
        );
        for budget in ["planning_phase_budget", "exploration_phase_budget"] {
            let task = store.create_task("Oversized phase", "research").unwrap();
            store
                .event(
                    &task,
                    budget,
                    "jev-plus",
                    json!({"extra":"x".repeat(17000)}),
                )
                .unwrap();
            assert!(read(&store, &task).is_err());
        }
    }

    #[test]
    fn advice_diagnostics_preserve_unknown_failures_and_bound_projection() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("Advice diagnostics", "deep").unwrap();
        store
            .event(
                &task,
                "expert_phase_budget",
                "jev-plus",
                json!({"max_milliseconds":1000}),
            )
            .unwrap();
        store
            .event(
                &task,
                "expert_failure",
                "jev-plus",
                json!({"failure_kind":"inference_timeout","reason":"private provider error"}),
            )
            .unwrap();
        // Historical/unknown failures must never be classified by error text.
        store
            .event(
                &task,
                "expert_failure",
                "jev-plus",
                json!({"reason":"Inference timeout"}),
            )
            .unwrap();
        store
            .event(
                &task,
                "expert_failure",
                "jev-plus",
                json!({"failure_kind":"future_failure"}),
            )
            .unwrap();
        let value = serde_json::to_value(read(&store, &task).unwrap().expert_advice).unwrap();
        assert_eq!(
            value,
            json!({"phases_started":1,"fallback_events":3,"inference_timeout_fallbacks":1,"unclassified_fallbacks":2})
        );
        assert!(!value.to_string().contains("private"));
        for oversized in [false, true] {
            let task = store
                .create_task("Bounded advice diagnostics", "deep")
                .unwrap();
            if oversized {
                store
                    .event(
                        &task,
                        "expert_failure",
                        "jev-plus",
                        json!({"reason":"x".repeat(17000)}),
                    )
                    .unwrap();
            } else {
                for _ in 0..129 {
                    store
                        .event(&task, "expert_phase_budget", "jev-plus", json!({}))
                        .unwrap();
                }
            }
            assert!(read(&store, &task).is_err());
        }
    }

    #[test]
    fn identity_projection_excludes_large_reasoning_and_retains_registration_snapshots() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("Metadata projection", "fast").unwrap();
        store.event(&task,"reasoning","jev-r",json!({"node":{"model":"provider.fixture"},"text":"reasoning_body_not_for_identity_report".repeat(5000)})).unwrap();
        let primary = json!({"registration":null,"weight_identity_verified":false});
        let expert = json!({"expert":"rust.fixture","requested_model":"expert.fixture","weights_attested":false});
        store
            .event(&task, "model_registration", "runtime", primary.clone())
            .unwrap();
        store
            .event(&task, "expert_model_snapshot", "jev-plus", expert.clone())
            .unwrap();
        let metadata = read(&store, &task).unwrap();
        assert_eq!(metadata.reported_models, vec!["provider.fixture"]);
        assert_eq!(metadata.primary_model_snapshot, Some(primary));
        assert_eq!(metadata.bound_expert_models, vec![expert]);
    }
    #[test]
    fn malformed_oversized_or_ambiguous_identity_records_fail_closed() {
        for scenario in 0..8 {
            let store = Store::in_memory().unwrap();
            let task = store.create_task("Metadata rejection", "fast").unwrap();
            match scenario {
                0..=3 => {
                    let model = match scenario {
                        0 => json!("x".repeat(257)),
                        1 => json!(""),
                        2 => json!("model\nforged"),
                        _ => json!(["not-a-label"]),
                    };
                    store
                        .event(&task, "reasoning", "jev-r", json!({"node":{"model":model}}))
                        .unwrap();
                }
                4 => {
                    store
                        .event(
                            &task,
                            "expert_model_snapshot",
                            "jev-plus",
                            json!({"extra":"x".repeat(17000)}),
                        )
                        .unwrap();
                }
                5 => {
                    for _ in 0..2 {
                        store
                            .event(
                                &task,
                                "model_registration",
                                "runtime",
                                json!({"registration":null}),
                            )
                            .unwrap();
                    }
                }
                6 => {
                    for _ in 0..129 {
                        store
                            .event(
                                &task,
                                "reasoning",
                                "jev-r",
                                json!({"node":{"model":"fixture"}}),
                            )
                            .unwrap();
                    }
                }
                _ => {
                    for _ in 0..20 {
                        store
                            .event(
                                &task,
                                "expert_model_snapshot",
                                "jev-plus",
                                json!({"extra":"x".repeat(15000)}),
                            )
                            .unwrap();
                    }
                }
            }
            assert!(read(&store, &task).is_err(), "scenario {scenario}");
        }
    }
}
