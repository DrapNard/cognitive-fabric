//! Operator-only bounded LM Studio native residency observations and lifecycle.
use crate::{backend, credentials::Bearer, security};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
pub mod lifecycle;
pub mod managed;
pub mod planning;

pub const CREDENTIAL_ENV: &str = "COGFABRIC_RESIDENCY_API_KEY";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Instance {
    pub id: String,
    pub config: InstanceConfig,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstanceConfig {
    pub context_length: u64,
    /// Provider-reported prediction slots; omitted means unknown, not unlimited.
    /// Advisory only: this is not proof of concurrent execution or free capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel: Option<u32>,
    /// Server-reported KV-cache placement, if this LM Studio version exposes it.
    /// `None` means unknown/omitted and is never treated as CPU placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offload_kv_cache_to_gpu: Option<bool>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Model {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub loaded_instances: Vec<Instance>,
}
#[derive(Deserialize)]
struct Inventory {
    models: Vec<Model>,
}

fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn parse(bytes: &[u8]) -> Result<Vec<Model>> {
    let inventory: Inventory = crate::strict_json::value(bytes, 262144)
        .and_then(|value| Ok(serde_json::from_value(value)?))
        .map_err(|_| anyhow::anyhow!("Invalid native model inventory"))?;
    if inventory.models.len() > 256 {
        bail!("Native model inventory exceeds bounds")
    }
    let mut keys = BTreeSet::new();
    let mut instances = BTreeSet::new();
    for model in &inventory.models {
        if !identity(&model.key)
            || !keys.insert(&model.key)
            || !matches!(model.kind.as_str(), "llm" | "embedding" | "decision")
            || model.loaded_instances.len() > 32
        {
            bail!("Invalid native model identity or bounds")
        }
        for instance in &model.loaded_instances {
            if !identity(&instance.id)
                || !instances.insert(&instance.id)
                || !(1..=16_777_216).contains(&instance.config.context_length)
                || instance
                    .config
                    .parallel
                    .is_some_and(|slots| !(1..=65536).contains(&slots))
            {
                bail!("Invalid native model instance or context bounds")
            }
        }
    }
    if instances.len() > 1024 {
        bail!("Native loaded instance capacity exceeded")
    }
    Ok(inventory.models)
}

fn endpoint(base: &str) -> Result<url::Url> {
    // Reuse literal-loopback validation but require an explicit native API path.
    let mut url = backend::local_endpoint(base)?;
    if url.path() != "/api/v1/chat/completions" {
        bail!("Residency requires an explicit /api/v1 native API base")
    }
    url.set_path("/api/v1/models");
    Ok(url)
}

async fn request(
    base: &str,
    authenticate: bool,
    operation: Option<&str>,
    body: Option<Value>,
    timeout: Duration,
) -> Result<Value> {
    let mut endpoint = endpoint(base)?;
    if let Some(operation) = operation {
        endpoint.set_path(&format!("/api/v1/models/{operation}"));
    }
    let credential = if authenticate {
        Some(Bearer::new(std::env::var(CREDENTIAL_ENV).map_err(
            |_| anyhow::anyhow!("Dedicated residency credential is unavailable"),
        )?)?)
    } else {
        None
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()?;
    let mut request = match body {
        Some(body) => client.post(endpoint.clone()).json(&body),
        None => client.get(endpoint.clone()),
    };
    if let Some(credential) = &credential {
        request = credential.apply(request)?;
    }
    let mut response = request.send().await?;
    if !response.status().is_success() {
        bail!("Native model inventory returned a non-success status")
    }
    if response.content_length().is_some_and(|n| n > 262144) {
        bail!("Native inventory response exceeds limit")
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 262144 {
            bail!("Native inventory response exceeds limit")
        }
        bytes.extend_from_slice(&chunk);
    }
    if let Some(credential) = &credential {
        credential.reject_echo(&bytes)?;
    }
    let value = crate::strict_json::value(&bytes, 262144)
        .map_err(|_| anyhow::anyhow!("Invalid native inventory JSON"))?;
    if let Some(credential) = &credential {
        credential.reject_echo(&serde_json::to_vec(&value)?)?;
    }
    Ok(value)
}

pub async fn status(base: &str, authenticate: bool) -> Result<Value> {
    security::authorize("human-cli", "model.residency.inspect")?;
    let start = Instant::now();
    let value = request(base, authenticate, None, None, Duration::from_secs(3)).await?;
    let models = parse(&serde_json::to_vec(&value)?)?;
    let loaded = models
        .iter()
        .map(|m| m.loaded_instances.len())
        .sum::<usize>();
    Ok(
        json!({"format_version":1,"adapter":"lmstudio-native-v1","endpoint":endpoint(base)?.as_str(),
        "models":models,"loaded_instances":loaded,"observation_ms":start.elapsed().as_secs_f64()*1000.0,
        "server_reported":true,"weights_attested":false,"generation_tested":false,
        "ownership_established":false,"model_mutation_performed":false,
        "prediction_concurrency":{
            "scope":"per_loaded_instance",
            "configured_slots_are_not_free_slots":true,
            "provider_execution_verified":false,
            "instances":models.iter().flat_map(|model|model.loaded_instances.iter().map(move |instance|json!({
                "model_key":model.key,"instance_id":instance.id,
                "configured_prediction_slots":instance.config.parallel,
                "queue_depth":Value::Null,"active_predictions":Value::Null
            }))).collect::<Vec<_>>()
        }}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_inventory_distinguishes_downloaded_models_and_loaded_instances() {
        let fixture = json!({"models":[{"key":"small","type":"llm","loaded_instances":[{"id":"small-instance","config":{"context_length":2048,"parallel":1}}]},
            {"key":"embedding","type":"embedding","loaded_instances":[],"publisher":"fixture"}]});
        let models = parse(&serde_json::to_vec(&fixture).unwrap()).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].loaded_instances[0].id, "small-instance");
        assert!(models[1].loaded_instances.is_empty());
        for mutation in 0..5 {
            let mut invalid = fixture.clone();
            match mutation {
                0 => invalid["models"][1]["key"] = json!("small"),
                1 => {
                    invalid["models"][1]["loaded_instances"] =
                        invalid["models"][0]["loaded_instances"].clone()
                }
                2 => {
                    invalid["models"][0]["loaded_instances"][0]["config"]["context_length"] =
                        json!(0)
                }
                3 => invalid["models"][0]["type"] = json!("arbitrary-executable"),
                _ => invalid["models"][0]["key"] = json!("x".repeat(257)),
            }
            assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        assert!(parse(br#"{"models":[{"key":"small","type":"llm"}]}"#).is_err());
        assert!(security::authorize("jev-plus", "model.residency.inspect").is_err());
    }
    #[tokio::test]
    async fn native_http_inventory_rejects_duplicate_fields_before_observation() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for body in [
            r#"{"models":[],"models":[]}"#,
            r#"{"models":[],"metadata":{"flag":true,"flag":false}}"#,
            r#"{"models":[],"metadata":{"flag":true}}"#,
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}/api/v1/", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let mut chunk = [0; 1024];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                    assert!(request.len() < 8192);
                }
                assert!(request.starts_with(b"GET /api/v1/models "));
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            });
            let result = status(&base, false).await;
            if body.contains("false") || body == r#"{"models":[],"models":[]}"# {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    "Invalid native inventory JSON"
                );
            } else {
                assert!(result.is_ok());
            }
            server.await.unwrap();
        }
    }

    #[test]
    fn native_endpoint_never_inherits_inference_paths_or_remote_access() {
        assert_eq!(
            endpoint("http://localhost:1234/api/v1/").unwrap().as_str(),
            "http://127.0.0.1:1234/api/v1/models"
        );
        for base in [
            "http://127.0.0.1:1234/v1",
            "https://127.0.0.1/api/v1",
            "http://example.com/api/v1",
            "http://user@127.0.0.1/api/v1",
            "http://127.0.0.1/api/v1?token=x",
        ] {
            assert!(endpoint(base).is_err());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mutation {
    Load {
        model: String,
        context_length: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offload_kv_cache_to_gpu: Option<bool>,
    },
    Unload {
        instance_id: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    state: String,
    adapter: String,
    endpoint: String,
    action: Mutation,
    external_outcome: String,
    #[serde(default)]
    managed: Option<bool>,
    #[serde(default)]
    receipt_id: Option<String>,
    #[serde(default)]
    permissions_granted: Option<bool>,
}

/// Observe a recorded operator intent without retrying or resolving its outcome.
/// Matching inventory cannot establish causality or exclusive instance ownership.
pub async fn inspect_operation(
    store: &crate::store::Store,
    task: &str,
    base: &str,
    authenticate: bool,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.inspect")?;
    uuid::Uuid::parse_str(task)?;
    let expected = endpoint(base)?;
    let (state, intent) = {
        let db = store.lock()?;
        let mut statement = db.prepare("SELECT CASE WHEN length(CAST(t.status AS BLOB))<=64 THEN t.status END, CASE WHEN length(CAST(e.payload AS BLOB))<=4096 THEN e.payload END FROM tasks t JOIN events e ON e.task_id=t.id WHERE t.id=?1 AND e.kind='model_residency' AND e.source='human-cli' AND CASE WHEN length(CAST(e.payload AS BLOB))<=4096 THEN json_extract(e.payload,'$.state')='requested' ELSE 1 END ORDER BY e.id LIMIT 2")?;
        let mut rows = statement.query([task])?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow::anyhow!("No recorded operator residency intent for task"))?;
        let state: String = row.get(0)?;
        let raw: String = row.get(1)?;
        if rows.next()?.is_some() {
            bail!("Ambiguous recorded residency intent")
        }
        let intent: Intent = serde_json::from_str(&raw)
            .map_err(|_| anyhow::anyhow!("Invalid recorded residency intent"))?;
        (state, intent)
    };
    if intent.state != "requested"
        || intent.adapter != "lmstudio-native-v1"
        || intent.external_outcome != "unknown"
        || intent.endpoint != expected.as_str()
    {
        bail!("Recorded residency intent does not match the explicit native endpoint")
    }
    validate_action(&intent.action)?;
    match intent.managed {
        Some(true) => {
            if intent.permissions_granted != Some(false) {
                bail!("Managed residency inspection found invalid permission metadata")
            }
            match (&intent.action, intent.receipt_id.as_deref()) {
                (Mutation::Load { .. }, None) => {}
                (Mutation::Unload { .. }, Some(receipt_id)) => {
                    uuid::Uuid::parse_str(receipt_id)?;
                }
                _ => bail!("Managed residency intent has inconsistent receipt metadata"),
            }
        }
        None if intent.receipt_id.is_none() && intent.permissions_granted.is_none() => {}
        _ => bail!("Residency intent has unsupported managed metadata"),
    }
    let inventory = status(base, authenticate).await?;
    let models = parse(&serde_json::to_vec(&inventory)?)?;
    let matching: Vec<&str> = match &intent.action {
        Mutation::Load {
            model,
            context_length,
            offload_kv_cache_to_gpu,
        } => models
            .iter()
            .filter(|m| &m.key == model)
            .flat_map(|m| &m.loaded_instances)
            .filter(|i| {
                i.config.context_length == u64::from(*context_length)
                    && offload_kv_cache_to_gpu
                        .is_none_or(|expected| i.config.offload_kv_cache_to_gpu == Some(expected))
            })
            .map(|i| i.id.as_str())
            .collect(),
        Mutation::Unload { instance_id } => models
            .iter()
            .flat_map(|m| &m.loaded_instances)
            .filter(|i| &i.id == instance_id)
            .map(|i| i.id.as_str())
            .collect(),
    };
    let observed = match intent.action {
        Mutation::Load { .. } => !matching.is_empty(),
        Mutation::Unload { .. } => matching.is_empty(),
    };
    Ok(
        json!({"format_version":1,"task_id":task,"task_state_at_read":state,
        "action":intent.action,"endpoint":expected.as_str(),"desired_state_observed":observed,
        "matching_instance_ids":matching,"server_reported":true,
        "operation_outcome":"not_inferred","ownership_established":false,
        "model_mutation_performed":false,"task_updated":false,
        "automatic_retry":false,"automatic_cleanup":false}),
    )
}

fn validate_action(action: &Mutation) -> Result<()> {
    match action {
        Mutation::Load {
            model,
            context_length,
            ..
        } if !identity(model) || !(1..=32768).contains(context_length) => {
            bail!("Invalid load identity or context ceiling")
        }
        Mutation::Unload { instance_id } if !identity(instance_id) => {
            bail!("Invalid unload instance identity")
        }
        _ => Ok(()),
    }
}

/// Explicit operator mutation; no model capability, automatic retry or implicit unload.
pub async fn mutate(
    store: &crate::store::Store,
    base: &str,
    authenticate: bool,
    action: Mutation,
    human_approve: bool,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve {
        bail!("Model residency changes require explicit operator approval")
    }
    endpoint(base)?;
    validate_action(&action)?;
    let _leases = mutation_leases(store)?;
    mutate_owned(store, base, authenticate, action, None).await
}

struct MutationContext<'a> {
    plan_version: u32,
    inventory_sha256: &'a str,
    inventory_kv_sha256: Option<&'a str>,
    plan_task: &'a str,
    step: usize,
}
fn mutation_leases(
    store: &crate::store::Store,
) -> Result<(
    crate::recovery::ExecutionLease,
    crate::recovery::ExecutionLease,
)> {
    let path = store
        .runtime_lock
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Residency changes require a persistent store"))?;
    let mut resource_path = path.as_os_str().to_os_string();
    resource_path.push(".resources");
    let resource =
        crate::recovery::ExecutionLease::acquire(std::path::Path::new(&resource_path), true)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Residency change refused while configured resource ownership is active"
                )
            })?;
    let runtime = crate::recovery::ExecutionLease::acquire(path, true).map_err(|_| {
        anyhow::anyhow!("Residency change refused while runtime execution is active")
    })?;
    Ok((resource, runtime))
}
async fn mutate_owned(
    store: &crate::store::Store,
    base: &str,
    authenticate: bool,
    action: Mutation,
    context: Option<MutationContext<'_>>,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    endpoint(base)?;
    validate_action(&action)?;
    let before = status(base, authenticate).await?;
    let models = parse(&serde_json::to_vec(&before)?)?;
    if let Some(context) = &context {
        if planning::fingerprint(&models)? != context.inventory_sha256 {
            bail!("Residency plan inventory changed before mutation")
        }
        if let Some(digest) = context.inventory_kv_sha256 {
            if planning::kv_fingerprint(&models)? != digest {
                bail!("Residency plan KV-cache placement changed before mutation")
            }
        }
    }
    let (operation, body) = match &action {
        Mutation::Load {
            model,
            context_length,
            offload_kv_cache_to_gpu,
        } => {
            let selected = models.iter().find(|m| &m.key == model).ok_or_else(|| {
                anyhow::anyhow!("Load requires an installed model key from native inventory")
            })?;
            if !selected.loaded_instances.is_empty() {
                bail!("Model already has loaded instances; inspect and reuse them explicitly")
            }
            if offload_kv_cache_to_gpu.is_some()
                && !matches!(selected.kind.as_str(), "llm" | "decision")
            {
                bail!(
                    "Explicit KV-cache GPU placement is supported only for LLM and decision models"
                )
            }
            let mut body = json!({"model":model,"context_length":context_length});
            if let Some(value) = offload_kv_cache_to_gpu {
                body["offload_kv_cache_to_gpu"] = json!(value);
                body["echo_load_config"] = json!(true);
            }
            ("load", body)
        }
        Mutation::Unload { instance_id } => {
            if !models
                .iter()
                .any(|m| m.loaded_instances.iter().any(|i| &i.id == instance_id))
            {
                bail!("Unload requires an existing exact instance ID")
            }
            ("unload", json!({"instance_id":instance_id}))
        }
    };
    let task = store.create_task("Operator native model residency change", "fast")?;
    if let Some(context) = &context {
        store.event(context.plan_task,"residency_plan_step","human-cli",json!({"format_version":context.plan_version,"step":context.step,"operation_task_id":task,"permissions_granted":false}))?;
    }
    // Persist exact bounded operator intent before the external effect.
    store.event(&task, "model_residency", "human-cli", json!({"state":"requested","adapter":"lmstudio-native-v1","endpoint":endpoint(base)?.as_str(),"action":action,"external_outcome":"unknown"}))?;
    // A manual provider mutation invalidates every managed receipt whose shared
    // full-inventory baseline can no longer be advanced by a known managed
    // transition. The invalidation is durable before the external POST.
    managed::quarantine_before_manual_mutation(store, &task, endpoint(base)?.as_str())?;
    let started = Instant::now();
    let outcome = async {
        let response = request(base, authenticate, Some(operation), Some(body), Duration::from_secs(60)).await?;
        let instance = response["instance_id"].as_str().filter(|id| identity(id))
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid residency response instance ID"))?;
        match &action {
            Mutation::Load { model, offload_kv_cache_to_gpu, .. } => {
                if response["status"] != "loaded" || !matches!(response["type"].as_str(), Some("llm" | "embedding" | "decision"))
                    || !response["load_time_seconds"].as_f64().is_some_and(|v| v.is_finite() && v >= 0.0) {
                    bail!("Invalid native load response")
                }
                if let Some(expected) = offload_kv_cache_to_gpu {
                    if !matches!(response["type"].as_str(), Some("llm" | "decision"))
                        || !models.iter().find(|m| &m.key == model)
                            .is_some_and(|selected| response["type"].as_str() == Some(selected.kind.as_str()))
                        || response["load_config"]["offload_kv_cache_to_gpu"].as_bool() != Some(*expected)
                    {
                        bail!("Native load response did not echo the requested KV-cache GPU placement")
                    }
                }
                if models.iter().any(|m| m.loaded_instances.iter().any(|i| i.id == instance)) {
                    bail!("Native load returned a pre-existing instance")
                }
            }
            Mutation::Unload { instance_id } if instance != instance_id => bail!("Native unload returned another instance"),
            _ => {}
        }
        let after = status(base, authenticate).await?;
        let observed = parse(&serde_json::to_vec(&after)?)?;
        let confirmed = match &action {
            Mutation::Load { model, context_length, offload_kv_cache_to_gpu } => observed.iter().any(|m| &m.key == model && m.loaded_instances.iter().any(|i| i.id == instance && i.config.context_length == u64::from(*context_length) && offload_kv_cache_to_gpu.is_none_or(|expected| i.config.offload_kv_cache_to_gpu == Some(expected)))),
            Mutation::Unload { instance_id } => !observed.iter().any(|m| m.loaded_instances.iter().any(|i| &i.id == instance_id)),
        };
        if !confirmed { bail!("Native residency change was not confirmed by fresh inventory") }
        if context.is_some() || matches!(&action, Mutation::Load { offload_kv_cache_to_gpu: Some(_), .. }) {
            planning::validate_transition(&models,&observed,&action,instance)?;
        }
        Ok::<_, anyhow::Error>(json!({"task_id":task,"operation":operation,"instance_id":instance,"server_confirmed":true,
            "after_inventory_sha256":planning::fingerprint(&observed)?,
            "after_inventory_kv_sha256":planning::kv_fingerprint(&observed)?,
            "elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"provider_load_seconds":response.get("load_time_seconds"),
            "ownership_established":false,"weights_attested":false,"generation_tested":false,"automatic_cleanup_performed":false}))
    }.await;
    match outcome {
        Ok(report) => {
            finish_operation(
                store,
                &task,
                json!({"state":"server_confirmed","report":report}),
                "completed",
                "Server-confirmed operator residency operation; no generation tested",
            )?;
            Ok(report)
        }
        Err(_) => {
            // Never persist provider error bodies or retry an uncertain external effect.
            finish_operation(
                store,
                &task,
                json!({"state":"outcome_unknown","automatic_retry":false,"automatic_cleanup":false}),
                "interrupted",
                "Residency outcome unknown; inspect native inventory before another operation",
            )?;
            bail!("Residency outcome unknown for task {task}; inspect inventory before retrying")
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[tokio::test]
    async fn mutations_require_approval_valid_bounds_and_exclusive_runtime_ownership() {
        let home =
            std::env::temp_dir().join(format!("fabric-residency-lease-{}", uuid::Uuid::new_v4()));
        let cfg = crate::config::Config {
            home: home.clone(),
            ..Default::default()
        };
        cfg.prepare().unwrap();
        let store = crate::store::Store::open(&cfg.db()).unwrap();
        let base = "http://127.0.0.1:1/api/v1";
        let load = Mutation::Load {
            model: "installed".into(),
            context_length: 2048,
            offload_kv_cache_to_gpu: None,
        };
        assert!(mutate(&store, base, false, load.clone(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("approval"));
        assert!(mutate(
            &store,
            base,
            false,
            Mutation::Load {
                model: "installed".into(),
                context_length: 32769,
                offload_kv_cache_to_gpu: None,
            },
            true
        )
        .await
        .is_err());
        let live = store.execution_lease().unwrap();
        assert!(mutate(&store, base, false, load, true)
            .await
            .unwrap_err()
            .to_string()
            .contains("runtime execution"));
        assert!(crate::security::authorize("jev-r", "model.residency.change").is_err());
        let count: i64 = store
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(live);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
}

fn finish_operation(
    store: &crate::store::Store,
    task: &str,
    payload: Value,
    state: &str,
    answer: &str,
) -> Result<()> {
    let raw = serde_json::to_string(&payload)?;
    if raw.len() > 4096 {
        bail!("Residency completion metadata exceeds bounds")
    }
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let inserted = tx.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT id,'model_residency','human-cli',?2,unixepoch() FROM tasks WHERE id=?1 AND status='running'", rusqlite::params![task,raw])?;
    if inserted != 1 {
        bail!("Residency operation anchor is missing or no longer running")
    }
    let changed = tx.execute(
        "UPDATE tasks SET status=?2,answer=?3,updated=unixepoch() WHERE id=?1 AND status='running'",
        rusqlite::params![task, state, answer],
    )?;
    if changed != 1 {
        bail!("Residency operation status update failed")
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    #[test]
    fn operation_completion_is_atomic_under_audit_and_status_failures() {
        let store = crate::store::Store::in_memory().unwrap();
        for failure in ["audit", "status", "none"] {
            let task = store
                .create_task("Residency test operation", "fast")
                .unwrap();
            match failure {
                "audit" => store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_completion BEFORE INSERT ON events WHEN NEW.kind='model_residency' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap(),
                "status" => store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_completion BEFORE UPDATE ON tasks BEGIN SELECT RAISE(ABORT,'status unavailable'); END;").unwrap(),
                _ => {}
            }
            let result = finish_operation(
                &store,
                &task,
                json!({"state":"server_confirmed"}),
                "completed",
                "Server observation only",
            );
            let trace = store.task(&task).unwrap();
            if failure == "none" {
                result.unwrap();
                assert_eq!(trace["task"]["status"], "completed");
                assert!(finish_operation(
                    &store,
                    &task,
                    json!({"state":"outcome_unknown"}),
                    "interrupted",
                    "unknown"
                )
                .is_err());
            } else {
                assert!(result.is_err());
                assert_eq!(trace["task"]["status"], "running");
                assert!(trace["events"].as_array().unwrap().is_empty());
                store
                    .lock()
                    .unwrap()
                    .execute_batch("DROP TRIGGER refuse_completion")
                    .unwrap();
                finish_operation(
                    &store,
                    &task,
                    json!({"state":"outcome_unknown"}),
                    "interrupted",
                    "Unknown external outcome",
                )
                .unwrap();
                assert_eq!(store.task(&task).unwrap()["task"]["status"], "interrupted");
            }
        }
        assert!(finish_operation(
            &store,
            &uuid::Uuid::new_v4().to_string(),
            json!({}),
            "completed",
            "missing"
        )
        .is_err());
    }
}
