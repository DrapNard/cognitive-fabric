//! Operator-authorized task/model lifecycle. Model roles cannot acquire this authority.
use super::*;
use crate::{
    config::Config,
    engine::{Engine, Outcome},
    store::Store,
};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) mod lazy;
pub mod restoration;

/// Immutable operator-selected scope for ordinary tasks. This file describes
/// which local models may be loaded and whether newly loaded receipts may be
/// released afterward; it contains no task text or approval assertion.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLifecyclePolicy {
    pub format_version: u32,
    pub api_base: String,
    #[serde(default)]
    pub authenticate: bool,
    pub models: Vec<planning::EnsureLoaded>,
    #[serde(default)]
    pub release_after: bool,
    #[serde(default)]
    pub typed_module_worker: bool,
    pub max_lifecycle_seconds: u64,
}

impl NativeLifecyclePolicy {
    /// Construct a reviewable policy from configured model keys only. This
    /// performs no provider operation and does not confer operator approval.
    pub fn draft_for_config(
        cfg: &Config,
        api_base: String,
        extra_models: Vec<String>,
        context_length: u32,
        offload_kv_cache_to_gpu: Option<bool>,
        authenticate: bool,
        max_lifecycle_seconds: u64,
    ) -> Result<Self> {
        cfg.validate()?;
        if cfg.backend != "openai-local" || cfg.language_model().is_none() {
            bail!("A network policy draft requires local inference and an explicit language-only model route")
        }
        if extra_models.len() > 8 {
            bail!("Native policy draft allows at most eight additional model keys")
        }
        if url::Url::parse(&api_base)?.origin() != url::Url::parse(&cfg.api_base)?.origin() {
            bail!("Native policy endpoint must match the configured provider origin")
        }
        let models = std::iter::once(&cfg.model)
            .chain(cfg.role_profiles.values().map(|profile| &profile.model))
            .chain(extra_models.iter())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|model| planning::EnsureLoaded {
                model: model.clone(),
                context_length,
                offload_kv_cache_to_gpu,
            })
            .collect();
        let policy = Self {
            format_version: 3,
            api_base,
            authenticate,
            models,
            release_after: false,
            typed_module_worker: false,
            max_lifecycle_seconds,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Load a bounded owner-selected policy without following a symlink.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let bytes = security::read_regular_bytes(path.as_ref(), 65_536)?;
        let value = crate::strict_json::value(&bytes, 65_536)?;
        let policy: Self = serde_json::from_value(value)?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        if !matches!(self.format_version, 1..=3)
            || !(1..=1800).contains(&self.max_lifecycle_seconds)
            || self.models.is_empty()
            || self.models.len() > 8
        {
            bail!("Invalid native lifecycle policy version or bounds")
        }
        endpoint(&self.api_base)?;
        let mut keys = std::collections::BTreeSet::new();
        let has_gpu_choice = self
            .models
            .iter()
            .any(|model| model.offload_kv_cache_to_gpu.is_some());
        if (self.format_version == 1 && has_gpu_choice)
            || (self.format_version == 2 && !has_gpu_choice)
        {
            bail!("Native lifecycle policy version does not match its KV-cache placement settings")
        }
        for model in &self.models {
            if !identity(&model.model)
                || !(1..=32768).contains(&model.context_length)
                || !keys.insert(&model.model)
            {
                bail!("Invalid or duplicate native lifecycle model")
            }
        }
        Ok(())
    }

    pub fn on_demand(&self) -> bool {
        self.format_version == 3
    }
}

/// Private proof that one coordinator retains both workspace exclusion leases.
/// Every borrowed runtime/pool keeps this same guard alive; no second flock is
/// acquired against our own exclusive ownership.
pub(crate) struct Ownership {
    store: Arc<Store>,
    _leases: (
        crate::recovery::ExecutionLease,
        crate::recovery::ExecutionLease,
    ),
}
impl Ownership {
    pub(crate) fn acquire(store: Arc<Store>) -> Result<Arc<Self>> {
        let leases = mutation_leases(&store)?;
        Ok(Arc::new(Self {
            store,
            _leases: leases,
        }))
    }
    pub(crate) fn validate(&self, store: &Store) -> Result<()> {
        if !std::ptr::eq(self.store.as_ref(), store) {
            bail!("Managed lifecycle ownership belongs to another store")
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub format_version: u32,
    pub query: String,
    pub mode: String,
    pub api_base: String,
    #[serde(default)]
    pub authenticate: bool,
    pub models: Vec<planning::EnsureLoaded>,
    #[serde(default)]
    pub release_after: bool,
    #[serde(default)]
    pub typed_module_worker: bool,
    pub max_lifecycle_seconds: u64,
}

#[derive(Serialize)]
pub struct Report {
    pub lifecycle_task_id: String,
    pub outcome: Outcome,
    pub receipt_ids: Vec<String>,
    pub released_receipt_ids: Vec<String>,
    pub release_deferred: bool,
    pub provider_stop_proven: bool,
    pub ownership_established: bool,
}

#[derive(Clone)]
struct BoundBackend {
    inner: Arc<crate::backend::native::NativeBackend>,
    ownership: Arc<Ownership>,
    base: String,
    authenticate: bool,
    child_task: String,
    receipts: std::collections::BTreeMap<String, managed::Receipt>,
}
impl crate::backend::InferenceBackend for BoundBackend {
    fn input_token_admission(&self) -> Option<crate::token_admission::Settings> {
        crate::backend::InferenceBackend::input_token_admission(self.inner.as_ref())
    }
    fn has_uncertain_dispatch(&self) -> bool {
        self.inner.has_uncertain_dispatch()
    }
    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        self.inner.inference_target(role)
    }
    fn pin(&self) -> Result<Option<Arc<dyn crate::backend::InferenceBackend>>> {
        Ok(Some(Arc::new(self.clone())))
    }
    fn generate<'a>(
        &'a self,
        _role: &'a str,
        _prompt: &'a str,
        _max_tokens: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        Box::pin(async { bail!("Managed inference requires its original task identity") })
    }
    fn generate_for<'a>(
        &'a self,
        task: &'a str,
        role: &'a str,
        prompt: &'a str,
        max_tokens: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        Box::pin(async move {
            if task != self.child_task {
                bail!("Native model binding belongs to another task")
            }
            self.ownership.validate(&self.ownership.store)?;
            let expected = self
                .receipts
                .get(role)
                .ok_or_else(|| anyhow::anyhow!("Missing pinned role receipt"))?;
            let inventory = parse(&serde_json::to_vec(
                &status(&self.base, self.authenticate).await?,
            )?)?;
            let observed = managed::validate_task_binding(
                &self.ownership.store,
                &expected.receipt_id,
                &self.base,
                &inventory,
            )?;
            if serde_json::to_value(&observed)? != serde_json::to_value(expected)? {
                bail!("Native role receipt changed after task admission")
            }
            self.ownership.store.event(
                task,
                "native_instance_dispatch",
                "security-kernel",
                json!({
                    "format_version":1,"role":role,"receipt_id":expected.receipt_id,
                    "instance_id":expected.instance_id,"model_key":expected.model_key,
                    "receipt_sha256":crate::store::sha256(&serde_json::to_vec(expected)?),
                    "inventory_sha256":planning::fingerprint(&inventory)?,
                    "response_instance_check_required":true,"weights_attested":false,
                    "permissions_granted":false
                }),
            )?;
            self.inner
                .generate_for(task, role, prompt, max_tokens)
                .await
        })
    }
}

/// Pending generation history is independent of receipt state, so process death
/// or a failed quarantine write cannot turn unknown inference into release rights.
pub(crate) fn require_no_unknown_generation(store: &Store, endpoint: &str) -> Result<()> {
    let db = store.lock()?;
    check_generation_history(&db, endpoint, Instant::now() + Duration::from_secs(5))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn check_generation_history(
    db: &rusqlite::Connection,
    endpoint: &str,
    deadline: Instant,
) -> Result<()> {
    db.progress_handler(1000, Some(move || Instant::now() >= deadline));
    let result = (|| -> Result<()> {
        if Instant::now() >= deadline {
            bail!("Native generation history deadline exhausted")
        }
        let mut stmt = db.prepare("SELECT task_id,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE kind='native_task_lifecycle' AND source='human-cli' ORDER BY id LIMIT 4097")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > 4096 {
            bail!("Native task lifecycle history exceeds its validation bound")
        }
        let mut phases =
            std::collections::BTreeMap::<String, (String, String, String, bool)>::new();
        type RootIdentity = (String, String, String, String);
        let mut on_demand_root_unknown = std::collections::BTreeMap::<String, RootIdentity>::new();
        let mut on_demand_phases = std::collections::BTreeMap::<
            (String, String),
            (String, String, String, String, String, String, bool),
        >::new();
        for (task, raw) in rows {
            if Instant::now() >= deadline {
                bail!("Native generation history deadline exhausted")
            }
            let v = crate::strict_json::value(raw.as_bytes(), 4096)?;
            if v["permissions_granted"] != false || v["provider_stop_proven"] != false {
                bail!("Malformed native task lifecycle evidence")
            }
            let state = v["state"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing native lifecycle phase"))?;
            if v["format_version"] == 2 {
                if v["activation"] != "on_demand" {
                    bail!("Unsupported native lifecycle activation mode")
                }
                let base = v["endpoint"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing native lifecycle endpoint"))?
                    .to_owned();
                let child = v["child_task_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing native child task"))?
                    .to_owned();
                let policy = v["policy_sha256"]
                    .as_str()
                    .filter(|hash| valid_digest(hash))
                    .ok_or_else(|| anyhow::anyhow!("Missing native lifecycle policy digest"))?
                    .to_owned();
                uuid::Uuid::parse_str(&task)?;
                uuid::Uuid::parse_str(&child)?;
                let expected = [
                    "format_version",
                    "activation",
                    "state",
                    "endpoint",
                    "child_task_id",
                    "policy_sha256",
                    "model_bindings_sha256",
                    "model_key",
                    "receipt_id",
                    "call_id",
                    "permissions_granted",
                    "provider_stop_proven",
                ];
                let mut allowed = expected
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>();
                if matches!(
                    state,
                    "generation_operator_stopped" | "execution_operator_stopped"
                ) {
                    allowed.insert("provider_stop_acknowledged");
                    allowed.insert("original_outcome");
                }
                if v.as_object().is_none_or(|object| {
                    object
                        .keys()
                        .map(String::as_str)
                        .collect::<std::collections::BTreeSet<_>>()
                        != allowed
                }) {
                    bail!("Unsupported on-demand native lifecycle event shape")
                }
                let model_bindings = v["model_bindings_sha256"]
                    .as_str()
                    .filter(|hash| valid_digest(hash))
                    .ok_or_else(|| {
                        anyhow::anyhow!("Missing native lifecycle model binding digest")
                    })?;
                if matches!(
                    state,
                    "execution_started"
                        | "execution_finished"
                        | "outcome_unknown"
                        | "execution_operator_stopped"
                ) {
                    if !v["call_id"].is_null()
                        || !v["model_key"].is_null()
                        || !v["receipt_id"].is_null()
                    {
                        bail!("Native lifecycle root phase contains call-specific identity")
                    }
                    let root_identity = (
                        base.clone(),
                        child.clone(),
                        policy.clone(),
                        model_bindings.to_owned(),
                    );
                    if state == "outcome_unknown" {
                        on_demand_root_unknown.insert(task.clone(), root_identity.clone());
                    }
                    if state == "execution_operator_stopped"
                        && (v["provider_stop_acknowledged"] != true
                            || v["original_outcome"] != "unknown"
                            || on_demand_root_unknown.remove(&task).as_ref()
                                != Some(&root_identity))
                    {
                        bail!("Lazy root resolution lacks an unresolved root outcome and stop acknowledgement")
                    }
                    continue;
                }
                if !matches!(
                    state,
                    "generation_started"
                        | "generation_confirmed"
                        | "generation_not_dispatched"
                        | "generation_outcome_unknown"
                        | "generation_operator_stopped"
                ) {
                    bail!("Unsupported on-demand native lifecycle phase")
                }
                let call = v["call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing on-demand native call identity"))?
                    .to_owned();
                uuid::Uuid::parse_str(&call)?;
                let model = v["model_key"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing on-demand native model identity"))?
                    .to_owned();
                let receipt = v["receipt_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing on-demand native receipt identity"))?
                    .to_owned();
                uuid::Uuid::parse_str(&receipt)?;
                let key = (task.clone(), call);
                if state == "generation_started" {
                    if on_demand_phases
                        .insert(
                            key,
                            (
                                base,
                                child,
                                policy,
                                model,
                                receipt,
                                model_bindings.to_owned(),
                                false,
                            ),
                        )
                        .is_some()
                    {
                        bail!("Duplicate on-demand native generation admission")
                    }
                } else {
                    let prior = on_demand_phases.get_mut(&key).ok_or_else(|| {
                        anyhow::anyhow!("On-demand native generation completion lacks admission")
                    })?;
                    if prior.0 != base
                        || prior.1 != child
                        || prior.2 != policy
                        || prior.3 != model
                        || prior.4 != receipt
                        || prior.5 != model_bindings
                        || prior.6
                    {
                        bail!("On-demand native generation completion differs from admission")
                    }
                    if matches!(
                        state,
                        "generation_confirmed"
                            | "generation_not_dispatched"
                            | "generation_operator_stopped"
                    ) {
                        if state == "generation_operator_stopped"
                            && (v["provider_stop_acknowledged"] != true
                                || v["original_outcome"] != "unknown")
                        {
                            bail!("Lazy generation resolution lacks operator stop acknowledgement")
                        }
                        prior.6 = true;
                    }
                }
                continue;
            }
            if v["format_version"] != 1 {
                bail!("Malformed native task lifecycle evidence")
            }
            if matches!(state, "requested" | "finished" | "outcome_unknown") {
                continue;
            }
            if !matches!(
                state,
                "generation_started" | "generation_confirmed" | "generation_operator_stopped"
            ) {
                bail!("Unsupported native lifecycle phase")
            }
            if state == "generation_operator_stopped"
                && (v["provider_stop_acknowledged"] != true || v["original_outcome"] != "unknown")
            {
                bail!("Native generation resolution lacks operator acknowledgement")
            }
            let base = v["endpoint"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing native lifecycle endpoint"))?
                .to_owned();
            let child = v["child_task_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing native child task"))?
                .to_owned();
            let hash = v["bindings_sha256"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing native bindings identity"))?
                .to_owned();
            uuid::Uuid::parse_str(&task)?;
            uuid::Uuid::parse_str(&child)?;
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                bail!("Invalid native binding digest")
            }
            if state == "generation_started" {
                if phases.insert(task, (base, child, hash, false)).is_some() {
                    bail!("Duplicate native generation admission")
                }
            } else {
                let prior = phases.get_mut(&task).ok_or_else(|| {
                    anyhow::anyhow!("Native generation completion lacks admission")
                })?;
                if prior.0 != base || prior.1 != child || prior.2 != hash || prior.3 {
                    bail!("Native generation completion differs from admission")
                }
                prior.3 = true;
            }
        }
        if phases
            .values()
            .any(|(base, _, _, confirmed)| base == endpoint && !confirmed)
        {
            bail!("Unknown native task inference blocks lifecycle mutation; independently stop the provider and resolve the task")
        }
        if on_demand_root_unknown
            .values()
            .any(|(base, _, _, _)| base == endpoint)
        {
            bail!("Unknown on-demand native lifecycle outcome blocks mutation; independently stop the provider and resolve the task")
        }
        if on_demand_phases
            .values()
            .any(|(base, _, _, _, _, _, confirmed)| base == endpoint && !confirmed)
        {
            bail!("Unknown on-demand native inference blocks lifecycle mutation; independently stop the provider and resolve the task")
        }
        if Instant::now() >= deadline {
            bail!("Native generation history deadline exhausted")
        }
        Ok(())
    })();
    db.progress_handler(0, None::<fn() -> bool>);
    result
}

fn event(
    store: &Store,
    task: &str,
    state: &str,
    endpoint: &str,
    child: &str,
    digest: &str,
) -> Result<()> {
    store.event(
        task,
        "native_task_lifecycle",
        "human-cli",
        json!({
            "format_version":1,"state":state,"endpoint":endpoint,"child_task_id":child,
            "bindings_sha256":digest,"permissions_granted":false,"provider_stop_proven":false
        }),
    )
}
struct Interrupted {
    store: Arc<Store>,
    task: String,
    endpoint: String,
    child: String,
    on_demand: bool,
    armed: bool,
}
impl Drop for Interrupted {
    fn drop(&mut self) {
        if self.armed {
            if self.on_demand {
                // Once lazy execution starts, its coordinator-owned guard writes
                // a terminal event with the exact committed activation map.
            } else {
                let _ = event(
                    &self.store,
                    &self.task,
                    "outcome_unknown",
                    &self.endpoint,
                    &self.child,
                    "",
                );
            }
            if let Ok(db) = self.store.lock() {
                let _=db.execute("UPDATE tasks SET status='interrupted',answer='Native lifecycle interrupted; no automatic cleanup or replay',updated=unixepoch() WHERE id IN (?1,?2) AND status='running'",rusqlite::params![self.task,self.child]);
            }
        }
    }
}

async fn wait_for_cancel(mut receiver: tokio::sync::watch::Receiver<bool>) {
    if !*receiver.borrow_and_update() {
        let _ = receiver.changed().await;
    }
}

fn ensure_setup_active(
    deadline: Instant,
    cancel: &tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    if *cancel.borrow() {
        bail!("Native lifecycle cancelled during setup")
    }
    if Instant::now() >= deadline {
        bail!("Native lifecycle deadline exhausted during setup")
    }
    Ok(())
}

/// SQLite and validation stages are synchronous and cannot be preempted. Yield
/// between bounded stages so cancellation is observed before further admission.
async fn setup_checkpoint(
    deadline: Instant,
    cancel: &tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    ensure_setup_active(deadline, cancel)?;
    tokio::task::yield_now().await;
    ensure_setup_active(deadline, cancel)
}

struct LifecycleRuntime<'a> {
    human_approve: bool,
    verifier: Option<Arc<dyn crate::verification::VerifierProvider>>,
    template: Option<&'a Engine>,
    execution: crate::engine::TaskExecution,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
}

/// Execute a complete task using only its approved model set. Loads and releases
/// are fixed operator actions outside all model-controlled roles. No foreign
/// inventory instance can be adopted; reuse requires an existing active receipt.
pub async fn run(
    cfg: Config,
    store: Arc<Store>,
    request: Request,
    human_approve: bool,
) -> Result<Report> {
    run_with_verifier(cfg, store, request, human_approve, None).await
}

pub async fn run_with_verifier(
    cfg: Config,
    store: Arc<Store>,
    request: Request,
    human_approve: bool,
    verifier: Option<Arc<dyn crate::verification::VerifierProvider>>,
) -> Result<Report> {
    let start = crate::execution_clock::ExecutionStart::new(cfg.max_wall_seconds);
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    run_internal(
        cfg,
        store,
        request,
        LifecycleRuntime {
            human_approve,
            verifier,
            template: None,
            execution: crate::engine::TaskExecution {
                task_id: uuid::Uuid::new_v4().to_string(),
                options: crate::language::TaskOptions::default(),
                profile: crate::engine::ExecutionProfile::Full,
                execution_start: start,
                task_precreated: false,
            },
            cancel_rx,
        },
    )
    .await
}

/// Run an ordinary Engine task through the approved native lifecycle while
/// retaining its task identity, budgets, host policies, options and deadline.
pub(crate) async fn run_engine_task<F>(
    template: &Engine,
    policy: Arc<NativeLifecyclePolicy>,
    query: &str,
    mode: &str,
    execution: crate::engine::TaskExecution,
    cancel: F,
) -> Result<Outcome>
where
    F: std::future::Future<Output = ()>,
{
    let request = Request {
        format_version: policy.format_version,
        query: query.to_owned(),
        mode: mode.to_owned(),
        api_base: policy.api_base.clone(),
        authenticate: policy.authenticate,
        models: policy.models.clone(),
        release_after: policy.release_after,
        typed_module_worker: policy.typed_module_worker,
        max_lifecycle_seconds: policy.max_lifecycle_seconds,
    };
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let run = run_internal(
        template.cfg.clone(),
        template.store.clone(),
        request,
        LifecycleRuntime {
            human_approve: true,
            verifier: None,
            template: Some(template),
            execution,
            cancel_rx,
        },
    );
    tokio::pin!(run);
    tokio::pin!(cancel);
    tokio::select! {
        biased;
        _ = &mut cancel => {
            let _ = cancel_tx.send(true);
            run.await.map(|report| report.outcome)
        },
        result = &mut run => result.map(|report| report.outcome),
    }
}

async fn run_internal(
    cfg: Config,
    store: Arc<Store>,
    request: Request,
    runtime: LifecycleRuntime<'_>,
) -> Result<Report> {
    let LifecycleRuntime {
        human_approve,
        verifier,
        template,
        execution,
        cancel_rx,
    } = runtime;
    let crate::engine::TaskExecution {
        task_id: child,
        options,
        profile,
        execution_start,
        ..
    } = execution;
    let deadline = execution_start
        .instant
        .checked_add(Duration::from_secs(request.max_lifecycle_seconds))
        .map(|policy_deadline| execution_start.deadline.min(policy_deadline))
        .unwrap_or(execution_start.deadline);
    ensure_setup_active(deadline, &cancel_rx)?;
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve {
        bail!("Managed task execution requires explicit operator approval")
    }
    cfg.validate()?;
    if request.format_version != 3
        && cfg
            .resource_admission
            .as_ref()
            .is_some_and(|resources| resources.gpu_observation.is_some())
    {
        bail!("GPU observation admission requires on-demand native lifecycle policy version 3")
    }
    if cfg.input_token_admission.is_some() {
        bail!("Native model lifecycle does not support llama.cpp token-admission routing")
    }
    setup_checkpoint(deadline, &cancel_rx).await?;
    let has_gpu_choice = request
        .models
        .iter()
        .any(|model| model.offload_kv_cache_to_gpu.is_some());
    if !matches!(request.format_version, 1..=3)
        || (request.format_version == 1 && has_gpu_choice)
        || (request.format_version == 2 && !has_gpu_choice)
        || request.query.trim().is_empty()
        || request.query.len() > 12000
        || !matches!(
            request.mode.as_str(),
            "fast" | "standard" | "deep" | "research"
        )
        || request.models.is_empty()
        || request.models.len() > 8
        || !(1..=1800).contains(&request.max_lifecycle_seconds)
        || cfg.backend != "openai-local"
        || cfg.inference_process
    {
        bail!("Invalid native task request, budget or backend configuration")
    }
    let native_endpoint = endpoint(&request.api_base)?.to_string();
    let mut desired = std::collections::BTreeMap::<String, planning::EnsureLoaded>::new();
    for model in &request.models {
        if !identity(&model.model)
            || !(1..=32768).contains(&model.context_length)
            || desired.insert(model.model.clone(), model.clone()).is_some()
        {
            bail!("Invalid or duplicate approved native model")
        }
    }
    let mut required = std::collections::BTreeSet::new();
    required.insert(cfg.model.clone());
    required.extend(cfg.role_profiles.values().map(|p| p.model.clone()));
    if !required.iter().all(|model| desired.contains_key(model)) {
        bail!("Approved native models must include every configured default and role model")
    }
    if request.typed_module_worker {
        if let Some(engine) = template {
            if !engine.modules.discover()?.is_empty() {
                bail!(
                    "Automatic typed worker requires a template without preinstalled typed modules"
                )
            }
        }
    }
    setup_checkpoint(deadline, &cancel_rx).await?;
    // Model registration integrity and all local request shape checks precede
    // any provider mutation. The Engine independently pins it again at admission.
    store.pin_model(&cfg)?;
    setup_checkpoint(deadline, &cancel_rx).await?;
    let ownership = Ownership::acquire(store.clone())?;
    let task_modules = match template {
        Some(engine) => engine.modules.fork_owned(ownership.clone())?,
        None => Arc::new(crate::fabric::modules::Broker::new(store.clone())),
    };
    if request.typed_module_worker {
        task_modules.register(
            Arc::new(
                crate::fabric::modules::ProcessModule::builtin_with_ownership(
                    store.clone(),
                    Some(ownership.clone()),
                )?
                .with_process_limits(cfg.worker_process_limits)?,
            ),
            crate::fabric::modules::Limits::default(),
        )?;
    }
    setup_checkpoint(deadline, &cancel_rx).await?;
    require_no_unknown_generation(&store, &native_endpoint)?;
    // The bounded history scan is synchronous; cancellation cannot interrupt
    // SQLite mid-query, so gate again before task creation or provider work.
    setup_checkpoint(deadline, &cancel_rx).await?;
    let parent = store.create_task("Operator-approved native task lifecycle", "fast")?;
    if let Err(error) = setup_checkpoint(deadline, &cancel_rx).await {
        let _ = store.finish_task(
            &parent,
            "failed",
            "Native lifecycle stopped before task admission",
        );
        return Err(error);
    }
    if let Err(error) = store.create_named_task(&child, &request.query, &request.mode) {
        let _ = store.finish_task(
            &parent,
            "failed",
            "Could not create the requested task identity",
        );
        return Err(error);
    }
    let mut interruption = Interrupted {
        store: store.clone(),
        task: parent.clone(),
        endpoint: native_endpoint.clone(),
        child: child.clone(),
        on_demand: request.format_version == 3,
        armed: true,
    };
    setup_checkpoint(deadline, &cancel_rx).await?;
    let request_hash = crate::store::sha256(&serde_json::to_vec(&request)?);
    let policy_hash = crate::store::sha256(&serde_json::to_vec(&NativeLifecyclePolicy {
        format_version: request.format_version,
        api_base: request.api_base.clone(),
        authenticate: request.authenticate,
        models: request.models.clone(),
        release_after: request.release_after,
        typed_module_worker: request.typed_module_worker,
        max_lifecycle_seconds: request.max_lifecycle_seconds,
    })?);
    if request.format_version == 3 {
        let cutoff = lazy::LifecycleCutoff::from_deadline(deadline)?;
        let policy = Arc::new(NativeLifecyclePolicy {
            format_version: request.format_version,
            api_base: request.api_base.clone(),
            authenticate: request.authenticate,
            models: request.models.clone(),
            release_after: request.release_after,
            typed_module_worker: request.typed_module_worker,
            max_lifecycle_seconds: request.max_lifecycle_seconds,
        });
        let report = lazy::run_on_demand(
            store.clone(),
            cfg,
            template,
            verifier,
            options,
            profile,
            execution_start,
            child,
            parent,
            request,
            policy,
            policy_hash,
            request_hash,
            ownership,
            task_modules,
            cutoff,
            deadline,
            cancel_rx,
        )
        .await?;
        interruption.armed = false;
        return Ok(report);
    }
    event(
        &store,
        &parent,
        "requested",
        &native_endpoint,
        &child,
        &request_hash,
    )?;
    let future = async {
        ensure_setup_active(deadline, &cancel_rx)?;
        let local = tokio::select! {
            result=async { managed::list(&store,&request.api_base) } => result?,
            _=wait_for_cancel(cancel_rx.clone()) => bail!("Native lifecycle cancelled before provider inventory")
        };
        if local["managed_lifecycle_blocked"] != false {
            bail!("Managed task endpoint needs operator reconciliation")
        }
        // Validate every explicit GPU-placement target before loading any
        // member of a multi-model lifecycle. A later unsupported model must
        // not leave earlier models loaded by this request.
        if desired
            .values()
            .any(|ensure| ensure.offload_kv_cache_to_gpu.is_some())
        {
            let preflight = tokio::select! {
                result=async { status(&request.api_base,request.authenticate).await } => result?,
                _=wait_for_cancel(cancel_rx.clone()) => bail!("Native lifecycle cancelled before model preflight")
            };
            let preflight_models = parse(&serde_json::to_vec(&preflight)?)?;
            for ensure in desired
                .values()
                .filter(|ensure| ensure.offload_kv_cache_to_gpu.is_some())
            {
                let model = preflight_models
                    .iter()
                    .find(|model| model.key == ensure.model)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Explicit GPU-placement model is not installed")
                    })?;
                if !matches!(model.kind.as_str(), "llm" | "decision") {
                    bail!("Explicit KV-cache GPU placement is supported only for LLM and decision models")
                }
            }
        }
        let mut receipts = std::collections::BTreeMap::<String, managed::Receipt>::new();
        let mut newly_loaded = std::collections::BTreeSet::new();
        for (model, ensure) in &desired {
            ensure_setup_active(deadline, &cancel_rx)?;
            let existing = local["receipts"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid managed receipt listing"))?
                .iter()
                .filter(|v| {
                    v["state"] == "active"
                        && v["receipt"]["model_key"] == *model
                        && v["receipt"]["context_length"] == ensure.context_length
                        && ensure.offload_kv_cache_to_gpu.is_none_or(|expected| {
                            v["receipt"]["offload_kv_cache_to_gpu"] == expected
                        })
                })
                .collect::<Vec<_>>();
            if existing.len() > 1 {
                bail!("Ambiguous existing native task model receipt")
            }
            let receipt = if let Some(row) = existing.first() {
                serde_json::from_value::<managed::Receipt>(row["receipt"].clone())?
            } else {
                ensure_setup_active(deadline, &cancel_rx)?;
                let loaded = tokio::select! {
                    loaded=managed::load_owned_with_gpu(&store,&request.api_base,request.authenticate,model,ensure.context_length,ensure.offload_kv_cache_to_gpu,(true,Some(&ownership))) => loaded?,
                    _=wait_for_cancel(cancel_rx.clone()) => bail!("Native lifecycle cancelled during model load")
                };
                let receipt =
                    serde_json::from_value::<managed::Receipt>(loaded["receipt"].clone())?;
                newly_loaded.insert(receipt.receipt_id.clone());
                receipt
            };
            receipts.insert(model.clone(), receipt);
            ensure_setup_active(deadline, &cancel_rx)?;
        }
        ensure_setup_active(deadline, &cancel_rx)?;
        let observed = tokio::select! {
            result=status(&request.api_base,request.authenticate) => result?,
            _=wait_for_cancel(cancel_rx.clone()) => bail!("Native lifecycle cancelled during instance binding")
        };
        let inventory = parse(&serde_json::to_vec(&observed)?)?;
        for receipt in receipts.values() {
            managed::validate_task_binding(
                &store,
                &receipt.receipt_id,
                &request.api_base,
                &inventory,
            )?;
        }
        let bindings_hash = crate::store::sha256(&serde_json::to_vec(&receipts)?);
        let mut native_cfg = cfg.clone();
        native_cfg.api_base = request.api_base.clone();
        let mut role_receipts = std::collections::BTreeMap::new();
        let mut targets = std::collections::BTreeMap::new();
        for role in crate::fabric::protocol::ROLES {
            let (model, _) = cfg.model_settings(role)?;
            let receipt = receipts.get(model).ok_or_else(|| {
                anyhow::anyhow!("Configured role is absent from approved model set")
            })?;
            role_receipts.insert(role.to_owned(), receipt.clone());
            targets.insert(
                role.to_owned(),
                crate::backend::NativeModelTarget {
                    api_base: request.api_base.clone(),
                    model: model.to_owned(),
                    model_instance_id: receipt.instance_id.clone(),
                }
                .canonical()?,
            );
        }
        let native = Arc::new(crate::backend::native::NativeBackend::with_role_targets(
            native_cfg,
            targets,
            request.authenticate,
        )?);
        let backend = Arc::new(BoundBackend {
            inner: native.clone(),
            ownership: ownership.clone(),
            base: request.api_base.clone(),
            authenticate: request.authenticate,
            child_task: child.clone(),
            receipts: role_receipts,
        });
        let native_backends = Arc::new(std::sync::Mutex::new(vec![native.clone()]));
        let factory_store = store.clone();
        let factory_owner = ownership.clone();
        let factory_receipts = receipts.clone();
        let factory_base = request.api_base.clone();
        let factory_child = child.clone();
        let factory_authenticate = request.authenticate;
        let factory_backends = native_backends.clone();
        let factory: crate::engine::ModelBackendFactory = Arc::new(move |model_cfg| {
            let receipt = factory_receipts.get(&model_cfg.model).ok_or_else(|| {
                anyhow::anyhow!("Expert model was not in the operator-approved native task set")
            })?;
            let registered = url::Url::parse(&model_cfg.api_base)?;
            let native_origin = url::Url::parse(&factory_base)?;
            if registered.origin() != native_origin.origin() {
                bail!("Expert registration points outside the approved native provider")
            }
            factory_owner.validate(&factory_store)?;
            let mut config = model_cfg.clone();
            config.api_base = factory_base.clone();
            let adapter = Arc::new(crate::backend::native::NativeBackend::new(
                config,
                receipt.instance_id.clone(),
                factory_authenticate,
            )?);
            let roles = crate::fabric::protocol::ROLES
                .iter()
                .map(|role| ((*role).to_owned(), receipt.clone()))
                .collect();
            {
                let mut registered = factory_backends
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Native expert backend registry poisoned"))?;
                if registered.len() >= 9 {
                    bail!("Native expert adapter capacity exhausted")
                }
                registered.push(adapter.clone());
            }
            Ok(Arc::new(BoundBackend {
                inner: adapter,
                ownership: factory_owner.clone(),
                base: factory_base.clone(),
                authenticate: factory_authenticate,
                child_task: factory_child.clone(),
                receipts: roles,
            }))
        });
        let engine = if let Some(template) = template {
            template.with_owned_native_runtime(
                backend,
                ownership.clone(),
                factory,
                policy_hash.clone(),
                None,
                task_modules.clone(),
            )?
        } else {
            let engine = Engine::new(cfg.clone(), store.clone()).with_owned_native_runtime(
                backend,
                ownership.clone(),
                factory,
                policy_hash.clone(),
                None,
                task_modules.clone(),
            )?;
            match &verifier {
                Some(provider) => engine.with_verifier_provider(provider.clone())?,
                None => engine,
            }
        };
        // Admission validates original resource declarations before any model
        // call, using the exact native target rather than configured HTTP guesses.
        engine.configured_resource_pool()?;
        ensure_setup_active(deadline, &cancel_rx)?;
        store.event(&parent,"native_task_bindings","human-cli",json!({
            "format_version":1,"child_task_id":child,"bindings_sha256":bindings_hash,
            "receipts":receipts,"request_sha256":request_hash,"policy_sha256":policy_hash,"permissions_granted":false,
            "weights_attested":false,"response_instance_check_required":true
        }))?;
        event(
            &store,
            &parent,
            "generation_started",
            &native_endpoint,
            &child,
            &bindings_hash,
        )?;
        ensure_setup_active(deadline, &cancel_rx)?;
        let mut cancellation = cancel_rx.clone();
        let cancel_future = async move {
            if !*cancellation.borrow_and_update() {
                let _ = cancellation.changed().await;
            }
        };
        let outcome = engine
            .run_execution_body(
                &request.query,
                &request.mode,
                crate::engine::TaskExecution {
                    task_id: child.clone(),
                    options,
                    profile,
                    execution_start,
                    task_precreated: true,
                },
                cancel_future,
            )
            .await?;
        let uncertain = native_backends
            .lock()
            .map_err(|_| anyhow::anyhow!("Native expert backend registry poisoned"))?
            .iter()
            .any(|backend| backend.has_uncertain_dispatch());
        let cleanup_safe = !uncertain
            && !matches!(
                outcome.status.as_str(),
                "cancelled" | "exhausted" | "failed" | "interrupted"
            );
        if !uncertain {
            event(
                &store,
                &parent,
                "generation_confirmed",
                &native_endpoint,
                &child,
                &bindings_hash,
            )?;
        }
        let mut released = Vec::new();
        if request.release_after && cleanup_safe {
            // Reused receipts remain resident. Only instances loaded by this
            // approved lifecycle can be released by its automatic cleanup.
            for id in &newly_loaded {
                tokio::select! {
                    result=managed::release_owned(&store,&request.api_base,request.authenticate,id,(true,Some(&ownership))) => { result?; },
                    _=wait_for_cancel(cancel_rx.clone()) => bail!("Native lifecycle cancelled during receipt cleanup")
                }
                released.push(id.clone());
            }
        }
        let deferred =
            request.release_after && (!cleanup_safe || released.len() != newly_loaded.len());
        if uncertain {
            event(
                &store,
                &parent,
                "outcome_unknown",
                &native_endpoint,
                &child,
                &bindings_hash,
            )?;
        } else {
            event(
                &store,
                &parent,
                "finished",
                &native_endpoint,
                &child,
                &bindings_hash,
            )?;
        }
        store.finish_task(
            &parent,
            if uncertain {
                "interrupted"
            } else {
                "completed"
            },
            if uncertain {
                "Native inference remains unknown; automatic release deferred"
            } else {
                "Native task lifecycle finished; no provider-stop or weight attestation"
            },
        )?;
        Ok(Report {
            lifecycle_task_id: parent.clone(),
            outcome,
            receipt_ids: receipts.values().map(|r| r.receipt_id.clone()).collect(),
            released_receipt_ids: released,
            release_deferred: deferred,
            provider_stop_proven: false,
            ownership_established: false,
        })
    };
    let report = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), future)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Native task lifecycle deadline exhausted; no automatic cleanup is permitted"
            )
        })??;
    interruption.armed = false;
    Ok(report)
}

/// Acknowledge independently stopped unknown inference without replaying it.
/// Every original receipt must still match the complete fresh inventory. This
/// does not rebase drift, adopt foreign instances or assert provider termination.
pub async fn resolve_generation(
    store: Arc<Store>,
    base: &str,
    authenticate: bool,
    task: &str,
    human_approve: bool,
    provider_stop_acknowledged: bool,
) -> Result<Value> {
    security::authorize("human-cli", "model.residency.change")?;
    if !human_approve || !provider_stop_acknowledged {
        bail!("Native generation resolution requires approval and provider-stop acknowledgement")
    }
    uuid::Uuid::parse_str(task)?;
    let native_endpoint = endpoint(base)?.to_string();
    let _ownership = Ownership::acquire(store.clone())?;
    let lazy_binding = {
        let db = store.lock()?;
        let mut statement = db.prepare("SELECT CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_bindings' AND source='human-cli' ORDER BY id LIMIT 2")?;
        let rows = statement
            .query_map([task], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() == 1 {
            Some(crate::strict_json::value(rows[0].as_bytes(), 16384)?)
        } else {
            None
        }
    };
    if lazy_binding
        .as_ref()
        .is_some_and(|binding| binding["format_version"] == 2)
    {
        return resolve_on_demand_generation(
            store,
            base,
            authenticate,
            task,
            lazy_binding.expect("checked above"),
        )
        .await;
    }
    let (binding_raw, start_raw) = {
        let db = store.lock()?;
        let mut statement=db.prepare("SELECT kind,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND source='human-cli' AND (kind IN ('native_task_bindings','native_continuation_bindings') OR (kind='native_task_lifecycle' AND json_extract(payload,'$.state') IN ('generation_started','generation_confirmed','generation_operator_stopped'))) ORDER BY id LIMIT 4")?;
        let rows = statement
            .query_map([task], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() != 2
            || !matches!(
                rows[0].0.as_str(),
                "native_task_bindings" | "native_continuation_bindings"
            )
            || rows[1].0 != "native_task_lifecycle"
        {
            bail!("Native generation is missing its unique unresolved binding/admission pair")
        }
        (rows[0].1.clone(), rows[1].1.clone())
    };
    let bindings = crate::strict_json::value(binding_raw.as_bytes(), 16384)?;
    let start = crate::strict_json::value(start_raw.as_bytes(), 4096)?;
    let child = start["child_task_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing original native child task"))?;
    uuid::Uuid::parse_str(child)?;
    let receipts: std::collections::BTreeMap<String, managed::Receipt> =
        serde_json::from_value(bindings["receipts"].clone())?;
    let digest = crate::store::sha256(&serde_json::to_vec(&receipts)?);
    if receipts.is_empty()
        || receipts.len() > 8
        || start["format_version"] != 1
        || start["state"] != "generation_started"
        || start["endpoint"] != native_endpoint
        || start["bindings_sha256"] != digest
        || start["permissions_granted"] != false
        || start["provider_stop_proven"] != false
        || bindings["format_version"] != 1
        || bindings["bindings_sha256"] != digest
        || bindings["child_task_id"] != child
        || bindings["permissions_granted"] != false
    {
        bail!("Native generation binding/admission identity changed")
    }
    let inventory = parse(&serde_json::to_vec(&status(base, authenticate).await?)?)?;
    for (model, receipt) in &receipts {
        if model != &receipt.model_key {
            bail!("Native generation model mapping changed")
        }
        let observed =
            managed::validate_task_binding(&store, &receipt.receipt_id, base, &inventory)?;
        if serde_json::to_value(observed)? != serde_json::to_value(receipt)? {
            bail!("Native generation receipt changed")
        }
    }
    let payload = json!({"format_version":1,"state":"generation_operator_stopped","endpoint":native_endpoint,
        "child_task_id":child,"bindings_sha256":digest,"permissions_granted":false,
        "provider_stop_proven":false,"provider_stop_acknowledged":true,"original_outcome":"unknown",
        "inventory_sha256":planning::fingerprint(&inventory)?});
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind='native_task_lifecycle' AND json_extract(payload,'$.state') IN ('generation_confirmed','generation_operator_stopped'))",[task],|r|r.get(0))?;
    if exists {
        bail!("Native generation already has a completion; replay refused")
    }
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'native_task_lifecycle','human-cli',?2,unixepoch())",rusqlite::params![task,payload.to_string()])?;
    tx.execute("UPDATE tasks SET status='interrupted',updated=unixepoch() WHERE id IN (?1,?2) AND status='running'",rusqlite::params![task,child])?;
    tx.commit()?;
    Ok(
        json!({"lifecycle_task_id":task,"child_task_id":child,"original_outcome":"unknown","provider_stop_acknowledged":true,"provider_stop_proven":false,"automatic_replay":false,"model_mutation_performed":false,"permissions_granted":false}),
    )
}

#[derive(Clone)]
struct LazyCall {
    call_id: String,
    model: String,
    receipt_id: String,
    bindings_sha256: String,
    start_event_id: i64,
    unresolved: bool,
}

/// Resolve every unresolved on-demand call in one lifecycle segment. The
/// operator acknowledgement is endpoint-scoped, so resolving only a subset
/// would leave a misleading partially cleared task history.
async fn resolve_on_demand_generation(
    store: Arc<Store>,
    base: &str,
    authenticate: bool,
    task: &str,
    root_binding: Value,
) -> Result<Value> {
    let endpoint = endpoint(base)?.to_string();
    if root_binding["format_version"] != 2
        || root_binding["permissions_granted"] != false
        || root_binding["weights_attested"] != false
        || root_binding["response_instance_check_required"] != true
        || root_binding["bindings_sha256"] != crate::store::sha256(b"{}")
        || !root_binding["receipts"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        bail!("Lazy native lifecycle root binding is malformed")
    }
    let binding_keys = [
        "format_version",
        "child_task_id",
        "request_sha256",
        "policy_sha256",
        "receipts",
        "bindings_sha256",
        "lifecycle_boot_id",
        "lifecycle_deadline_ns",
        "permissions_granted",
        "weights_attested",
        "response_instance_check_required",
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    if root_binding.as_object().is_none_or(|object| {
        object
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>()
            != binding_keys
    }) || !root_binding["request_sha256"]
        .as_str()
        .is_some_and(valid_digest)
        || !root_binding["lifecycle_deadline_ns"]
            .as_u64()
            .is_some_and(|deadline| deadline > 0)
    {
        bail!("Lazy native lifecycle root binding has an unsupported shape")
    }
    let child = root_binding["child_task_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing original on-demand child task"))?
        .to_owned();
    uuid::Uuid::parse_str(&child)?;
    let policy_sha256 = root_binding["policy_sha256"]
        .as_str()
        .filter(|hash| valid_digest(hash))
        .ok_or_else(|| anyhow::anyhow!("Missing lazy native policy identity"))?
        .to_owned();
    let (scope, rows) = {
        let db = store.lock()?;
        let mut scope_stmt = db.prepare("SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_scope' AND source='security-kernel' ORDER BY id LIMIT 2")?;
        let scopes = scope_stmt
            .query_map([&child], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if scopes.len() != 1 {
            bail!("Lazy native lifecycle lacks one exact task scope")
        }
        let scope = crate::strict_json::value(scopes[0].as_bytes(), 4096)?;
        let mut stmt = db.prepare("SELECT id,kind,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE task_id=?1 AND source='human-cli' AND kind IN ('native_task_lifecycle','native_task_bindings') ORDER BY id LIMIT 258")?;
        let rows = stmt
            .query_map([task], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > 257 {
            bail!("Lazy native lifecycle history exceeds its resolution bound")
        }
        (scope, rows)
    };
    if scope["format_version"] != 2
        || scope["instance_binding_required"] != true
        || scope["activation"] != "on_demand"
        || scope["policy_sha256"] != policy_sha256
        || scope["permissions_granted"] != false
    {
        bail!("Lazy native scope differs from its original approved policy")
    }
    let mut bindings = BTreeMap::<String, managed::Receipt>::new();
    let mut calls = BTreeMap::<String, LazyCall>::new();
    let mut root_active = false;
    let mut root_started = false;
    let mut root_unknown = false;
    for (event_id, kind, raw) in rows {
        let payload = crate::strict_json::value(raw.as_bytes(), 4096)?;
        if kind == "native_task_bindings" {
            if payload["format_version"] != 2 || payload["child_task_id"] != child {
                bail!("Lazy native task binding differs from its original child")
            }
            continue;
        }
        if payload["format_version"] != 2
            || payload["activation"] != "on_demand"
            || payload["endpoint"] != endpoint
            || payload["child_task_id"] != child
            || payload["policy_sha256"] != policy_sha256
            || payload["permissions_granted"] != false
            || payload["provider_stop_proven"] != false
        {
            bail!("Lazy native lifecycle event differs from its original identity")
        }
        let state = payload["state"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing lazy lifecycle phase"))?;
        let lifecycle_keys = [
            "format_version",
            "activation",
            "state",
            "endpoint",
            "child_task_id",
            "policy_sha256",
            "model_bindings_sha256",
            "model_key",
            "receipt_id",
            "call_id",
            "permissions_granted",
            "provider_stop_proven",
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
        let mut allowed_keys = lifecycle_keys.clone();
        if state == "generation_operator_stopped" {
            allowed_keys.insert("provider_stop_acknowledged");
            allowed_keys.insert("original_outcome");
        }
        if payload.as_object().is_none_or(|object| {
            object
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                != allowed_keys
        }) || !payload["model_bindings_sha256"]
            .as_str()
            .is_some_and(valid_digest)
        {
            bail!("Lazy lifecycle event has an unsupported shape")
        }
        match state {
            "execution_started" => {
                if root_active
                    || !payload["model_key"].is_null()
                    || !payload["receipt_id"].is_null()
                    || !payload["call_id"].is_null()
                {
                    bail!("Lazy lifecycle has an ambiguous execution start")
                }
                root_active = true;
                root_started = true;
            }
            "execution_finished" | "outcome_unknown" => {
                if !root_active
                    || !payload["model_key"].is_null()
                    || !payload["receipt_id"].is_null()
                    || !payload["call_id"].is_null()
                {
                    bail!("Lazy lifecycle has an ambiguous execution terminal")
                }
                root_active = false;
                root_unknown = state == "outcome_unknown";
            }
            "execution_operator_stopped" => {
                if root_active
                    || !root_unknown
                    || !payload["model_key"].is_null()
                    || !payload["receipt_id"].is_null()
                    || !payload["call_id"].is_null()
                    || payload["provider_stop_acknowledged"] != true
                    || payload["original_outcome"] != "unknown"
                {
                    bail!(
                        "Lazy root resolution lacks an unresolved outcome and stop acknowledgement"
                    )
                }
                root_unknown = false;
            }
            "generation_started" => {
                if !root_active {
                    bail!("Lazy generation call is outside an execution segment")
                }
                let call_id = payload["call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing lazy call ID"))?
                    .to_owned();
                uuid::Uuid::parse_str(&call_id)?;
                let model = payload["model_key"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing lazy model key"))?
                    .to_owned();
                let receipt_id = payload["receipt_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing lazy receipt ID"))?
                    .to_owned();
                uuid::Uuid::parse_str(&receipt_id)?;
                let digest = payload["model_bindings_sha256"]
                    .as_str()
                    .filter(|hash| valid_digest(hash))
                    .ok_or_else(|| anyhow::anyhow!("Missing lazy model binding digest"))?
                    .to_owned();
                if calls
                    .insert(
                        call_id.clone(),
                        LazyCall {
                            call_id,
                            model,
                            receipt_id,
                            bindings_sha256: digest,
                            start_event_id: event_id,
                            unresolved: true,
                        },
                    )
                    .is_some()
                {
                    bail!("Duplicate lazy generation call")
                }
            }
            "generation_confirmed"
            | "generation_not_dispatched"
            | "generation_operator_stopped"
            | "generation_outcome_unknown" => {
                if !root_active {
                    bail!("Lazy generation completion is outside an execution segment")
                }
                let call_id = payload["call_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing lazy call ID"))?;
                let call = calls
                    .get_mut(call_id)
                    .ok_or_else(|| anyhow::anyhow!("Lazy generation completion lacks admission"))?;
                if payload["model_key"] != call.model
                    || payload["receipt_id"] != call.receipt_id
                    || payload["model_bindings_sha256"] != call.bindings_sha256
                    || !call.unresolved
                {
                    bail!("Lazy generation completion differs from its admission")
                }
                if state == "generation_operator_stopped"
                    && (payload["provider_stop_acknowledged"] != true
                        || payload["original_outcome"] != "unknown")
                {
                    bail!("Lazy generation resolution lacks operator stop acknowledgement")
                }
                if state == "generation_confirmed"
                    || state == "generation_not_dispatched"
                    || state == "generation_operator_stopped"
                {
                    call.unresolved = false;
                }
            }
            _ => bail!("Unsupported lazy native lifecycle phase"),
        }
    }
    let unresolved = calls
        .values()
        .filter(|call| call.unresolved)
        .cloned()
        .collect::<Vec<_>>();
    if !root_started || unresolved.is_empty() {
        bail!("Lazy native lifecycle has no unresolved generation to resolve")
    }
    if !root_unknown {
        bail!("Lazy native root outcome is not awaiting operator resolution")
    }
    let activation_rows = {
        let db = store.lock()?;
        let mut stmt = db.prepare("SELECT id,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND kind='native_task_model_binding' AND source='security-kernel' ORDER BY id LIMIT 9")?;
        let rows = stmt
            .query_map([task], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if activation_rows.len() > 8 {
        bail!("Lazy native activated-model history exceeds policy bounds")
    }
    let mut bindings_before =
        BTreeMap::<i64, (BTreeMap<String, managed::Receipt>, BTreeMap<String, String>)>::new();
    let mut origins = BTreeMap::<String, String>::new();
    for (id, raw) in activation_rows {
        let payload = crate::strict_json::value(raw.as_bytes(), 16384)?;
        let activation_keys = [
            "format_version",
            "policy_sha256",
            "child_task_id",
            "model_key",
            "receipt",
            "receipt_sha256",
            "inventory_sha256",
            "origin",
            "role",
            "prompt_sha256",
            "permissions_granted",
            "weights_attested",
            "response_instance_check_required",
            "activation_binding_sha256",
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
        if payload.as_object().is_none_or(|object| {
            object
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                != activation_keys
        }) {
            bail!("Lazy native activation event has an unsupported shape")
        }
        if payload["format_version"] != 1
            || payload["policy_sha256"] != policy_sha256
            || payload["child_task_id"] != child
            || payload["permissions_granted"] != false
            || payload["weights_attested"] != false
            || payload["response_instance_check_required"] != true
            || !payload["inventory_sha256"]
                .as_str()
                .is_some_and(valid_digest)
            || !payload["prompt_sha256"].as_str().is_some_and(valid_digest)
            || payload["role"]
                .as_str()
                .is_none_or(|role| crate::fabric::protocol::actor(role).is_err())
        {
            bail!("Lazy native activation receipt record is malformed")
        }
        let model = payload["model_key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing activated model key"))?
            .to_owned();
        let receipt: managed::Receipt = serde_json::from_value(payload["receipt"].clone())?;
        let mut activation_commitment = payload.clone();
        let expected_commitment = activation_commitment["activation_binding_sha256"]
            .as_str()
            .filter(|hash| valid_digest(hash))
            .ok_or_else(|| anyhow::anyhow!("Missing activation receipt commitment"))?
            .to_owned();
        activation_commitment
            .as_object_mut()
            .expect("event payload is object")
            .remove("activation_binding_sha256");
        if receipt.model_key != model
            || payload["receipt_sha256"] != crate::store::sha256(&serde_json::to_vec(&receipt)?)
            || expected_commitment
                != crate::store::sha256(&serde_json::to_vec(&activation_commitment)?)
            || !matches!(payload["origin"].as_str(), Some("newly_loaded" | "reused"))
        {
            bail!("Lazy native activation receipt identity changed")
        }
        if bindings.insert(model.clone(), receipt).is_some() {
            bail!("Lazy native model has duplicate activation records")
        }
        origins.insert(model, payload["origin"].as_str().unwrap().to_owned());
        bindings_before.insert(id, (bindings.clone(), origins.clone()));
    }
    let inventory = parse(&serde_json::to_vec(&status(base, authenticate).await?)?)?;
    for call in &unresolved {
        let (receipt_map, origin_map) = bindings_before
            .range(..call.start_event_id)
            .next_back()
            .map(|(_, maps)| maps)
            .ok_or_else(|| {
                anyhow::anyhow!("Unresolved generation has no prior activated receipt map")
            })?;
        if lazy::activation_bindings_sha256(receipt_map, origin_map)? != call.bindings_sha256
            || receipt_map
                .get(&call.model)
                .is_none_or(|receipt| receipt.receipt_id != call.receipt_id)
        {
            bail!("Unresolved generation does not match its exact activated model bindings")
        }
        for receipt in receipt_map.values() {
            let observed =
                managed::validate_task_binding(&store, &receipt.receipt_id, base, &inventory)?;
            if serde_json::to_value(observed)? != serde_json::to_value(receipt)? {
                bail!("Lazy native receipt changed before operator stop resolution")
            }
        }
    }
    let mut db = store.lock()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for call in &unresolved {
        let payload = json!({
            "format_version":2,"activation":"on_demand","state":"generation_operator_stopped",
            "endpoint":endpoint,"child_task_id":child,"policy_sha256":policy_sha256,
            "model_bindings_sha256":call.bindings_sha256,"model_key":call.model,
            "receipt_id":call.receipt_id,"call_id":call.call_id,"permissions_granted":false,
            "provider_stop_proven":false,"provider_stop_acknowledged":true,"original_outcome":"unknown"
        });
        tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'native_task_lifecycle','human-cli',?2,unixepoch())", rusqlite::params![task, payload.to_string()])?;
    }
    let current_digest = lazy::activation_bindings_sha256(&bindings, &origins)?;
    let root_payload = json!({
        "format_version":2,"activation":"on_demand","state":"execution_operator_stopped",
        "endpoint":endpoint,"child_task_id":child,"policy_sha256":policy_sha256,
        "model_bindings_sha256":current_digest,"model_key":null,"receipt_id":null,"call_id":null,
        "permissions_granted":false,"provider_stop_proven":false,
        "provider_stop_acknowledged":true,"original_outcome":"unknown"
    });
    tx.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'native_task_lifecycle','human-cli',?2,unixepoch())", rusqlite::params![task, root_payload.to_string()])?;
    tx.execute("UPDATE tasks SET status='interrupted',updated=unixepoch() WHERE id IN (?1,?2) AND status='running'", rusqlite::params![task,child])?;
    tx.commit()?;
    Ok(
        json!({"lifecycle_task_id":task,"child_task_id":child,"resolved_calls":unresolved.len(),
        "original_outcome":"unknown","provider_stop_acknowledged":true,"provider_stop_proven":false,
        "automatic_replay":false,"model_mutation_performed":false,"permissions_granted":false}),
    )
}

#[cfg(test)]
mod generation_history_tests {
    use super::*;

    #[test]
    fn unknown_activation_root_blocks_mutation_until_explicit_stop_resolution() {
        let store = Store::in_memory().unwrap();
        let parent = store
            .create_task("Unknown native activation", "fast")
            .unwrap();
        let base = "http://127.0.0.1:1234/api/v1/models";
        let mut payload = json!({
            "format_version":2,"activation":"on_demand","state":"outcome_unknown",
            "endpoint":base,"child_task_id":uuid::Uuid::new_v4().to_string(),
            "policy_sha256":"a".repeat(64),"model_bindings_sha256":"b".repeat(64),
            "model_key":null,"receipt_id":null,"call_id":null,
            "permissions_granted":false,"provider_stop_proven":false
        });
        store
            .event(
                &parent,
                "native_task_lifecycle",
                "human-cli",
                payload.clone(),
            )
            .unwrap();
        assert!(require_no_unknown_generation(&store, base).is_err());
        assert!(
            require_no_unknown_generation(&store, "http://127.0.0.1:4321/api/v1/models").is_ok()
        );
        payload["state"] = json!("execution_operator_stopped");
        payload["provider_stop_acknowledged"] = json!(true);
        payload["original_outcome"] = json!("unknown");
        store
            .event(&parent, "native_task_lifecycle", "human-cli", payload)
            .unwrap();
        require_no_unknown_generation(&store, base).unwrap();
    }

    #[test]
    fn expired_history_deadline_refuses_and_clears_sqlite_interrupt_handler() {
        let store = Store::in_memory().unwrap();
        let db = store.lock().unwrap();
        assert!(check_generation_history(
            &db,
            "http://127.0.0.1:1234/api/v1/models",
            Instant::now()
        )
        .is_err());
        // A leaked expired progress handler would interrupt this longer query.
        let sum:i64=db.query_row("WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<10000) SELECT sum(n) FROM numbers",[],|row|row.get(0)).unwrap();
        assert_eq!(sum, 50_005_000);
        check_generation_history(
            &db,
            "http://127.0.0.1:1234/api/v1/models",
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
    }

    #[test]
    fn incomplete_or_malformed_generation_history_never_becomes_clearance() {
        let store = Store::in_memory().unwrap();
        let parent = store
            .create_task("Native generation history", "fast")
            .unwrap();
        let child = uuid::Uuid::new_v4().to_string();
        let base = "http://127.0.0.1:1234/api/v1/models";
        event(
            &store,
            &parent,
            "generation_started",
            base,
            &child,
            &"a".repeat(64),
        )
        .unwrap();
        assert!(require_no_unknown_generation(&store, base).is_err());
        assert!(
            require_no_unknown_generation(&store, "http://127.0.0.1:4321/api/v1/models").is_ok()
        );
        event(
            &store,
            &parent,
            "generation_confirmed",
            base,
            &child,
            &"a".repeat(64),
        )
        .unwrap();
        require_no_unknown_generation(&store, base).unwrap();
        event(
            &store,
            &parent,
            "generation_confirmed",
            base,
            &child,
            &"a".repeat(64),
        )
        .unwrap();
        assert!(require_no_unknown_generation(&store, base).is_err());
        let db = store.lock().unwrap();
        let count: i64 = db
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }
}
