//! Operator-authorized task/model lifecycle. Model roles cannot acquire this authority.
use super::*;
use crate::{
    config::Config,
    engine::{Engine, Outcome},
    store::Store,
};
use std::sync::Arc;

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
    /// Load a bounded owner-selected policy without following a symlink.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        use std::{io::Read, os::unix::fs::OpenOptionsExt};
        let path = path.as_ref();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > 65_536 {
            bail!("Native lifecycle policy must be a regular file no larger than 64 KiB")
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(65_537).read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            bail!("Native lifecycle policy exceeds 64 KiB")
        }
        let value = crate::strict_json::value(&bytes, 65_536)?;
        let policy: Self = serde_json::from_value(value)?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1
            || !(1..=1800).contains(&self.max_lifecycle_seconds)
            || self.models.is_empty()
            || self.models.len() > 8
        {
            bail!("Invalid native lifecycle policy version or bounds")
        }
        endpoint(&self.api_base)?;
        let mut keys = std::collections::BTreeSet::new();
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
    let mut stmt = db.prepare("SELECT task_id,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE kind='native_task_lifecycle' AND source='human-cli' ORDER BY id LIMIT 4097")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 4096 {
        bail!("Native task lifecycle history exceeds its validation bound")
    }
    let mut phases = std::collections::BTreeMap::<String, (String, String, String, bool)>::new();
    for (task, raw) in rows {
        let v = crate::strict_json::value(raw.as_bytes(), 4096)?;
        if v["format_version"] != 1
            || v["permissions_granted"] != false
            || v["provider_stop_proven"] != false
        {
            bail!("Malformed native task lifecycle evidence")
        }
        let state = v["state"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing native lifecycle phase"))?;
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
            let prior = phases
                .get_mut(&task)
                .ok_or_else(|| anyhow::anyhow!("Native generation completion lacks admission"))?;
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
    Ok(())
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
    armed: bool,
}
impl Drop for Interrupted {
    fn drop(&mut self) {
        if self.armed {
            let _ = event(
                &self.store,
                &self.task,
                "outcome_unknown",
                &self.endpoint,
                &self.child,
                "",
            );
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
        human_approve,
        verifier,
        None,
        None,
        crate::language::TaskOptions::default(),
        crate::engine::ExecutionProfile::Full,
        start,
        cancel_rx,
    )
    .await
}

/// Run an ordinary Engine task through the approved native lifecycle while
/// retaining its task identity, budgets, host policies, options and deadline.
pub(crate) async fn run_engine_task<F>(
    template: &Engine,
    policy: Arc<NativeLifecyclePolicy>,
    task: &str,
    query: &str,
    mode: &str,
    options: crate::language::TaskOptions,
    profile: crate::engine::ExecutionProfile,
    execution_start: crate::execution_clock::ExecutionStart,
    cancel: F,
) -> Result<Outcome>
where
    F: std::future::Future<Output = ()>,
{
    let request = Request {
        format_version: 1,
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
        true,
        None,
        Some(template),
        Some(task.to_owned()),
        options,
        profile,
        execution_start,
        cancel_rx,
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
    human_approve: bool,
    verifier: Option<Arc<dyn crate::verification::VerifierProvider>>,
    template: Option<&Engine>,
    requested_child: Option<String>,
    options: crate::language::TaskOptions,
    profile: crate::engine::ExecutionProfile,
    execution_start: crate::execution_clock::ExecutionStart,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<Report> {
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
    setup_checkpoint(deadline, &cancel_rx).await?;
    if request.format_version != 1
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
    let mut desired = std::collections::BTreeMap::new();
    for model in &request.models {
        if !identity(&model.model)
            || !(1..=32768).contains(&model.context_length)
            || desired
                .insert(model.model.clone(), model.context_length)
                .is_some()
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
    setup_checkpoint(deadline, &cancel_rx).await?;
    // Model registration integrity and all local request shape checks precede
    // any provider mutation. The Engine independently pins it again at admission.
    store.pin_model(&cfg)?;
    setup_checkpoint(deadline, &cancel_rx).await?;
    let ownership = Ownership::acquire(store.clone())?;
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
    let child = requested_child.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
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
        let mut receipts = std::collections::BTreeMap::<String, managed::Receipt>::new();
        let mut newly_loaded = std::collections::BTreeSet::new();
        for (model, context) in &desired {
            ensure_setup_active(deadline, &cancel_rx)?;
            let existing = local["receipts"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid managed receipt listing"))?
                .iter()
                .filter(|v| {
                    v["state"] == "active"
                        && v["receipt"]["model_key"] == *model
                        && v["receipt"]["context_length"] == *context
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
                    loaded=managed::load_owned(&store,&request.api_base,request.authenticate,model,*context,(true,Some(&ownership))) => loaded?,
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
            let model = cfg
                .role_profiles
                .get(role)
                .map(|p| p.model.as_str())
                .unwrap_or(&cfg.model);
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
            )?
        } else {
            let engine = Engine::with_backend(cfg.clone(), store.clone(), backend)
                .with_managed_ownership(ownership.clone())?
                .with_model_backend_factory(factory);
            match &verifier {
                Some(provider) => engine.with_verifier_provider(provider.clone())?,
                None => engine,
            }
        };
        let engine = if request.typed_module_worker {
            engine.with_builtin_module_worker()?
        } else {
            engine
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
                options,
                child.clone(),
                profile,
                execution_start,
                true,
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
    let (binding_raw, start_raw) = {
        let db = store.lock()?;
        let mut statement=db.prepare("SELECT kind,CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM events WHERE task_id=?1 AND source='human-cli' AND (kind='native_task_bindings' OR (kind='native_task_lifecycle' AND json_extract(payload,'$.state') IN ('generation_started','generation_confirmed','generation_operator_stopped'))) ORDER BY id LIMIT 4")?;
        let rows = statement
            .query_map([task], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() != 2
            || rows[0].0 != "native_task_bindings"
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
