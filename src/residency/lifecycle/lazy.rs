//! First-use activation for an explicitly approved native model set.
//!
//! The coordinator owns the operator's fixed model scope and serializes loads
//! per model. A model is never represented as instance-pinned until a managed
//! load/reuse receipt has been validated and recorded.
use super::*;
use crate::{
    backend::{InferenceBackend, ManagedNativeTarget},
    config::Config,
    store::Store,
};
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Serialize)]
struct ActivationDigestEntry<'a> {
    receipt: &'a managed::Receipt,
    origin: &'a str,
}

pub(crate) fn activation_bindings_sha256(
    receipts: &BTreeMap<String, managed::Receipt>,
    origins: &BTreeMap<String, String>,
) -> Result<String> {
    if receipts.len() != origins.len() || receipts.keys().any(|model| !origins.contains_key(model))
    {
        bail!("Native activation receipts and origins differ")
    }
    let entries = receipts
        .iter()
        .map(|(model, receipt)| {
            (
                model.clone(),
                ActivationDigestEntry {
                    receipt,
                    origin: origins[model].as_str(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(crate::store::sha256(&serde_json::to_vec(&entries)?))
}

#[derive(Clone, Debug)]
pub(crate) struct LifecycleCutoff {
    pub boot_id: String,
    pub deadline_ns: u64,
}

impl LifecycleCutoff {
    pub(crate) fn from_deadline(deadline: Instant) -> Result<Self> {
        let (boot_id, now_ns) = crate::execution_clock::observation().ok_or_else(|| {
            anyhow::anyhow!("On-demand native activation requires a same-boot clock")
        })?;
        let remaining_ns = deadline
            .saturating_duration_since(Instant::now())
            .as_nanos();
        let remaining_ns = u64::try_from(remaining_ns)
            .map_err(|_| anyhow::anyhow!("Native lifecycle deadline exceeds clock bounds"))?;
        let deadline_ns = now_ns
            .checked_add(remaining_ns)
            .ok_or_else(|| anyhow::anyhow!("Native lifecycle deadline overflows clock bounds"))?;
        Ok(Self {
            boot_id,
            deadline_ns,
        })
    }

    pub(crate) fn remaining(&self) -> Result<Duration> {
        let (boot_id, now_ns) = crate::execution_clock::observation()
            .ok_or_else(|| anyhow::anyhow!("Native lifecycle clock is unavailable"))?;
        if boot_id != self.boot_id || now_ns >= self.deadline_ns {
            bail!("Original native lifecycle deadline expired or belongs to another boot")
        }
        Ok(Duration::from_nanos(self.deadline_ns - now_ns))
    }

    pub(crate) fn instant(&self) -> Result<Instant> {
        Instant::now()
            .checked_add(self.remaining()?)
            .ok_or_else(|| anyhow::anyhow!("Native lifecycle deadline is out of range"))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BoundReceipt {
    pub receipt: managed::Receipt,
    pub origin: &'static str,
}

#[derive(Clone, Debug)]
enum Activation {
    Unloaded,
    Loading,
    Unknown,
    Loaded(Box<BoundReceipt>),
}

struct ModelCell {
    state: tokio::sync::Mutex<Activation>,
}

/// One shared authorization and activation state for the main adapter and all
/// model-backed experts created from its factory.
pub(crate) struct Coordinator {
    store: Arc<Store>,
    owner: Arc<Ownership>,
    lifecycle_task: String,
    child_task: String,
    endpoint: String,
    api_base: String,
    authenticate: bool,
    policy_sha256: String,
    cutoff: LifecycleCutoff,
    deadline: Instant,
    cancel: tokio::sync::watch::Receiver<bool>,
    models: BTreeMap<String, planning::EnsureLoaded>,
    cells: Mutex<BTreeMap<String, Arc<ModelCell>>>,
    initial: BTreeMap<String, BoundReceipt>,
    committed: Mutex<BTreeMap<String, BoundReceipt>>,
    mutation_gate: tokio::sync::RwLock<()>,
    uncertain_dispatch: std::sync::atomic::AtomicBool,
}

impl Coordinator {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        store: Arc<Store>,
        policy: Arc<NativeLifecyclePolicy>,
        owner: Arc<Ownership>,
        lifecycle_task: impl Into<String>,
        child_task: impl Into<String>,
        authenticate: bool,
        cutoff: LifecycleCutoff,
        deadline_cap: Instant,
        cancel: tokio::sync::watch::Receiver<bool>,
        initial: BTreeMap<String, BoundReceipt>,
    ) -> Result<Arc<Self>> {
        policy.validate()?;
        if policy.format_version != 3 || authenticate != policy.authenticate {
            bail!("On-demand native coordinator does not match its approved policy")
        }
        owner.validate(&store)?;
        let inventory_endpoint = endpoint(&policy.api_base)?;
        let endpoint = inventory_endpoint.to_string();
        let mut api_base = inventory_endpoint;
        api_base.set_path("/api/v1");
        let api_base = api_base.to_string();
        let policy_sha256 = crate::store::sha256(&serde_json::to_vec(policy.as_ref())?);
        let deadline = cutoff.instant()?.min(deadline_cap);
        let models = policy
            .models
            .iter()
            .cloned()
            .map(|model| (model.model.clone(), model))
            .collect::<BTreeMap<_, _>>();
        for (model, binding) in &initial {
            let approved = models.get(model).ok_or_else(|| {
                anyhow::anyhow!("Restored receipt is outside the approved model scope")
            })?;
            Self::validate_receipt(approved, &binding.receipt, &endpoint)?;
        }
        Ok(Arc::new(Self {
            store,
            owner,
            lifecycle_task: lifecycle_task.into(),
            child_task: child_task.into(),
            endpoint,
            api_base,
            authenticate,
            policy_sha256,
            cutoff,
            deadline,
            cancel,
            models,
            cells: Mutex::new(BTreeMap::new()),
            committed: Mutex::new(initial.clone()),
            initial,
            mutation_gate: tokio::sync::RwLock::new(()),
            uncertain_dispatch: std::sync::atomic::AtomicBool::new(false),
        }))
    }

    fn cell(&self, model: &str) -> Result<Arc<ModelCell>> {
        let mut cells = self
            .cells
            .lock()
            .map_err(|_| anyhow::anyhow!("Native activation registry poisoned"))?;
        if !cells.contains_key(model) {
            let state = self
                .initial
                .get(model)
                .cloned()
                .map(|binding| Activation::Loaded(Box::new(binding)))
                .unwrap_or(Activation::Unloaded);
            cells.insert(
                model.to_owned(),
                Arc::new(ModelCell {
                    state: tokio::sync::Mutex::new(state),
                }),
            );
        }
        Ok(cells[model].clone())
    }

    fn ensure_active(&self) -> Result<()> {
        self.owner.validate(&self.store)?;
        super::ensure_setup_active(self.deadline, &self.cancel)
    }

    fn ensure_generation_allowed(&self) -> Result<()> {
        self.ensure_active()?;
        if self
            .uncertain_dispatch
            .load(std::sync::atomic::Ordering::Acquire)
        {
            bail!("An earlier native inference has an unknown outcome; further task inference is refused")
        }
        Ok(())
    }

    fn mark_uncertain_dispatch(&self) {
        self.uncertain_dispatch
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub(crate) fn target(&self, model: &str) -> Result<ManagedNativeTarget> {
        if !self.models.contains_key(model) {
            bail!("Native model is outside the approved lifecycle scope")
        }
        ManagedNativeTarget {
            api_base: self.api_base.clone(),
            model: model.to_owned(),
            policy_sha256: self.policy_sha256.clone(),
        }
        .canonical()
    }

    pub(crate) async fn activate(
        &self,
        task: &str,
        role: &str,
        model: &str,
        prompt: &str,
        expert_pinned: bool,
    ) -> Result<BoundReceipt> {
        if task != self.child_task {
            bail!("Native model activation belongs to another task")
        }
        crate::fabric::protocol::actor(role)?;
        self.ensure_generation_allowed()?;
        let ensure = self.models.get(model).ok_or_else(|| {
            anyhow::anyhow!("Native model is outside the approved lifecycle scope")
        })?;
        let cell = self.cell(model)?;
        let mut state = cell.state.lock().await;
        match &*state {
            Activation::Loaded(binding) => {
                self.ensure_active()?;
                return Ok((**binding).clone());
            }
            Activation::Loading | Activation::Unknown => {
                bail!("Native model activation has an unresolved prior outcome; retry is refused")
            }
            Activation::Unloaded => {}
        }

        // A dropped future leaves Loading behind. No later request in this task
        // can silently repeat a provider mutation whose effect may be unknown.
        *state = Activation::Loading;
        let _mutation = tokio::select! {
            biased;
            _ = super::wait_for_cancel(self.cancel.clone()) => {
                *state = Activation::Unloaded;
                bail!("Native activation cancelled before provider mutation")
            }
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(self.deadline)) => {
                *state = Activation::Unloaded;
                bail!("Native activation deadline exhausted before provider mutation")
            }
            guard = self.mutation_gate.write() => guard,
        };
        let local = match bounded(self.deadline, &self.cancel, async {
            managed::list(&self.store, &self.api_base)
        })
        .await
        {
            Ok(value) => value,
            Err(error) => {
                *state = Activation::Unloaded;
                return Err(error);
            }
        };
        if local["managed_lifecycle_blocked"] != false {
            *state = Activation::Unloaded;
            bail!("Managed native endpoint requires operator reconciliation")
        }
        let inventory_value = match bounded(self.deadline, &self.cancel, async {
            status(&self.api_base, self.authenticate).await
        })
        .await
        {
            Ok(value) => value,
            Err(error) => {
                *state = Activation::Unloaded;
                return Err(error);
            }
        };
        let inventory = match parse(&serde_json::to_vec(&inventory_value)?) {
            Ok(inventory) => inventory,
            Err(error) => {
                *state = Activation::Unloaded;
                return Err(error);
            }
        };
        let selected = inventory
            .iter()
            .find(|item| item.key == model)
            .ok_or_else(|| anyhow::anyhow!("Approved native model is not installed"))?;
        if ensure.offload_kv_cache_to_gpu.is_some()
            && !matches!(selected.kind.as_str(), "llm" | "decision")
        {
            *state = Activation::Unloaded;
            bail!("Explicit KV-cache GPU placement is supported only for LLM and decision models")
        }
        let receipt_rows = local["receipts"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Managed native receipt listing is malformed"))?;
        let matching = receipt_rows
            .iter()
            .filter(|row| {
                row["state"] == "active"
                    && row["receipt"]["model_key"] == model
                    && row["receipt"]["context_length"] == ensure.context_length
                    && row["receipt"]["offload_kv_cache_to_gpu"]
                        == json!(ensure.offload_kv_cache_to_gpu)
            })
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            *state = Activation::Unloaded;
            bail!("Ambiguous exact native model receipts")
        }
        let binding = if let Some(row) = matching.first() {
            let receipt: managed::Receipt = serde_json::from_value(row["receipt"].clone())?;
            Self::validate_receipt(ensure, &receipt, &self.endpoint)?;
            let observed = managed::validate_task_binding(
                &self.store,
                &receipt.receipt_id,
                &self.api_base,
                &inventory,
            )?;
            if serde_json::to_value(&observed)? != serde_json::to_value(&receipt)? {
                *state = Activation::Unloaded;
                bail!("Existing native receipt changed during first-use admission")
            }
            BoundReceipt {
                receipt,
                origin: "reused",
            }
        } else {
            if !selected.loaded_instances.is_empty() {
                *state = Activation::Unloaded;
                bail!(
                    "Loaded native model has no exact active managed receipt; adoption is refused"
                )
            }
            // From this point, cancellation/drop is an unresolved external
            // mutation. Keep Loading until the durable receipt and task binding
            // are both validated and stored.
            let loaded = match bounded(self.deadline, &self.cancel, async {
                managed::load_owned_with_gpu(
                    &self.store,
                    &self.api_base,
                    self.authenticate,
                    model,
                    ensure.context_length,
                    ensure.offload_kv_cache_to_gpu,
                    (true, Some(&self.owner)),
                )
                .await
            })
            .await
            {
                Ok(value) => value,
                Err(error) => {
                    *state = Activation::Unknown;
                    return Err(error);
                }
            };
            let receipt: managed::Receipt = match serde_json::from_value(loaded["receipt"].clone())
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    *state = Activation::Unknown;
                    return Err(error.into());
                }
            };
            if let Err(error) = Self::validate_receipt(ensure, &receipt, &self.endpoint) {
                *state = Activation::Unknown;
                return Err(error);
            }
            BoundReceipt {
                receipt,
                origin: "newly_loaded",
            }
        };

        self.ensure_active().inspect_err(|_error| {
            *state = Activation::Unknown;
        })?;
        let fresh = match bounded(self.deadline, &self.cancel, async {
            status(&self.api_base, self.authenticate).await
        })
        .await
        {
            Ok(value) => value,
            Err(error) => {
                *state = Activation::Unknown;
                return Err(error);
            }
        };
        let current = parse(&serde_json::to_vec(&fresh)?)?;
        let observed = match managed::validate_task_binding(
            &self.store,
            &binding.receipt.receipt_id,
            &self.api_base,
            &current,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                *state = Activation::Unknown;
                return Err(error);
            }
        };
        if serde_json::to_value(&observed)? != serde_json::to_value(&binding.receipt)? {
            *state = Activation::Unknown;
            bail!("Native receipt differs from current provider inventory")
        }
        let mut raw = json!({
            "format_version":1,"policy_sha256":self.policy_sha256,
            "child_task_id":self.child_task,"model_key":model,"receipt":binding.receipt,
            "receipt_sha256":crate::store::sha256(&serde_json::to_vec(&binding.receipt)?),
            "inventory_sha256":planning::fingerprint(&current)?,"origin":binding.origin,
            "role":role,"prompt_sha256":crate::store::sha256(prompt.as_bytes()),
            "backend_kind":if expert_pinned {"expert_pinned"} else {"configured_role"},
            "permissions_granted":false,"weights_attested":false,
            "response_instance_check_required":true,
        });
        let activation_binding_sha256 = crate::store::sha256(&serde_json::to_vec(&raw)?);
        raw["activation_binding_sha256"] = json!(activation_binding_sha256);
        if let Err(error) = self.store.event(
            &self.lifecycle_task,
            "native_task_model_binding",
            "security-kernel",
            raw,
        ) {
            *state = Activation::Unknown;
            return Err(error);
        }
        self.committed
            .lock()
            .map_err(|_| anyhow::anyhow!("Native activation binding registry poisoned"))?
            .insert(model.to_owned(), binding.clone());
        *state = Activation::Loaded(Box::new(binding.clone()));
        Ok(binding)
    }

    fn validate_receipt(
        ensure: &planning::EnsureLoaded,
        receipt: &managed::Receipt,
        endpoint: &str,
    ) -> Result<()> {
        if receipt.endpoint != endpoint
            || receipt.model_key != ensure.model
            || receipt.context_length != ensure.context_length
            || receipt.offload_kv_cache_to_gpu != ensure.offload_kv_cache_to_gpu
        {
            bail!("Native receipt differs from the approved model configuration")
        }
        Ok(())
    }

    pub(crate) fn bindings(&self) -> Result<BTreeMap<String, BoundReceipt>> {
        self.committed
            .lock()
            .map(|bindings| bindings.clone())
            .map_err(|_| anyhow::anyhow!("Native activation binding registry poisoned"))
    }

    pub(crate) fn newly_loaded_ids(&self) -> Result<Vec<String>> {
        let bindings = self.bindings()?;
        Ok(bindings
            .values()
            .filter(|binding| binding.origin == "newly_loaded")
            .map(|binding| binding.receipt.receipt_id.clone())
            .collect())
    }

    pub(crate) fn bindings_sha256(&self) -> Result<String> {
        let bindings = self.bindings()?;
        let receipts = bindings
            .iter()
            .map(|(model, binding)| (model.clone(), binding.receipt.clone()))
            .collect::<BTreeMap<_, _>>();
        let origins = bindings
            .iter()
            .map(|(model, binding)| (model.clone(), binding.origin.to_owned()))
            .collect::<BTreeMap<_, _>>();
        activation_bindings_sha256(&receipts, &origins)
    }

    pub(crate) fn has_uncertain_activation(&self) -> bool {
        self.cells.lock().map_or(true, |cells| {
            cells.values().any(|cell| {
                cell.state.try_lock().map_or(true, |state| {
                    matches!(*state, Activation::Loading | Activation::Unknown)
                })
            })
        })
    }

    pub(crate) fn has_uncertain_dispatch(&self) -> bool {
        self.uncertain_dispatch
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn cutoff(&self) -> &LifecycleCutoff {
        &self.cutoff
    }

    pub(crate) async fn generation_guard(&self) -> Result<tokio::sync::RwLockReadGuard<'_, ()>> {
        tokio::select! {
            biased;
            _ = super::wait_for_cancel(self.cancel.clone()) => bail!("Native inference cancelled before dispatch"),
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(self.deadline)) => bail!("Native inference deadline exhausted before dispatch"),
            guard = self.mutation_gate.read() => Ok(guard),
        }
    }

    pub(crate) fn lifecycle_event(
        &self,
        lifecycle_task: &str,
        state: &str,
        model: Option<&str>,
        receipt_id: Option<&str>,
        call_id: Option<&str>,
    ) -> Result<()> {
        let bindings = self.bindings_sha256()?;
        self.lifecycle_event_with_digest(
            lifecycle_task,
            state,
            model,
            receipt_id,
            call_id,
            &bindings,
        )
    }

    fn lifecycle_event_with_digest(
        &self,
        lifecycle_task: &str,
        state: &str,
        model: Option<&str>,
        receipt_id: Option<&str>,
        call_id: Option<&str>,
        bindings: &str,
    ) -> Result<()> {
        self.store.event(
            lifecycle_task,
            "native_task_lifecycle",
            "human-cli",
            json!({
                "format_version":2,"activation":"on_demand","state":state,
                "endpoint":self.endpoint,"child_task_id":self.child_task,
                "policy_sha256":self.policy_sha256,"model_bindings_sha256":bindings,
                "model_key":model,"receipt_id":receipt_id,"call_id":call_id,
                "permissions_granted":false,"provider_stop_proven":false,
            }),
        )
    }
}

async fn bounded<T>(
    deadline: Instant,
    cancel: &tokio::sync::watch::Receiver<bool>,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        _ = super::wait_for_cancel(cancel.clone()) => bail!("Native model activation cancelled; provider mutation outcome may be unknown"),
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => bail!("Original native lifecycle deadline exhausted"),
        result = future => result,
    }
}

/// A per-engine adapter. All cloned/pinned adapters share the operator-owned
/// coordinator, but retain their own generation settings.
#[derive(Clone)]
pub(crate) struct LazyBackend {
    config: Config,
    fixed_model: Option<String>,
    coordinator: Arc<Coordinator>,
    natives: Arc<Mutex<BTreeMap<String, Arc<crate::backend::native::NativeBackend>>>>,
}

struct UncertainCallGuard {
    coordinator: Arc<Coordinator>,
    armed: bool,
}

impl UncertainCallGuard {
    fn resolve(&mut self) {
        self.armed = false;
    }
}

impl Drop for UncertainCallGuard {
    fn drop(&mut self) {
        if self.armed {
            self.coordinator.mark_uncertain_dispatch();
        }
    }
}

struct LifecycleRootGuard {
    coordinator: Arc<Coordinator>,
    lifecycle_task: String,
    started: bool,
    armed: bool,
}

impl Drop for LifecycleRootGuard {
    fn drop(&mut self) {
        if self.started && self.armed {
            let _ = self.coordinator.lifecycle_event(
                &self.lifecycle_task,
                "outcome_unknown",
                None,
                None,
                None,
            );
        }
    }
}

impl LazyBackend {
    pub(crate) fn new(
        config: Config,
        coordinator: Arc<Coordinator>,
        fixed_model: Option<String>,
    ) -> Result<Self> {
        config.validate()?;
        let models = std::iter::once(config.model.as_str())
            .chain(
                config
                    .role_profiles
                    .values()
                    .map(|profile| profile.model.as_str()),
            )
            .collect::<BTreeSet<_>>();
        for model in models {
            if !coordinator.models.contains_key(model) {
                bail!("Native adapter configuration exceeds the approved lifecycle model scope")
            }
        }
        if let Some(model) = &fixed_model {
            if !coordinator.models.contains_key(model) {
                bail!("Native expert model is outside the approved lifecycle model scope")
            }
        }
        Ok(Self {
            config,
            fixed_model,
            coordinator,
            natives: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    fn model_for(&self, role: &str) -> Result<String> {
        crate::fabric::protocol::actor(role)?;
        let (configured, _) = self.config.model_settings(role)?;
        let model = self.fixed_model.as_deref().unwrap_or(configured);
        self.config.require_role_model(role, model)?;
        if !self.coordinator.models.contains_key(model) {
            bail!("Native role target is outside the approved lifecycle model scope")
        }
        Ok(model.to_owned())
    }

    fn native(
        &self,
        model: &str,
        receipt: &managed::Receipt,
    ) -> Result<Arc<crate::backend::native::NativeBackend>> {
        if let Some(native) = self
            .natives
            .lock()
            .map_err(|_| anyhow::anyhow!("Native adapter cache poisoned"))?
            .get(model)
            .cloned()
        {
            return Ok(native);
        }
        let mut config = self.config.clone();
        // This private cached adapter receives only the model selected by the
        // outer role guard. Preserve the original guard on the lazy wrapper;
        // a one-instance adapter cannot represent the entire network role map.
        if let Some(profile) = config.role_profiles.get("language") {
            if profile.model == model {
                config.temperature = profile.temperature;
                // The private instance adapter drops the outer routing marker;
                // pin language effort before that marker leaves its config.
                config.reasoning_effort = Some("none".into());
            }
        }
        config.role_profiles.remove("language");
        config.model = model.to_owned();
        for profile in config.role_profiles.values_mut() {
            profile.model = model.to_owned();
        }
        let native = Arc::new(crate::backend::native::NativeBackend::with_target(
            config,
            model,
            receipt.instance_id.clone(),
            self.coordinator.authenticate,
        )?);
        let mut cache = self
            .natives
            .lock()
            .map_err(|_| anyhow::anyhow!("Native adapter cache poisoned"))?;
        Ok(cache
            .entry(model.to_owned())
            .or_insert_with(|| native.clone())
            .clone())
    }

    fn uncertain(&self) -> bool {
        self.coordinator.has_uncertain_dispatch()
            || self.natives.lock().map_or(true, |natives| {
                natives
                    .values()
                    .any(|native| native.has_uncertain_dispatch())
            })
    }
}

impl InferenceBackend for LazyBackend {
    fn has_uncertain_dispatch(&self) -> bool {
        self.uncertain()
    }

    fn inference_target(&self, role: &str) -> Result<Option<crate::backend::InferenceTarget>> {
        let model = self.model_for(role)?;
        Ok(Some(crate::backend::InferenceTarget::ManagedNative(
            self.coordinator.target(&model)?,
        )))
    }

    fn pin(&self) -> Result<Option<Arc<dyn InferenceBackend>>> {
        Ok(Some(Arc::new(self.clone())))
    }

    fn generate<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<crate::backend::Generation>> + Send + 'a>,
    > {
        Box::pin(async { bail!("On-demand native inference requires its original task identity") })
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
            let model = self.model_for(role)?;
            let binding = self
                .coordinator
                .activate(task, role, &model, prompt, self.fixed_model.is_some())
                .await?;
            let _generation_guard = self.coordinator.generation_guard().await?;
            self.coordinator.ensure_generation_allowed()?;
            let call_id = uuid::Uuid::new_v4().to_string();
            let model_bindings_sha256 = self.coordinator.bindings_sha256()?;
            self.coordinator.lifecycle_event_with_digest(
                &self.coordinator.lifecycle_task,
                "generation_started",
                Some(&model),
                Some(&binding.receipt.receipt_id),
                Some(&call_id),
                &model_bindings_sha256,
            )?;
            let mut call_guard = UncertainCallGuard {
                coordinator: self.coordinator.clone(),
                armed: true,
            };
            let native = match self.native(&model, &binding.receipt) {
                Ok(native) => native,
                Err(error) => {
                    if let Err(audit_error) = self.coordinator.lifecycle_event_with_digest(
                        &self.coordinator.lifecycle_task,
                        "generation_not_dispatched",
                        Some(&model),
                        Some(&binding.receipt.receipt_id),
                        Some(&call_id),
                        &model_bindings_sha256,
                    ) {
                        self.coordinator.mark_uncertain_dispatch();
                        return Err(audit_error);
                    }
                    call_guard.resolve();
                    return Err(error);
                }
            };
            let bound = super::BoundBackend {
                inner: native,
                ownership: self.coordinator.owner.clone(),
                base: self.coordinator.api_base.clone(),
                authenticate: self.coordinator.authenticate,
                child_task: self.coordinator.child_task.clone(),
                receipts: [(role.to_owned(), binding.receipt.clone())]
                    .into_iter()
                    .collect(),
            };
            match bound.generate_for(task, role, prompt, max_tokens).await {
                Ok(report) => {
                    if let Err(audit_error) = self.coordinator.lifecycle_event_with_digest(
                        &self.coordinator.lifecycle_task,
                        "generation_confirmed",
                        Some(&model),
                        Some(&binding.receipt.receipt_id),
                        Some(&call_id),
                        &model_bindings_sha256,
                    ) {
                        self.coordinator.mark_uncertain_dispatch();
                        return Err(audit_error);
                    }
                    call_guard.resolve();
                    Ok(report)
                }
                Err(error) => {
                    let uncertain = self.uncertain();
                    if uncertain {
                        self.coordinator.mark_uncertain_dispatch();
                    }
                    if let Err(audit_error) = self.coordinator.lifecycle_event_with_digest(
                        &self.coordinator.lifecycle_task,
                        if uncertain {
                            "generation_outcome_unknown"
                        } else {
                            "generation_not_dispatched"
                        },
                        Some(&model),
                        Some(&binding.receipt.receipt_id),
                        Some(&call_id),
                        &model_bindings_sha256,
                    ) {
                        self.coordinator.mark_uncertain_dispatch();
                        return Err(audit_error);
                    }
                    call_guard.resolve();
                    Err(error)
                }
            }
        })
    }
}

pub(crate) fn backend_for_expert(
    config: Config,
    coordinator: Arc<Coordinator>,
    model: String,
) -> Result<Arc<LazyBackend>> {
    Ok(Arc::new(LazyBackend::new(
        config,
        coordinator,
        Some(model),
    )?))
}

#[derive(Serialize)]
pub(crate) struct BindingEvidence {
    pub format_version: u32,
    pub child_task_id: String,
    pub request_sha256: String,
    pub policy_sha256: String,
    pub receipts: BTreeMap<String, managed::Receipt>,
    pub bindings_sha256: String,
    pub lifecycle_boot_id: String,
    pub lifecycle_deadline_ns: u64,
    pub permissions_granted: bool,
    pub weights_attested: bool,
    pub response_instance_check_required: bool,
}

pub(crate) fn empty_binding_evidence(
    child: &str,
    request_sha256: &str,
    policy_sha256: &str,
    cutoff: &LifecycleCutoff,
) -> Result<BindingEvidence> {
    let receipts = BTreeMap::new();
    Ok(BindingEvidence {
        format_version: 2,
        child_task_id: child.to_owned(),
        request_sha256: request_sha256.to_owned(),
        policy_sha256: policy_sha256.to_owned(),
        bindings_sha256: crate::store::sha256(&serde_json::to_vec(&receipts)?),
        receipts,
        lifecycle_boot_id: cutoff.boot_id.clone(),
        lifecycle_deadline_ns: cutoff.deadline_ns,
        permissions_granted: false,
        weights_attested: false,
        response_instance_check_required: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_on_demand(
    store: Arc<Store>,
    cfg: Config,
    template: Option<&Engine>,
    verifier: Option<Arc<dyn crate::verification::VerifierProvider>>,
    options: crate::language::TaskOptions,
    profile: crate::engine::ExecutionProfile,
    execution_start: crate::execution_clock::ExecutionStart,
    child_task: String,
    lifecycle_task: String,
    request: Request,
    policy: Arc<NativeLifecyclePolicy>,
    policy_sha256: String,
    request_sha256: String,
    owner: Arc<Ownership>,
    task_modules: Arc<crate::fabric::modules::Broker>,
    cutoff: LifecycleCutoff,
    deadline_cap: Instant,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<Report> {
    let deadline = execution_start
        .instant
        .checked_add(Duration::from_secs(request.max_lifecycle_seconds))
        .map(|policy_deadline| execution_start.deadline.min(policy_deadline))
        .unwrap_or(execution_start.deadline);
    let deadline = deadline.min(deadline_cap).min(cutoff.instant()?);
    owner.validate(&store)?;
    let coordinator = Coordinator::new(
        store.clone(),
        policy.clone(),
        owner.clone(),
        lifecycle_task.clone(),
        child_task.clone(),
        request.authenticate,
        cutoff.clone(),
        deadline,
        cancel.clone(),
        BTreeMap::new(),
    )?;
    let mut native_cfg = cfg.clone();
    native_cfg.api_base = request.api_base.clone();
    let main = Arc::new(LazyBackend::new(native_cfg, coordinator.clone(), None)?);
    let live_backends = Arc::new(Mutex::new(BTreeMap::<Vec<u8>, Arc<LazyBackend>>::new()));
    let factory_store = store.clone();
    let factory_owner = owner.clone();
    let factory_coordinator = coordinator.clone();
    let factory_base = request.api_base.clone();
    let factory_backends = live_backends.clone();
    let factory: crate::engine::ModelBackendFactory = Arc::new(move |model_cfg| {
        let registered = url::Url::parse(&model_cfg.api_base)?;
        let native_origin = url::Url::parse(&factory_base)?;
        if registered.origin() != native_origin.origin() {
            bail!("Expert registration points outside the approved native provider")
        }
        factory_owner.validate(&factory_store)?;
        let mut native_cfg = model_cfg.clone();
        native_cfg.api_base = factory_base.clone();
        let adapter = backend_for_expert(
            native_cfg,
            factory_coordinator.clone(),
            model_cfg.model.clone(),
        )?;
        // A model name is not a sufficient cache key: expert registrations can
        // differ in context, sampling, resource declarations, or other config.
        // Use the complete serialized config so only identical adapters share
        // backend state, while avoiding one duplicate per repeated factory call.
        let identity = serde_json::to_vec(&model_cfg)?;
        let mut registered = factory_backends
            .lock()
            .map_err(|_| anyhow::anyhow!("Native expert backend registry poisoned"))?;
        if let Some(adapter) = registered.get(&identity) {
            return Ok(adapter.clone() as Arc<dyn InferenceBackend>);
        }
        if registered.len() >= 9 {
            bail!("Native expert adapter capacity exhausted")
        }
        registered.insert(identity, adapter.clone());
        Ok(adapter as Arc<dyn InferenceBackend>)
    });

    let main_backend: Arc<dyn InferenceBackend> = main;
    let engine = if let Some(template) = template {
        template.with_owned_native_runtime(
            main_backend,
            owner.clone(),
            factory,
            policy_sha256.clone(),
            Some("on_demand"),
            task_modules,
        )?
    } else {
        let engine = Engine::new(cfg.clone(), store.clone()).with_owned_native_runtime(
            main_backend,
            owner.clone(),
            factory,
            policy_sha256.clone(),
            Some("on_demand"),
            task_modules,
        )?;
        match verifier {
            Some(provider) => engine.with_verifier_provider(provider)?,
            None => engine,
        }
    };
    engine.configured_resource_pool()?;
    super::ensure_setup_active(deadline, &cancel)?;

    let evidence = empty_binding_evidence(&child_task, &request_sha256, &policy_sha256, &cutoff)?;
    store.event(
        &lifecycle_task,
        "native_task_bindings",
        "human-cli",
        serde_json::to_value(evidence)?,
    )?;
    let mut root_guard = LifecycleRootGuard {
        coordinator: coordinator.clone(),
        lifecycle_task: lifecycle_task.clone(),
        started: false,
        armed: true,
    };
    coordinator.lifecycle_event(&lifecycle_task, "execution_started", None, None, None)?;
    root_guard.started = true;

    let run_cancel = cancel.clone();
    let outcome = engine
        .run_execution_body(
            &request.query,
            &request.mode,
            crate::engine::TaskExecution {
                task_id: child_task.clone(),
                options,
                profile,
                execution_start,
                task_precreated: true,
            },
            async move {
                let mut receiver = run_cancel;
                if !*receiver.borrow_and_update() {
                    let _ = receiver.changed().await;
                }
            },
        )
        .await?;

    let uncertain = coordinator.has_uncertain_activation()
        || coordinator.has_uncertain_dispatch()
        || live_backends
            .lock()
            .map_err(|_| anyhow::anyhow!("Native expert backend registry poisoned"))?
            .values()
            .any(|backend| backend.has_uncertain_dispatch());
    let cleanup_safe = !uncertain
        && !matches!(
            outcome.status.as_str(),
            "cancelled" | "exhausted" | "failed" | "interrupted"
        );
    let bindings = coordinator.bindings()?;
    let mut released = Vec::new();
    if request.release_after && cleanup_safe {
        for receipt_id in coordinator.newly_loaded_ids()? {
            bounded(deadline, &cancel, async {
                managed::release_owned(
                    &store,
                    &request.api_base,
                    request.authenticate,
                    &receipt_id,
                    (true, Some(&owner)),
                )
                .await?;
                Ok(())
            })
            .await?;
            released.push(receipt_id);
        }
    }
    coordinator.lifecycle_event(
        &lifecycle_task,
        if uncertain {
            "outcome_unknown"
        } else {
            "execution_finished"
        },
        None,
        None,
        None,
    )?;
    root_guard.armed = false;
    store.finish_task(
        &lifecycle_task,
        if uncertain { "interrupted" } else { "completed" },
        if uncertain {
            "On-demand native inference or activation remains unknown; cleanup deferred"
        } else {
            "On-demand native lifecycle finished; provider weights and physical residency are unattested"
        },
    )?;
    Ok(Report {
        lifecycle_task_id: lifecycle_task,
        outcome,
        receipt_ids: bindings
            .values()
            .map(|binding| binding.receipt.receipt_id.clone())
            .collect(),
        released_receipt_ids: released.clone(),
        release_deferred: request.release_after
            && (!cleanup_safe || released.len() != coordinator.newly_loaded_ids()?.len()),
        provider_stop_proven: false,
        ownership_established: false,
    })
}
