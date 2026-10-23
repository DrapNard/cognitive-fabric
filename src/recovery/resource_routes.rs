//! Validation and exact restoration of the initial resource-aware route map.
use crate::{
    backend::{InferenceBackend, InferenceTarget},
    config::Config,
    resource_scheduler::{Configuration, ResourcePool},
};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

const BUILTIN_ID: &str = "inference.local";
const BUILTIN_VERSION: &str = "1.0.0";

/// Validate every durable routing record against the original saved resource
/// declarations and the immutable module snapshot. The returned map is the only
/// set of roles recovery may expose to the inference backend.
pub(crate) fn validate(
    db: &rusqlite::Connection,
    task: &str,
    cfg: &Config,
    execution_settings: &Value,
    module_snapshot: &Value,
) -> Result<Option<(BTreeMap<String, InferenceTarget>, String)>> {
    let mut intent: Option<(i64, String, Value)> = None;
    let mut selection: Option<(i64, String, Value)> = None;
    let mut snapshot_id = None;
    let mut first_dispatch = None;
    let mut stmt = db.prepare(
        "SELECT id,kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=262144 THEN payload END FROM events WHERE task_id=?1 AND kind IN ('routing_resource_intent','routing_resource_selection','module_snapshot','model_dispatch') ORDER BY id LIMIT 4097",
    )?;
    let mut rows = stmt.query([task])?;
    let mut seen = 0usize;
    while let Some(row) = rows.next()? {
        seen += 1;
        if seen > 4096 {
            bail!("Routing history exceeds bounds")
        }
        let id: i64 = row.get(0)?;
        let kind: String = row.get(1)?;
        let source: String = row.get(2)?;
        let raw: String = row.get(3)?;
        if kind == "model_dispatch" {
            first_dispatch.get_or_insert(id);
            continue;
        }
        let payload = crate::strict_json::value(raw.as_bytes(), 262144)?;
        match kind.as_str() {
            "routing_resource_intent" => {
                if intent.replace((id, source, payload)).is_some() {
                    bail!("Repeated resource routing intent")
                }
            }
            "routing_resource_selection" => {
                if selection.replace((id, source, payload)).is_some() {
                    bail!("Repeated resource routing selection")
                }
            }
            "module_snapshot" if source == "fabric" && payload == *module_snapshot => {
                if snapshot_id.replace(id).is_some() {
                    bail!("Repeated module snapshot")
                }
            }
            "module_snapshot" => {}
            _ => unreachable!(),
        }
    }

    let marked = intent.is_some()
        || selection.is_some()
        || execution_settings.get("resource_routing_version").is_some()
        || module_snapshot.get("resource_routes").is_some()
        || module_snapshot.get("resource_routing_version").is_some();
    if !marked {
        return Ok(None);
    }
    if execution_settings["resource_routing_version"] != 1
        || module_snapshot["resource_routing_version"] != 1
    {
        bail!("Resource route history is missing its supported version marker")
    }
    let resources = cfg
        .resource_admission
        .as_ref()
        .ok_or_else(|| anyhow!("Resource route history has no saved resource configuration"))?;
    if cfg.backend != "openai-local" {
        bail!("Resource route recovery supports only the builtin local inference adapter")
    }
    let (intent_id, intent_source, intent_payload) =
        intent.ok_or_else(|| anyhow!("Missing original resource routing intent"))?;
    let (selection_id, selection_source, selection_payload) =
        selection.ok_or_else(|| anyhow!("Missing original resource routing selection"))?;
    let snapshot_id = snapshot_id.ok_or_else(|| anyhow!("Missing original module snapshot"))?;
    if intent_source != "jev-plus" || selection_source != "jev-plus" {
        bail!("Invalid resource routing audit source")
    }
    if !(intent_id < selection_id && selection_id < snapshot_id)
        || first_dispatch.is_some_and(|dispatch| snapshot_id >= dispatch)
    {
        bail!("Resource routing history is out of chronological order")
    }

    let binding = ResourcePool::configuration_binding(resources)?;
    require_exact_keys(
        &intent_payload,
        &[
            "format_version",
            "pool_id",
            "binding",
            "roles",
            "permissions_granted",
            "weights_attested",
            "physical_limits_enforced",
        ],
    )?;
    if intent_payload["format_version"] != 1
        || intent_payload["pool_id"].as_str().is_none_or(str::is_empty)
        || intent_payload["binding"] != binding
        || intent_payload["roles"] != json!(crate::fabric::protocol::ROLES)
        || intent_payload["permissions_granted"] != false
        || intent_payload["weights_attested"] != false
        || intent_payload["physical_limits_enforced"] != false
    {
        bail!("Original resource routing intent changed")
    }
    uuid::Uuid::parse_str(intent_payload["pool_id"].as_str().unwrap())
        .map_err(|_| anyhow!("Invalid original resource pool identity"))?;
    require_exact_keys(
        &selection_payload,
        &[
            "format_version",
            "pool_id",
            "binding",
            "routes",
            "unmatched_roles",
            "excluded_candidates",
            "route_scope_sha256",
            "eligible",
            "permissions_granted",
            "weights_attested",
            "physical_limits_enforced",
        ],
    )?;
    if selection_payload["format_version"] != 1
        || selection_payload["pool_id"] != intent_payload["pool_id"]
        || selection_payload["binding"] != binding
        || selection_payload["eligible"]
            != selection_payload["routes"]
                .as_array()
                .is_some_and(|routes| !routes.is_empty())
        || selection_payload["permissions_granted"] != false
        || selection_payload["weights_attested"] != false
        || selection_payload["physical_limits_enforced"] != false
    {
        bail!("Original resource route selection changed")
    }
    if serde_json::to_vec(&selection_payload)?.len() > 32_768 {
        bail!("Resource routing selection exceeds its audit bound")
    }
    let excluded = selection_payload["excluded_candidates"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing resource routing exclusions"))?;
    if excluded.len() > 224 {
        bail!("Resource routing exclusions exceed their audit bound")
    }
    for candidate in excluded {
        require_exact_keys(
            candidate,
            &["role", "module_id", "module_version", "reason_code"],
        )?;
        if !crate::fabric::protocol::ROLES.contains(&candidate["role"].as_str().unwrap_or(""))
            || candidate["module_id"]
                .as_str()
                .is_none_or(|value| value.is_empty() || value.len() > 64)
            || candidate["module_version"]
                .as_str()
                .is_none_or(|value| value.is_empty() || value.len() > 128)
            || !matches!(
                candidate["reason_code"].as_str(),
                Some(
                    "queue_full"
                        | "unavailable"
                        | "metadata_error"
                        | "unknown_target"
                        | "unlisted_profile"
                        | "resident_miss"
                )
            )
        {
            bail!("Malformed resource routing exclusion")
        }
    }
    let routes = selection_payload["routes"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing selected resource routes"))?;
    if routes.len() > crate::fabric::protocol::ROLES.len() {
        bail!("Resource route map exceeds role bounds")
    }

    let expected_modules =
        json!([{"id":BUILTIN_ID,"version":BUILTIN_VERSION,"roles":crate::fabric::protocol::ROLES}]);
    let expected_module_snapshot = if routes.is_empty() {
        json!([])
    } else {
        expected_modules
    };
    if module_snapshot.get("modules") != Some(&expected_module_snapshot)
        || module_snapshot.get("resource_routes") != Some(&selection_payload)
        || module_snapshot
            .as_object()
            .is_none_or(|object| object.len() != 3)
    {
        bail!("Pinned module snapshot does not match the original resource route map")
    }

    let mut role_targets = BTreeMap::new();
    let mut previous_rank = 0usize;
    for (index, route) in routes.iter().enumerate() {
        require_exact_keys(route, &["role", "module_id", "module_version", "resource"])?;
        let role = route["role"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing resource route role"))?;
        let rank = crate::fabric::protocol::ROLES
            .iter()
            .position(|known| *known == role)
            .ok_or_else(|| anyhow!("Unknown role in saved resource route map"))?;
        if (index > 0 && rank <= previous_rank)
            || route["module_id"] != BUILTIN_ID
            || route["module_version"] != BUILTIN_VERSION
        {
            bail!("Unsupported or reordered pinned resource route")
        }
        previous_rank = rank;
        let target = crate::backend::configured_target(cfg, Some(role))?
            .ok_or_else(|| anyhow!("Saved route target cannot be reconstructed"))?;
        if !matches!(target, InferenceTarget::LocalHttp(_)) {
            bail!("Resource route recovery requires the original local HTTP target")
        }
        let resource = expected_resource_metadata(resources, role, target.clone())?;
        if route["resource"] != resource {
            bail!("Saved resource route differs from original role and target declarations")
        }
        if role_targets.insert(role.to_string(), target).is_some() {
            bail!("Duplicate role in saved resource route map")
        }
    }
    let unmatched = crate::fabric::protocol::ROLES
        .iter()
        .filter(|role| !role_targets.contains_key(**role))
        .copied()
        .collect::<Vec<_>>();
    if selection_payload["unmatched_roles"] != json!(unmatched) {
        bail!("Saved resource route omissions do not match the selected role map")
    }
    if role_targets.is_empty() && first_dispatch.is_some() {
        bail!("An empty saved resource route map cannot have model dispatch history")
    }
    let scope = selection_payload["route_scope_sha256"]
        .as_str()
        .filter(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .ok_or_else(|| anyhow!("Invalid saved resource route scope digest"))?;
    if crate::fabric::resource_route_scope(&selection_payload)? != scope {
        bail!("Saved resource route scope digest differs from its immutable selection")
    }
    Ok(Some((role_targets, scope.to_owned())))
}

fn expected_resource_metadata(
    configuration: &Configuration,
    role: &str,
    target: InferenceTarget,
) -> Result<Value> {
    let profile = ResourcePool::profile_for(configuration, role, Some(&target))?;
    let resident_coverage = if configuration.residents.is_empty() {
        Value::Null
    } else {
        ResourcePool::configuration_coverage(configuration, Some(target.clone()))?
    };
    Ok(json!({
        "target":target,
        "profile_id":profile.as_ref().map(|item| item.id.clone()),
        "allocation":profile.map(|item| item.allocation),
        "resident_coverage":resident_coverage,
        "eligible":true,
        "reason_code":"configured_resource_match",
        "permissions_granted":false,
        "weights_attested":false,
        "physical_limits_enforced":false,
    }))
}

pub(crate) fn restrict_backend(
    backend: Arc<dyn InferenceBackend>,
    routes: Option<BTreeMap<String, InferenceTarget>>,
) -> Result<Arc<dyn InferenceBackend>> {
    match routes {
        Some(routes) if routes.is_empty() => Ok(Arc::new(EmptyRouteBackend)),
        Some(routes) => crate::fabric::restrict_inference_roles(backend, routes),
        None => Ok(backend),
    }
}

/// A checkpoint whose original selection contained no eligible inference role
/// must not regain routes from the current registry during continuation.
/// Attempts fail locally and never reach the reconstructed provider backend.
struct EmptyRouteBackend;

impl InferenceBackend for EmptyRouteBackend {
    fn inference_target(&self, _role: &str) -> Result<Option<InferenceTarget>> {
        Ok(None)
    }

    fn availability(&self) -> crate::fabric::Availability {
        crate::fabric::Availability::Unknown
    }

    fn modules(&self) -> Result<Vec<crate::fabric::ModuleDescriptor>> {
        Ok(Vec::new())
    }

    fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
        Ok(Some(Arc::new(Self)))
    }

    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _max_tokens: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        Box::pin(async {
            bail!("No inference role was eligible in the original resource route map")
        })
    }
}

fn require_exact_keys(value: &Value, expected: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("Malformed resource routing record"))?;
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        bail!("Malformed resource routing record shape")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::Generation,
        resource_scheduler::{Allocation, Settings, WorkingProfile},
    };
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    const TASK: &str = "00000000-0000-4000-8000-000000000001";

    struct Fixture {
        db: rusqlite::Connection,
        cfg: Config,
        settings: Value,
        snapshot: Value,
        selection: Value,
    }

    fn fixture() -> Fixture {
        let allocation = Allocation {
            compute_slots: 1,
            host_memory_bytes: 256,
            device_memory_bytes: 256,
        };
        let resources = Configuration {
            settings: Settings {
                label: "test-pool".into(),
                capacity: Allocation {
                    compute_slots: 2,
                    host_memory_bytes: 2048,
                    device_memory_bytes: 2048,
                },
                max_pending: 8,
                max_wait_ms: 1000,
                max_execution_ms: 1000,
                scheduling: None,
            },
            allocation,
            residents: Vec::new(),
            working_profiles: vec![WorkingProfile {
                id: "configured-solver".into(),
                target: crate::backend::ModelTarget {
                    api_base: "http://127.0.0.1:1234/v1".into(),
                    model: "profiled-model".into(),
                },
                roles: vec!["solver".into()],
                allocation,
            }],
        };
        let binding = ResourcePool::configuration_binding(&resources).unwrap();
        let pool_id = "00000000-0000-4000-8000-000000000002";
        let intent = json!({
            "format_version":1,
            "pool_id":pool_id,
            "binding":binding,
            "roles":crate::fabric::protocol::ROLES,
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false,
        });
        let mut selection = json!({
            "format_version":1,
            "pool_id":pool_id,
            "binding":binding,
            "routes":[],
            "unmatched_roles":crate::fabric::protocol::ROLES,
            "excluded_candidates":[],
            "route_scope_sha256":"",
            "eligible":false,
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false,
        });
        selection["route_scope_sha256"] =
            json!(crate::fabric::resource_route_scope(&selection).unwrap());
        let snapshot = json!({
            "modules":[],
            "resource_routing_version":1,
            "resource_routes":selection,
        });
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE events (id INTEGER PRIMARY KEY, task_id TEXT, kind TEXT, source TEXT, payload TEXT);",
        )
        .unwrap();
        insert_event(&db, 1, "routing_resource_intent", "jev-plus", &intent);
        insert_event(&db, 2, "routing_resource_selection", "jev-plus", &selection);
        insert_event(&db, 3, "module_snapshot", "fabric", &snapshot);

        let cfg = Config {
            backend: "openai-local".into(),
            model: "active-model".into(),
            resource_admission: Some(resources),
            ..Config::default()
        };
        Fixture {
            db,
            cfg,
            settings: json!({"resource_routing_version":1}),
            snapshot,
            selection,
        }
    }

    fn insert_event(db: &rusqlite::Connection, id: i64, kind: &str, source: &str, payload: &Value) {
        db.execute(
            "INSERT INTO events(id,task_id,kind,source,payload) VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![id, TASK, kind, source, payload.to_string()],
        )
        .unwrap();
    }

    fn replace_snapshot(fixture: &Fixture, snapshot: &Value) {
        fixture
            .db
            .execute(
                "UPDATE events SET payload=?1 WHERE task_id=?2 AND kind='module_snapshot'",
                rusqlite::params![snapshot.to_string(), TASK],
            )
            .unwrap();
    }

    #[test]
    fn accepts_an_empty_selection_and_returns_the_exact_empty_role_map() {
        let fixture = fixture();
        let (routes, scope) = validate(
            &fixture.db,
            TASK,
            &fixture.cfg,
            &fixture.settings,
            &fixture.snapshot,
        )
        .unwrap()
        .unwrap();
        assert!(routes.is_empty());
        assert_eq!(scope, fixture.selection["route_scope_sha256"]);
    }

    #[test]
    fn rejects_empty_selection_if_any_model_dispatch_was_recorded() {
        let fixture = fixture();
        insert_event(&fixture.db, 4, "model_dispatch", "jev-r", &json!({}));
        assert!(validate(
            &fixture.db,
            TASK,
            &fixture.cfg,
            &fixture.settings,
            &fixture.snapshot,
        )
        .is_err());
    }

    #[test]
    fn rejects_fabricated_modules_roles_and_route_scope() {
        let fabricated_modules = fixture();
        let mut snapshot = fabricated_modules.snapshot.clone();
        snapshot["modules"] = json!([{
            "id":BUILTIN_ID,
            "version":BUILTIN_VERSION,
            "roles":crate::fabric::protocol::ROLES,
        }]);
        replace_snapshot(&fabricated_modules, &snapshot);
        assert!(validate(
            &fabricated_modules.db,
            TASK,
            &fabricated_modules.cfg,
            &fabricated_modules.settings,
            &snapshot,
        )
        .is_err());

        let fabricated_role = fixture();
        let mut selection = fabricated_role.selection.clone();
        selection["routes"] = json!([{
            "role":"operator",
            "module_id":BUILTIN_ID,
            "module_version":BUILTIN_VERSION,
            "resource":{},
        }]);
        selection["eligible"] = json!(true);
        selection["route_scope_sha256"] =
            json!(crate::fabric::resource_route_scope(&selection).unwrap());
        let snapshot = json!({
            "modules":[{
                "id":BUILTIN_ID,
                "version":BUILTIN_VERSION,
                "roles":crate::fabric::protocol::ROLES,
            }],
            "resource_routing_version":1,
            "resource_routes":selection,
        });
        replace_snapshot(&fabricated_role, &snapshot);
        fabricated_role.db.execute(
            "UPDATE events SET payload=?1 WHERE task_id=?2 AND kind='routing_resource_selection'",
            rusqlite::params![selection.to_string(), TASK],
        ).unwrap();
        assert!(validate(
            &fabricated_role.db,
            TASK,
            &fabricated_role.cfg,
            &fabricated_role.settings,
            &snapshot,
        )
        .is_err());

        let fabricated_scope = fixture();
        let mut selection = fabricated_scope.selection.clone();
        selection["route_scope_sha256"] = json!("0".repeat(64));
        let snapshot = json!({
            "modules":[],
            "resource_routing_version":1,
            "resource_routes":selection,
        });
        replace_snapshot(&fabricated_scope, &snapshot);
        fabricated_scope.db.execute(
            "UPDATE events SET payload=?1 WHERE task_id=?2 AND kind='routing_resource_selection'",
            rusqlite::params![selection.to_string(), TASK],
        ).unwrap();
        assert!(validate(
            &fabricated_scope.db,
            TASK,
            &fabricated_scope.cfg,
            &fabricated_scope.settings,
            &snapshot,
        )
        .is_err());
    }

    #[derive(Default)]
    struct BackendCalls {
        metadata: AtomicUsize,
        modules: AtomicUsize,
        availability: AtomicUsize,
        pin: AtomicUsize,
        generation: AtomicUsize,
    }

    struct CallbackBackend(Arc<BackendCalls>);

    impl InferenceBackend for CallbackBackend {
        fn inference_target(&self, _role: &str) -> Result<Option<InferenceTarget>> {
            self.0.metadata.fetch_add(1, Ordering::SeqCst);
            Ok(Some(InferenceTarget::Deterministic))
        }

        fn modules(&self) -> Result<Vec<crate::fabric::ModuleDescriptor>> {
            self.0.modules.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }

        fn availability(&self) -> crate::fabric::Availability {
            self.0.availability.fetch_add(1, Ordering::SeqCst);
            crate::fabric::Availability::Connected
        }

        fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
            self.0.pin.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        fn generate<'a>(
            &'a self,
            _role: &'a str,
            _prompt: &'a str,
            _max_tokens: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                self.0.generation.fetch_add(1, Ordering::SeqCst);
                Ok(Generation {
                    text: "unexpected".into(),
                    model: "unexpected".into(),
                    input_tokens: None,
                    output_tokens: None,
                    elapsed_ms: 0.0,
                    queue_wait_ms: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn empty_route_restoration_does_not_call_underlying_backend() {
        let calls = Arc::new(BackendCalls::default());
        let backend: Arc<dyn InferenceBackend> = Arc::new(CallbackBackend(calls.clone()));
        let restored = restrict_backend(backend, Some(BTreeMap::new())).unwrap();
        assert!(restored.inference_target("solver").unwrap().is_none());
        assert!(restored.modules().unwrap().is_empty());
        assert!(restored
            .generate_for(TASK, "solver", "prompt", 8)
            .await
            .is_err());
        let pinned = restored.pin().unwrap().unwrap();
        assert!(pinned.inference_target("planner").unwrap().is_none());
        assert_eq!(calls.metadata.load(Ordering::SeqCst), 0);
        assert_eq!(calls.modules.load(Ordering::SeqCst), 0);
        assert_eq!(calls.availability.load(Ordering::SeqCst), 0);
        assert_eq!(calls.pin.load(Ordering::SeqCst), 0);
        assert_eq!(calls.generation.load(Ordering::SeqCst), 0);
    }
}
