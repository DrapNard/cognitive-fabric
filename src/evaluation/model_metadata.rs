//! Content-minimized model identity disclosure from bounded task audit records.
use super::*;
#[path = "hypothesis_diagnostics.rs"]
mod hypotheses;
pub(crate) use hypotheses::HypothesisDiagnostics;
#[derive(Default)]
pub(crate) struct ModelMetadata {
    pub reported_models: Vec<String>,
    pub bound_expert_models: Vec<Value>,
    pub primary_model_snapshot: Option<Value>,
    /// The stable selected role/module/target map, with endpoint values reduced
    /// to hashes for evaluation reports.
    pub resource_route_sha256: Option<String>,
    pub resource_routes: Option<Vec<Value>>,
    pub expert_advice: AdviceDiagnostics,
    pub planning_advice: AdviceDiagnostics,
    pub exploration_advice: AdviceDiagnostics,
    pub hypothesis_diagnostics: HypothesisDiagnostics,
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
    result.hypothesis_diagnostics = hypotheses::read(&db, task)?;
    let (route_digest, routes) = resource_routes(&db, task)?;
    result.resource_route_sha256 = route_digest;
    result.resource_routes = routes;
    Ok(result)
}

/// Read only the bounded routing snapshot used for this task's resource-aware
/// pin. Reports include selected identities and target hashes; endpoint strings,
/// rejected candidates, pool IDs and callback errors stay in the local audit.
fn resource_routes(
    db: &rusqlite::Connection,
    task: &str,
) -> Result<(Option<String>, Option<Vec<Value>>)> {
    let mut statement = db.prepare(
        "SELECT CASE WHEN length(CAST(payload AS BLOB))<=65536 THEN payload END FROM events WHERE task_id=?1 AND kind='module_snapshot' ORDER BY id LIMIT 2",
    )?;
    let mut rows = statement.query([task])?;
    let Some(row) = rows.next()? else {
        return Ok((None, None));
    };
    let raw: Option<String> = row.get(0)?;
    let raw = raw.ok_or_else(|| anyhow::anyhow!("Module snapshot exceeds metadata bounds"))?;
    if rows.next()?.is_some() {
        bail!("Duplicate module snapshots in evaluation metadata")
    }
    let snapshot = crate::strict_json::value(raw.as_bytes(), 65_536)?;
    let has_marker = snapshot.get("resource_routing_version").is_some()
        || snapshot.get("resource_routes").is_some();
    if !has_marker {
        return Ok((None, None));
    }
    if snapshot["resource_routing_version"] != 1 {
        bail!("Unsupported resource routing snapshot in evaluation metadata")
    }
    let selection = &snapshot["resource_routes"];
    require_exact_keys(
        selection,
        &[
            "format_version",
            "pool_id",
            "binding",
            "routes",
            "unmatched_roles",
            "excluded_candidates",
            "eligible",
            "permissions_granted",
            "weights_attested",
            "physical_limits_enforced",
            "route_scope_sha256",
        ],
    )?;
    if selection["format_version"] != 1
        || selection["eligible"].as_bool().is_none()
        || selection["permissions_granted"] != false
        || selection["weights_attested"] != false
        || selection["physical_limits_enforced"] != false
    {
        bail!("Malformed resource routing selection in evaluation metadata")
    }
    let route_digest = selection["route_scope_sha256"]
        .as_str()
        .filter(|value| is_lower_sha256(value))
        .ok_or_else(|| anyhow::anyhow!("Missing resource route scope digest"))?;
    if crate::fabric::resource_route_scope(selection)? != route_digest {
        bail!("Resource route scope digest does not match its selection")
    }
    let routes = selection["routes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing selected resource routes"))?;
    if routes.len() > crate::fabric::protocol::ROLES.len() {
        bail!("Selected resource routes exceed role bounds")
    }
    if selection["eligible"] != !routes.is_empty() {
        bail!("Resource route eligibility does not match the selected map")
    }
    let mut projected = Vec::with_capacity(routes.len());
    let mut previous_role = None;
    for route in routes {
        require_exact_keys(route, &["role", "module_id", "module_version", "resource"])?;
        let role = route["role"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing selected route role"))?;
        crate::fabric::protocol::actor(role)?;
        let role_order = crate::fabric::protocol::ROLES
            .iter()
            .position(|known| *known == role)
            .ok_or_else(|| anyhow::anyhow!("Unknown selected route role"))?;
        if previous_role.is_some_and(|previous| role_order <= previous) {
            bail!("Selected resource routes are duplicated or unordered")
        }
        previous_role = Some(role_order);
        let module_id = route["module_id"]
            .as_str()
            .filter(|value| (1..=64).contains(&value.len()))
            .ok_or_else(|| anyhow::anyhow!("Invalid selected module identity"))?;
        let module_version = route["module_version"]
            .as_str()
            .filter(|value| (1..=128).contains(&value.len()))
            .ok_or_else(|| anyhow::anyhow!("Invalid selected module version"))?;
        crate::fabric::ModuleDescriptor {
            id: module_id.to_owned(),
            version: module_version.to_owned(),
            roles: vec![role.to_owned()],
        }
        .validate()?;
        let resource = &route["resource"];
        require_exact_keys(
            resource,
            &[
                "target",
                "profile_id",
                "allocation",
                "resident_coverage",
                "eligible",
                "reason_code",
                "permissions_granted",
                "weights_attested",
                "physical_limits_enforced",
            ],
        )?;
        if resource["eligible"] != true
            || resource["permissions_granted"] != false
            || resource["weights_attested"] != false
            || resource["physical_limits_enforced"] != false
        {
            bail!("Invalid selected route resource declaration")
        }
        let target = canonical_target(&resource["target"])?;
        let profile_id = match resource["profile_id"].as_str() {
            Some(value) if value.len() <= 64 => Some(value),
            None if resource["profile_id"].is_null() => None,
            _ => bail!("Invalid selected route profile identity"),
        };
        let target_sha256 = crate::store::sha256(&serde_json::to_vec(&target)?);
        projected.push(serde_json::json!({
            "role":role,
            "module_id":module_id,
            "module_version":module_version,
            "target_sha256":target_sha256,
            "profile_id":profile_id,
        }));
    }
    Ok((Some(route_digest.to_owned()), Some(projected)))
}

fn canonical_target(value: &Value) -> Result<crate::backend::InferenceTarget> {
    use crate::backend::{InferenceTarget, ModelTarget, NativeModelTarget};
    let target = match value["kind"].as_str() {
        Some("deterministic") if value.as_object().is_some_and(|object| object.len() == 1) => {
            InferenceTarget::Deterministic
        }
        Some("local_http")
            if value
                .as_object()
                .is_some_and(|object| object.len() == 2 && object.contains_key("target")) =>
        {
            let target: ModelTarget = serde_json::from_value(value["target"].clone())?;
            let canonical = target.canonical()?;
            if canonical != target
                || value["target"]
                    .as_object()
                    .is_none_or(|object| object.len() != 2)
            {
                bail!("Selected route target is not canonical")
            }
            InferenceTarget::LocalHttp(target)
        }
        Some("native")
            if value
                .as_object()
                .is_some_and(|object| object.len() == 2 && object.contains_key("target")) =>
        {
            let target: NativeModelTarget = serde_json::from_value(value["target"].clone())?;
            let canonical = target.canonical()?;
            if canonical != target
                || value["target"]
                    .as_object()
                    .is_none_or(|object| object.len() != 3)
            {
                bail!("Selected native route target is not canonical")
            }
            InferenceTarget::Native(target)
        }
        _ => bail!("Malformed selected route target"),
    };
    if serde_json::to_value(&target)? != *value {
        bail!("Selected route target shape changed")
    }
    Ok(target)
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn require_exact_keys(value: &Value, expected: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Malformed resource route metadata"))?;
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        bail!("Malformed resource route metadata shape")
    }
    Ok(())
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

    fn routed_selection(roles: &[&str]) -> Value {
        let routes = roles
            .iter()
            .map(|role| {
                json!({
                    "role":role,
                    "module_id":format!("fixture.{role}"),
                    "module_version":"1.2.3",
                    "resource":{
                        "target":{"kind":"local_http","target":{"api_base":"http://127.0.0.1:1234/v1","model":format!("model-{role}")}},
                        "profile_id":format!("{role}-profile"),
                        "allocation":{"compute_slots":1,"host_memory_bytes":1024,"device_memory_bytes":0},
                        "resident_coverage":null,
                        "eligible":true,
                        "reason_code":"configured_resource_match",
                        "permissions_granted":false,
                        "weights_attested":false,
                        "physical_limits_enforced":false
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut selection = json!({
            "format_version":1,
            "pool_id":"private-pool-identity",
            "binding":{"private_catalog_value":"must-not-appear-in-report"},
            "routes":routes,
            "unmatched_roles":[],
            "excluded_candidates":[{"module_id":"private.rejected","reason_code":"private_callback_detail","endpoint":"http://secret.invalid/private"}],
            "eligible":!roles.is_empty(),
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false
        });
        let digest = crate::fabric::resource_route_scope(&selection).unwrap();
        selection["route_scope_sha256"] = json!(digest);
        selection
    }

    fn read_routing_snapshot(snapshot: Value) -> Result<ModelMetadata> {
        let store = Store::in_memory()?;
        let task = store.create_task("Route metadata", "standard")?;
        store.event(&task, "module_snapshot", "fabric", snapshot)?;
        read(&store, &task)
    }

    fn wrapped_selection(selection: Value) -> Value {
        json!({
            "modules":[{"id":"inference.local","version":"1.0.0","roles":["planner"]}],
            "resource_routing_version":1,
            "resource_routes":selection
        })
    }

    #[test]
    fn resource_route_report_discloses_only_selected_identity_and_target_hashes() {
        let selection = routed_selection(&["planner", "solver"]);
        let metadata = read_routing_snapshot(wrapped_selection(selection.clone())).unwrap();
        assert_eq!(
            metadata.resource_route_sha256.as_deref(),
            selection["route_scope_sha256"].as_str()
        );
        let routes = metadata.resource_routes.unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0]["role"], "planner");
        assert_eq!(routes[0]["module_id"], "fixture.planner");
        assert_eq!(routes[0]["module_version"], "1.2.3");
        assert_eq!(routes[0]["profile_id"], "planner-profile");
        let target = crate::backend::InferenceTarget::LocalHttp(crate::backend::ModelTarget {
            api_base: "http://127.0.0.1:1234/v1".into(),
            model: "model-planner".into(),
        });
        assert_eq!(
            routes[0]["target_sha256"],
            crate::store::sha256(&serde_json::to_vec(&target).unwrap())
        );
        let report = serde_json::to_string(&json!({
            "route_scope_sha256":metadata.resource_route_sha256,
            "routes":routes,
        }))
        .unwrap();
        for private in [
            "api_base",
            "127.0.0.1",
            "private-pool-identity",
            "private_catalog_value",
            "private.rejected",
            "private_callback_detail",
            "secret.invalid",
            "must-not-appear-in-report",
        ] {
            assert!(!report.contains(private), "report leaked {private}");
        }
        assert!(report.contains("fixture.planner"));
        assert!(report.contains("planner-profile"));
    }

    #[test]
    fn native_route_report_hashes_the_exact_instance_but_omits_api_identity() {
        let native = crate::backend::NativeModelTarget {
            api_base: "http://127.0.0.1:1234/api/v1".into(),
            model: "native-model-key".into(),
            model_instance_id: "instance-a".into(),
        };
        let target = crate::backend::InferenceTarget::Native(native.clone());
        let mut selection = routed_selection(&["planner"]);
        selection["routes"][0]["resource"]["target"] = serde_json::to_value(&target).unwrap();
        selection["route_scope_sha256"] =
            json!(crate::fabric::resource_route_scope(&selection).unwrap());

        let metadata = read_routing_snapshot(wrapped_selection(selection)).unwrap();
        let route = &metadata.resource_routes.unwrap()[0];
        assert_eq!(
            route["target_sha256"],
            crate::store::sha256(&serde_json::to_vec(&target).unwrap())
        );
        assert_ne!(
            route["target_sha256"],
            crate::store::sha256(
                &serde_json::to_vec(&crate::backend::InferenceTarget::Native(
                    crate::backend::NativeModelTarget {
                        model_instance_id: "instance-b".into(),
                        ..native
                    }
                ))
                .unwrap()
            )
        );
        let report = serde_json::to_string(route).unwrap();
        assert!(!report.contains("api_base"));
        assert!(!report.contains("127.0.0.1"));
        assert!(!report.contains("instance-a"));
    }

    #[test]
    fn native_route_report_rejects_openai_protocol_bases() {
        let mut selection = routed_selection(&["planner"]);
        selection["routes"][0]["resource"]["target"] = json!({
            "kind":"native",
            "target":{
                "api_base":"http://127.0.0.1:1234/v1",
                "model":"native-model-key",
                "model_instance_id":"instance-a"
            }
        });
        selection["route_scope_sha256"] =
            json!(crate::fabric::resource_route_scope(&selection).unwrap());
        assert!(read_routing_snapshot(wrapped_selection(selection)).is_err());
    }

    #[test]
    fn empty_and_legacy_route_snapshots_remain_distinguishable() {
        let empty = read_routing_snapshot(wrapped_selection(routed_selection(&[]))).unwrap();
        assert!(empty.resource_routes.unwrap().is_empty());
        assert!(empty.resource_route_sha256.is_some());

        for legacy in [json!({"modules":[]}), json!({})] {
            let metadata = read_routing_snapshot(legacy).unwrap();
            assert_eq!(metadata.resource_routes, None);
            assert_eq!(metadata.resource_route_sha256, None);
        }
        let no_snapshot = Store::in_memory().unwrap();
        let task = no_snapshot
            .create_task("Legacy route metadata", "fast")
            .unwrap();
        let metadata = read(&no_snapshot, &task).unwrap();
        assert_eq!(metadata.resource_routes, None);
        assert_eq!(metadata.resource_route_sha256, None);
    }

    #[test]
    fn malformed_routing_projection_is_rejected() {
        let scenarios = ["duplicate", "unordered", "digest", "unsupported_version"];
        for scenario in scenarios {
            let mut selection = routed_selection(&["planner", "solver"]);
            let mut version = 1;
            match scenario {
                "duplicate" => {
                    selection["routes"][1]["role"] = json!("planner");
                    selection["routes"][1]["module_id"] = json!("fixture.duplicate");
                }
                "unordered" => selection["routes"].as_array_mut().unwrap().reverse(),
                "digest" => selection["route_scope_sha256"] = json!("a".repeat(64)),
                "unsupported_version" => version = 2,
                _ => unreachable!(),
            }
            if scenario == "duplicate" || scenario == "unordered" {
                selection["route_scope_sha256"] =
                    json!(crate::fabric::resource_route_scope(&selection).unwrap());
            }
            let mut snapshot = wrapped_selection(selection);
            snapshot["resource_routing_version"] = json!(version);
            assert!(
                read_routing_snapshot(snapshot).is_err(),
                "scenario {scenario} should fail closed"
            );
        }
    }

    #[test]
    fn oversized_routing_snapshot_is_rejected_before_projection() {
        let snapshot = json!({
            "modules":[],
            "resource_routing_version":1,
            "resource_routes":routed_selection(&[]),
            "padding":"x".repeat(66_000)
        });
        assert!(read_routing_snapshot(snapshot).is_err());
    }
}
