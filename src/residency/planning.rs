//! Bounded operator-reviewed residency reconciliation; no model actor can apply it.
use super::*;
use crate::store::{sha256, Store};
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnsureLoaded {
    pub model: String,
    pub context_length: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offload_kv_cache_to_gpu: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub ensure_loaded: Vec<EnsureLoaded>,
    #[serde(default)]
    pub release_instances: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reuse {
    pub model: String,
    pub context_length: u32,
    pub instance_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offload_kv_cache_to_gpu: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub format_version: u32,
    pub adapter: String,
    pub endpoint: String,
    pub created_unix: u64,
    pub inventory_sha256: String,
    /// Independent, version-2 snapshot of optional provider KV placement.
    /// The legacy inventory digest intentionally remains byte-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_kv_sha256: Option<String>,
    pub request: Request,
    pub reused: Vec<Reuse>,
    pub actions: Vec<Mutation>,
    pub digest: String,
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn canonical_request(request: &Request) -> Result<Request> {
    if request.ensure_loaded.is_empty()
        || request.ensure_loaded.len() > 8
        || request.release_instances.len() > 8
    {
        bail!("Residency plan request exceeds bounds")
    }
    let mut seen = BTreeSet::new();
    for ensure in &request.ensure_loaded {
        if !identity(&ensure.model) || !(1..=32768).contains(&ensure.context_length) {
            bail!("Invalid desired model or context ceiling")
        }
        if !seen.insert(ensure.model.clone()) {
            bail!("Duplicate desired residency model")
        }
    }
    let mut releases = BTreeSet::new();
    for id in &request.release_instances {
        if !identity(id) || !releases.insert(id) {
            bail!("Invalid or duplicate explicit release instance")
        }
    }
    let mut request = request.clone();
    request.ensure_loaded.sort_by(|a, b| a.model.cmp(&b.model));
    request.release_instances.sort();
    Ok(request)
}
// This is the exact pre-GPU inventory projection used by format-version-1
// plans and managed receipt baselines. Never let a newly exposed provider
// field reinterpret those durable hashes.
#[derive(Serialize)]
struct LegacyInstance<'a> {
    id: &'a str,
    config: LegacyConfig,
}
#[derive(Serialize)]
struct LegacyConfig {
    context_length: u64,
}
#[derive(Serialize)]
struct LegacyModel<'a> {
    key: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    loaded_instances: Vec<LegacyInstance<'a>>,
}
fn canonical_inventory(models: &[Model]) -> Vec<LegacyModel<'_>> {
    let mut models: Vec<_> = models
        .iter()
        .map(|model| {
            let mut loaded_instances: Vec<_> = model
                .loaded_instances
                .iter()
                .map(|instance| LegacyInstance {
                    id: &instance.id,
                    config: LegacyConfig {
                        context_length: instance.config.context_length,
                    },
                })
                .collect();
            loaded_instances.sort_by(|a, b| a.id.cmp(b.id));
            LegacyModel {
                key: &model.key,
                kind: &model.kind,
                loaded_instances,
            }
        })
        .collect();
    models.sort_by(|a, b| a.key.cmp(b.key));
    models
}
pub(super) fn fingerprint(models: &[Model]) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&canonical_inventory(models))?))
}

pub(super) fn kv_fingerprint(models: &[Model]) -> Result<String> {
    let mut entries: Vec<_> = models
        .iter()
        .flat_map(|model| {
            model.loaded_instances.iter().map(move |instance| {
                (
                    model.key.as_str(),
                    instance.id.as_str(),
                    instance.config.offload_kv_cache_to_gpu,
                )
            })
        })
        .collect();
    entries.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    Ok(sha256(&serde_json::to_vec(&entries)?))
}
pub(super) fn validate_transition(
    before: &[Model],
    after: &[Model],
    action: &Mutation,
    instance: &str,
) -> Result<()> {
    let mut expected = before.to_vec();
    match action {
        Mutation::Unload { instance_id } => {
            for model in &mut expected {
                model.loaded_instances.retain(|i| &i.id != instance_id)
            }
        }
        Mutation::Load {
            model,
            context_length,
            offload_kv_cache_to_gpu,
        } => {
            let selected = expected
                .iter_mut()
                .find(|m| &m.key == model)
                .ok_or_else(|| anyhow::anyhow!("Missing installed planned model"))?;
            if offload_kv_cache_to_gpu.is_some()
                && !matches!(selected.kind.as_str(), "llm" | "decision")
            {
                bail!(
                    "Explicit KV-cache GPU placement is supported only for LLM and decision models"
                )
            }
            selected.loaded_instances.push(Instance {
                id: instance.to_owned(),
                config: InstanceConfig {
                    context_length: u64::from(*context_length),
                    offload_kv_cache_to_gpu: *offload_kv_cache_to_gpu,
                },
            });
        }
    }
    if fingerprint(&expected)? != fingerprint(after)? {
        bail!("Unrelated native inventory changed during planned mutation")
    }
    for before_model in before {
        for before_instance in &before_model.loaded_instances {
            let after_instance = after
                .iter()
                .find(|m| m.key == before_model.key)
                .and_then(|m| {
                    m.loaded_instances
                        .iter()
                        .find(|i| i.id == before_instance.id)
                });
            if let Some(after_instance) = after_instance {
                if before_instance.config.offload_kv_cache_to_gpu
                    != after_instance.config.offload_kv_cache_to_gpu
                {
                    bail!("Existing native instance KV-cache placement changed during residency mutation")
                }
            }
        }
    }
    if let Mutation::Load {
        model,
        offload_kv_cache_to_gpu: Some(expected),
        ..
    } = action
    {
        let loaded = after
            .iter()
            .find(|m| &m.key == model)
            .and_then(|m| m.loaded_instances.iter().find(|i| i.id == instance));
        if loaded.and_then(|i| i.config.offload_kv_cache_to_gpu) != Some(*expected) {
            bail!("Provider inventory did not confirm the requested KV-cache GPU placement")
        }
    }
    Ok(())
}
impl Plan {
    fn checksum(&self) -> Result<String> {
        let legacy = (
            self.format_version,
            &self.adapter,
            &self.endpoint,
            self.created_unix,
            &self.inventory_sha256,
            &self.request,
            &self.reused,
            &self.actions,
        );
        if self.format_version == 1 {
            Ok(sha256(&serde_json::to_vec(&legacy)?))
        } else {
            Ok(sha256(&serde_json::to_vec(&(
                legacy,
                &self.inventory_kv_sha256,
            ))?))
        }
    }
    pub fn validate(&self) -> Result<()> {
        let observed = now()?;
        if !matches!(self.format_version, 1 | 2)
            || self.adapter != "lmstudio-native-v1"
            || self.endpoint.len() > 2048
            || self.created_unix > observed
            || observed - self.created_unix > 300
            || self.inventory_sha256.len() != 64
            || !self
                .inventory_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.actions.len() > 8
            || self.reused.len() > 8
            || canonical_request(&self.request)? != self.request
        {
            bail!("Invalid or expired residency plan")
        }
        let has_gpu_choice = self
            .request
            .ensure_loaded
            .iter()
            .any(|item| item.offload_kv_cache_to_gpu.is_some());
        let has_gpu_action = self.actions.iter().any(|action| {
            matches!(
                action,
                Mutation::Load {
                    offload_kv_cache_to_gpu: Some(_),
                    ..
                }
            )
        });
        let has_gpu_reuse = self
            .reused
            .iter()
            .any(|reuse| reuse.offload_kv_cache_to_gpu.is_some());
        match (
            self.format_version,
            has_gpu_choice,
            self.inventory_kv_sha256.as_deref(),
        ) {
            (1, false, None) if !has_gpu_action && !has_gpu_reuse => {}
            (2, true, Some(hash)) if valid_hash(hash) => {}
            _ => bail!("Residency plan version and KV-cache placement snapshot differ"),
        }
        for action in &self.actions {
            validate_action(action)?;
        }
        for reused in &self.reused {
            if !identity(&reused.model)
                || !identity(&reused.instance_id)
                || !(1..=32768).contains(&reused.context_length)
            {
                bail!("Invalid reused instance")
            }
            let requested = self
                .request
                .ensure_loaded
                .iter()
                .find(|item| item.model == reused.model)
                .ok_or_else(|| {
                    anyhow::anyhow!("Reused model is absent from the residency request")
                })?;
            if requested.context_length != reused.context_length
                || requested.offload_kv_cache_to_gpu != reused.offload_kv_cache_to_gpu
            {
                bail!("Reused instance configuration differs from the approved request")
            }
        }
        if self.digest != self.checksum()? {
            bail!("Residency plan content changed")
        }
        Ok(())
    }
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn build(base: &str, request: &Request, models: &[Model], created_unix: u64) -> Result<Plan> {
    let request = canonical_request(request)?;
    let releasing = request.release_instances.iter().collect::<BTreeSet<_>>();
    let instances = models
        .iter()
        .flat_map(|m| &m.loaded_instances)
        .map(|i| &i.id)
        .collect::<BTreeSet<_>>();
    if releasing.iter().any(|id| !instances.contains(id)) {
        bail!("Plan release requires an existing exact instance ID")
    }
    let mut actions = request
        .release_instances
        .iter()
        .map(|id| Mutation::Unload {
            instance_id: id.clone(),
        })
        .collect::<Vec<_>>();
    let mut reused = Vec::new();
    for ensure in &request.ensure_loaded {
        let model = models
            .iter()
            .find(|m| m.key == ensure.model)
            .ok_or_else(|| anyhow::anyhow!("Planned model is not installed"))?;
        let remaining = model
            .loaded_instances
            .iter()
            .filter(|i| !releasing.contains(&i.id))
            .collect::<Vec<_>>();
        if let Some(instance) = remaining
            .iter()
            .filter(|i| {
                i.config.context_length == u64::from(ensure.context_length)
                    && ensure.offload_kv_cache_to_gpu.map_or(true, |expected| {
                        i.config.offload_kv_cache_to_gpu == Some(expected)
                    })
            })
            .min_by(|a, b| a.id.cmp(&b.id))
        {
            reused.push(Reuse {
                model: ensure.model.clone(),
                context_length: ensure.context_length,
                instance_id: instance.id.clone(),
                offload_kv_cache_to_gpu: ensure.offload_kv_cache_to_gpu,
            });
        } else {
            if !remaining.is_empty() {
                bail!("Incompatible loaded contexts require explicit release before replacement")
            }
            actions.push(Mutation::Load {
                model: ensure.model.clone(),
                context_length: ensure.context_length,
                offload_kv_cache_to_gpu: ensure.offload_kv_cache_to_gpu,
            });
        }
    }
    if actions.len() > 8 {
        bail!("Residency plan exceeds eight external operations")
    }
    let explicit_gpu = request
        .ensure_loaded
        .iter()
        .any(|item| item.offload_kv_cache_to_gpu.is_some());
    if explicit_gpu {
        for ensure in &request.ensure_loaded {
            if ensure.offload_kv_cache_to_gpu.is_some() {
                let model = models
                    .iter()
                    .find(|m| m.key == ensure.model)
                    .ok_or_else(|| anyhow::anyhow!("Planned model is not installed"))?;
                if !matches!(model.kind.as_str(), "llm" | "decision") {
                    bail!("Explicit KV-cache GPU placement is supported only for LLM and decision models")
                }
            }
        }
    }
    let mut plan = Plan {
        format_version: if explicit_gpu { 2 } else { 1 },
        adapter: "lmstudio-native-v1".into(),
        endpoint: endpoint(base)?.to_string(),
        created_unix,
        inventory_sha256: fingerprint(models)?,
        inventory_kv_sha256: explicit_gpu.then(|| kv_fingerprint(models)).transpose()?,
        request,
        reused,
        actions,
        digest: String::new(),
    };
    plan.digest = plan.checksum()?;
    Ok(plan)
}
/// Read-only provider inventory; this never initializes a Store or performs a POST.
pub async fn preview(base: &str, authenticate: bool, request: &Request) -> Result<Plan> {
    security::authorize("human-cli", "model.residency.inspect")?;
    canonical_request(request)?;
    endpoint(base)?;
    let inventory = status(base, authenticate).await?;
    let models = parse(&serde_json::to_vec(&inventory)?)?;
    build(base, request, &models, now()?)
}
fn claim(store: &Store, plan: &Plan) -> Result<String> {
    let mut db = store.lock()?;
    let started = Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > Duration::from_secs(5)),
    );
    let result = claim_from(&mut db, plan);
    db.progress_handler(0, None::<fn() -> bool>);
    result
}
fn claim_from(db: &mut rusqlite::Connection, plan: &Plan) -> Result<String> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut statement=tx.prepare("SELECT CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=32768 THEN payload END FROM events WHERE kind='residency_plan' ORDER BY id LIMIT 1025")?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 1024 {
            bail!("Residency plan receipt capacity exhausted")
        };
        let source: String = row.get(0)?;
        let bytes: String = row.get(1)?;
        let value = crate::strict_json::value(bytes.as_bytes(), 32768)?;
        if source != "human-cli"
            || !matches!(value["format_version"].as_u64(), Some(1 | 2))
            || !matches!(
                value["state"].as_str(),
                Some("requested" | "completed" | "interrupted")
            )
            || value["digest"].as_str().is_none_or(|v| {
                v.len() != 64
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            bail!("Invalid residency plan receipt")
        }
        if value["digest"] == plan.digest {
            bail!("Residency plan already attempted; review a fresh plan")
        }
    }
    drop(rows);
    drop(statement);
    crate::storage::admit(&tx)?;
    let task = uuid::Uuid::new_v4().to_string();
    tx.execute("INSERT INTO tasks VALUES (?1,'Operator reviewed residency plan','fast','running','',unixepoch(),unixepoch())",[&task])?;
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'residency_plan','human-cli',?2,unixepoch())",rusqlite::params![task,json!({"state":"requested","format_version":plan.format_version,"digest":plan.digest,"plan":plan,"automatic_retry":false,"permissions_granted":false}).to_string()])?;
    tx.commit()?;
    Ok(task)
}
fn finish(
    store: &Store,
    task: &str,
    plan: &Plan,
    state: &str,
    confirmed: &[Value],
) -> Result<Value> {
    let report = json!({"format_version":plan.format_version,"task_id":task,"digest":plan.digest,"state":state,"confirmed_steps":confirmed,"planned_operations":plan.actions.len(),"reused":plan.reused,"server_reported":true,"ownership_established":false,"weights_attested":false,"generation_tested":false,"automatic_retry":false,"automatic_cleanup":false,"physical_limits_enforced":false});
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if tx.execute(
        "UPDATE tasks SET status=?2,updated=unixepoch() WHERE id=?1 AND status='running'",
        rusqlite::params![
            task,
            if state == "completed" {
                "completed"
            } else {
                "interrupted"
            }
        ],
    )? != 1
    {
        bail!("Residency plan lost its task anchor")
    }
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'residency_plan','human-cli',?2,unixepoch())",rusqlite::params![task,json!({"format_version":plan.format_version,"state":state,"digest":plan.digest,"report":report}).to_string()])?;
    tx.commit()?;
    Ok(report)
}
/// Approval covers this exact plan once, with a fresh inventory check at each step.
pub async fn apply(
    store: &Store,
    base: &str,
    authenticate: bool,
    plan: &Plan,
    human_approve: bool,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve {
        bail!("Residency plan application requires explicit operator approval")
    }
    plan.validate()?;
    if endpoint(base)?.as_str() != plan.endpoint {
        bail!("Residency plan endpoint differs from explicit native endpoint")
    }
    let _leases = mutation_leases(store)?;
    let inventory = status(base, authenticate).await?;
    let models = parse(&serde_json::to_vec(&inventory)?)?;
    if build(base, &plan.request, &models, plan.created_unix)? != *plan {
        bail!("Residency inventory or proposed operations changed; review a fresh plan")
    }
    let task = claim(store, plan)?;
    let mut confirmed = Vec::new();
    let mut expected = plan.inventory_sha256.clone();
    let mut expected_kv = plan.inventory_kv_sha256.clone();
    let execution = async {
        for (step, action) in plan.actions.iter().enumerate() {
            let result = mutate_owned(
                store,
                base,
                authenticate,
                action.clone(),
                Some(MutationContext {
                    plan_version: plan.format_version,
                    inventory_sha256: &expected,
                    inventory_kv_sha256: expected_kv.as_deref(),
                    plan_task: &task,
                    step,
                }),
            )
            .await?;
            expected = result["after_inventory_sha256"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing confirmed inventory fingerprint"))?
                .into();
            if expected_kv.is_some() {
                expected_kv = Some(
                    result["after_inventory_kv_sha256"]
                        .as_str()
                        .ok_or_else(|| {
                            anyhow::anyhow!("Missing confirmed KV-cache placement fingerprint")
                        })?
                        .into(),
                );
            }
            confirmed.push(result);
        }
        if plan.format_version == 2 {
            let final_value = status(base, authenticate).await?;
            let final_models = parse(&serde_json::to_vec(&final_value)?)?;
            let final_kv = kv_fingerprint(&final_models)?;
            if expected_kv.as_deref() != Some(final_kv.as_str()) {
                bail!("Final native KV-cache placement differs from the confirmed plan state")
            }
            for ensure in &plan.request.ensure_loaded {
                let model = final_models
                    .iter()
                    .find(|model| model.key == ensure.model)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Planned model disappeared before completion")
                    })?;
                if !model.loaded_instances.iter().any(|instance| {
                    instance.config.context_length == u64::from(ensure.context_length)
                        && ensure.offload_kv_cache_to_gpu.map_or(true, |expected| {
                            instance.config.offload_kv_cache_to_gpu == Some(expected)
                        })
                }) {
                    bail!("Final native inventory does not satisfy the approved model placement")
                }
            }
            if plan.request.release_instances.iter().any(|id| {
                final_models.iter().any(|model| {
                    model
                        .loaded_instances
                        .iter()
                        .any(|instance| &instance.id == id)
                })
            }) {
                bail!("Explicitly released native instance remains present after the plan")
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let completed = matches!(
        tokio::time::timeout(Duration::from_secs(120), execution).await,
        Ok(Ok(()))
    );
    finish(
        store,
        &task,
        plan,
        if completed {
            "completed"
        } else {
            "interrupted"
        },
        &confirmed,
    )
}

#[cfg(test)]
mod tests;
