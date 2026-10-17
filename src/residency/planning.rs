//! Bounded operator-reviewed residency reconciliation; no model actor can apply it.
use super::*;
use crate::store::{sha256, Store};
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnsureLoaded {
    pub model: String,
    pub context_length: u32,
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
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub format_version: u32,
    pub adapter: String,
    pub endpoint: String,
    pub created_unix: u64,
    pub inventory_sha256: String,
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
fn canonical_inventory(models: &[Model]) -> Vec<Model> {
    let mut models = models.to_vec();
    for model in &mut models {
        model.loaded_instances.sort_by(|a, b| a.id.cmp(&b.id));
    }
    models.sort_by(|a, b| a.key.cmp(&b.key));
    models
}
pub(super) fn fingerprint(models: &[Model]) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&canonical_inventory(models))?))
}
pub(super) fn validate_transition(
    before: &[Model],
    after: &[Model],
    action: &Mutation,
    instance: &str,
) -> Result<()> {
    let mut expected = canonical_inventory(before);
    match action {
        Mutation::Unload { instance_id } => {
            for model in &mut expected {
                model.loaded_instances.retain(|i| &i.id != instance_id)
            }
        }
        Mutation::Load {
            model,
            context_length,
        } => {
            expected
                .iter_mut()
                .find(|m| &m.key == model)
                .ok_or_else(|| anyhow::anyhow!("Missing installed planned model"))?
                .loaded_instances
                .push(Instance {
                    id: instance.into(),
                    config: InstanceConfig {
                        context_length: u64::from(*context_length),
                    },
                });
        }
    }
    if fingerprint(&expected)? != fingerprint(after)? {
        bail!("Unrelated native inventory changed during planned mutation")
    }
    Ok(())
}
impl Plan {
    fn checksum(&self) -> Result<String> {
        Ok(sha256(&serde_json::to_vec(&(
            self.format_version,
            &self.adapter,
            &self.endpoint,
            self.created_unix,
            &self.inventory_sha256,
            &self.request,
            &self.reused,
            &self.actions,
        ))?))
    }
    pub fn validate(&self) -> Result<()> {
        let observed = now()?;
        if self.format_version != 1
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
        }
        if self.digest != self.checksum()? {
            bail!("Residency plan content changed")
        }
        Ok(())
    }
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
            .filter(|i| i.config.context_length == u64::from(ensure.context_length))
            .min_by(|a, b| a.id.cmp(&b.id))
        {
            reused.push(Reuse {
                model: ensure.model.clone(),
                context_length: ensure.context_length,
                instance_id: instance.id.clone(),
            });
        } else {
            if !remaining.is_empty() {
                bail!("Incompatible loaded contexts require explicit release before replacement")
            }
            actions.push(Mutation::Load {
                model: ensure.model.clone(),
                context_length: ensure.context_length,
            });
        }
    }
    if actions.len() > 8 {
        bail!("Residency plan exceeds eight external operations")
    }
    let mut plan = Plan {
        format_version: 1,
        adapter: "lmstudio-native-v1".into(),
        endpoint: endpoint(base)?.to_string(),
        created_unix,
        inventory_sha256: fingerprint(models)?,
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
            || value["format_version"] != 1
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
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'residency_plan','human-cli',?2,unixepoch())",rusqlite::params![task,json!({"state":"requested","format_version":1,"digest":plan.digest,"plan":plan,"automatic_retry":false,"permissions_granted":false}).to_string()])?;
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
    let report = json!({"format_version":1,"task_id":task,"digest":plan.digest,"state":state,"confirmed_steps":confirmed,"planned_operations":plan.actions.len(),"reused":plan.reused,"server_reported":true,"ownership_established":false,"weights_attested":false,"generation_tested":false,"automatic_retry":false,"automatic_cleanup":false,"physical_limits_enforced":false});
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
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'residency_plan','human-cli',?2,unixepoch())",rusqlite::params![task,json!({"format_version":1,"state":state,"digest":plan.digest,"report":report}).to_string()])?;
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
    let execution = async {
        for (step, action) in plan.actions.iter().enumerate() {
            let result = mutate_owned(
                store,
                base,
                authenticate,
                action.clone(),
                Some(MutationContext {
                    inventory_sha256: &expected,
                    plan_task: &task,
                    step,
                }),
            )
            .await?;
            expected = result["after_inventory_sha256"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing confirmed inventory fingerprint"))?
                .into();
            confirmed.push(result);
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
