//! Host-configured, process-local bounded admission across inference modules.
//! Declared allocations are not physical memory limits or provider-stop proof.
use crate::{backend::InferenceBackend, security, store::Store};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

mod adapter;
mod admission;
mod configured;
mod profiles;
mod queue;
mod resident;
use profiles::canonical_working_profiles;
pub use profiles::WorkingProfile;
pub use queue::PriorityScheduling;
pub use resident::{ResidentAllocation, ResidentConsumer};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub compute_slots: u32,
    pub host_memory_bytes: u64,
    pub device_memory_bytes: u64,
}
impl Allocation {
    fn fits(self, capacity: Self) -> bool {
        self.compute_slots <= capacity.compute_slots
            && self.host_memory_bytes <= capacity.host_memory_bytes
            && self.device_memory_bytes <= capacity.device_memory_bytes
    }
    fn add(&mut self, rhs: Self) {
        self.compute_slots += rhs.compute_slots;
        self.host_memory_bytes += rhs.host_memory_bytes;
        self.device_memory_bytes += rhs.device_memory_bytes;
    }
    fn subtract(&mut self, rhs: Self) {
        self.compute_slots -= rhs.compute_slots;
        self.host_memory_bytes -= rhs.host_memory_bytes;
        self.device_memory_bytes -= rhs.device_memory_bytes;
    }
    fn available(self, used: Self) -> Self {
        let mut value = self;
        value.subtract(used);
        value
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub label: String,
    pub capacity: Allocation,
    pub max_pending: usize,
    pub max_wait_ms: u64,
    pub max_execution_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduling: Option<PriorityScheduling>,
}
impl Settings {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.label.len() > 64
            || !crate::registry::identifier(&self.label)
            || !(1..=256).contains(&self.capacity.compute_slots)
            || self.capacity.host_memory_bytes > 1 << 60
            || self.capacity.device_memory_bytes > 1 << 60
            || !(1..=1024).contains(&self.max_pending)
            || !(1..=60_000).contains(&self.max_wait_ms)
            || !(1..=600_000).contains(&self.max_execution_ms)
        {
            bail!("Invalid resource pool bounds")
        }
        if let Some(policy) = &self.scheduling {
            policy.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub settings: Settings,
    pub allocation: Allocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residents: Vec<ResidentAllocation>,
    /// Optional exact working-set declarations for a role and configured local model.
    /// Empty preserves the historical uniform-allocation behavior.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub working_profiles: Vec<WorkingProfile>,
}
impl Configuration {
    pub fn validate(&self) -> Result<()> {
        self.settings.validate()?;
        let (_, resident) = resident::canonical(&self.residents, self.settings.capacity)?;
        canonical_working_profiles(&self.working_profiles, self.settings.capacity, resident)?;
        if self.allocation.compute_slots == 0
            || !self
                .allocation
                .fits(self.settings.capacity.available(resident))
        {
            bail!("Configured inference allocation cannot fit the pool")
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Waiting,
    Reserved,
    Dispatched,
    Quarantined,
    Resolving,
}
struct Entry {
    task: String,
    allocation: Allocation,
    phase: Phase,
    notify: Arc<Notify>,
    priority: u8,
    bypasses: u16,
}
#[derive(Default)]
struct State {
    used: Allocation,
    entries: BTreeMap<String, Entry>,
    queue: VecDeque<String>,
    closed: bool,
}
#[derive(Debug, Serialize)]
pub struct ReservationStatus {
    pub reservation_id: String,
    pub task_id: String,
    pub allocation: Allocation,
    pub phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_bypasses: Option<u16>,
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub format_version: u32,
    pub pool_id: String,
    pub settings: Settings,
    pub allocated: Allocation,
    pub resident_allocated: Allocation,
    pub working_allocated: Allocation,
    pub residents: Vec<ResidentAllocation>,
    pub resident_catalog_sha256: String,
    pub residency_attested: bool,
    pub closed: bool,
    pub reservations: Vec<ReservationStatus>,
    pub physical_limits_enforced: bool,
    pub provider_stop_proven: bool,
}

/// Share one Arc between every backend that competes for the same declared budget.
/// Creation/configuration is a trusted host operation, never a model decision.
pub struct ResourcePool {
    id: String,
    settings: Settings,
    store: Arc<Store>,
    state: Mutex<State>,
    residents: Vec<ResidentAllocation>,
    resident_allocated: Allocation,
    resident_catalog_sha256: String,
    working_profiles: Vec<WorkingProfile>,
    working_profiles_sha256: Option<String>,
    _workspace_lease: Option<crate::recovery::ExecutionLease>,
    _managed_ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
}

pub(crate) struct RoutingCandidate {
    pub target: crate::backend::InferenceTarget,
    pub metadata: Value,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum RoutingRejection {
    UnknownTarget,
    MetadataError,
    UnlistedProfile,
    ResidentMiss,
}
impl RoutingRejection {
    pub(crate) fn reason_code(self) -> &'static str {
        match self {
            Self::UnknownTarget => "unknown_target",
            Self::MetadataError => "metadata_error",
            Self::UnlistedProfile => "unlisted_profile",
            Self::ResidentMiss => "resident_miss",
        }
    }
}
impl ResourcePool {
    /// Persist the immutable resource catalog scope before initial route metadata
    /// callbacks. The event is task-scoped but grants no provider or model rights.
    pub(crate) fn record_routing_intent(&self, task: &str) -> Result<Value> {
        let binding = Self::configuration_binding_from_pool(self)?;
        let intent = json!({
            "format_version":1,
            "pool_id":self.id,
            "binding":binding,
            "roles":crate::fabric::protocol::ROLES,
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false,
        });
        self.store
            .event(task, "routing_resource_intent", "jev-plus", intent.clone())?;
        Ok(intent)
    }

    pub(crate) fn record_routing_selection(
        &self,
        task: &str,
        selection: &Value,
        eligible: bool,
    ) -> Result<()> {
        let mut selection = selection.clone();
        selection["eligible"] = json!(eligible);
        self.store
            .event(task, "routing_resource_selection", "jev-plus", selection)
    }

    fn configuration_binding_from_pool(&self) -> Result<Value> {
        let mut binding = json!({
            "resident_catalog_sha256":self.resident_catalog_sha256,
            "resident_allocated":self.resident_allocated,
            "residency_attested":false,
        });
        if let Some(digest) = &self.working_profiles_sha256 {
            binding["working_profiles_sha256"] = json!(digest);
        }
        Ok(binding)
    }

    /// Resolve a candidate target against the existing canonical operator
    /// declarations. The caller must treat any error as an ineligible candidate
    /// and must not expose its text in task audit records.
    pub(crate) fn routing_candidate(
        &self,
        role: &str,
        target: Option<crate::backend::InferenceTarget>,
    ) -> std::result::Result<RoutingCandidate, RoutingRejection> {
        let canonical_target = match target {
            Some(crate::backend::InferenceTarget::LocalHttp(target)) => match target.canonical() {
                Ok(target) => crate::backend::InferenceTarget::LocalHttp(target),
                Err(_) => return Err(RoutingRejection::MetadataError),
            },
            Some(crate::backend::InferenceTarget::Native(target)) => match target.canonical() {
                Ok(target) => crate::backend::InferenceTarget::Native(target),
                Err(_) => return Err(RoutingRejection::MetadataError),
            },
            Some(crate::backend::InferenceTarget::ManagedNative(target)) => {
                match target.canonical() {
                    Ok(target) => crate::backend::InferenceTarget::ManagedNative(target),
                    Err(_) => return Err(RoutingRejection::MetadataError),
                }
            }
            Some(crate::backend::InferenceTarget::Deterministic) => {
                crate::backend::InferenceTarget::Deterministic
            }
            None => return Err(RoutingRejection::UnknownTarget),
        };
        let (profile_id, allocation) = if self.working_profiles.is_empty() {
            (None, None)
        } else {
            let profile = match &canonical_target {
                crate::backend::InferenceTarget::LocalHttp(target) => {
                    match profiles::select_profile(&self.working_profiles, role, target) {
                        Ok(Some(profile)) => profile,
                        Ok(None) => return Err(RoutingRejection::UnlistedProfile),
                        Err(_) => return Err(RoutingRejection::MetadataError),
                    }
                }
                crate::backend::InferenceTarget::Native(target) => {
                    let resource_target = match target.resource_target() {
                        Ok(target) => target,
                        Err(_) => return Err(RoutingRejection::MetadataError),
                    };
                    match profiles::select_profile(&self.working_profiles, role, &resource_target) {
                        Ok(Some(profile)) => profile,
                        Ok(None) => return Err(RoutingRejection::UnlistedProfile),
                        Err(_) => return Err(RoutingRejection::MetadataError),
                    }
                }
                crate::backend::InferenceTarget::ManagedNative(target) => {
                    let resource_target = match target.resource_target() {
                        Ok(target) => target,
                        Err(_) => return Err(RoutingRejection::MetadataError),
                    };
                    match profiles::select_profile(&self.working_profiles, role, &resource_target) {
                        Ok(Some(profile)) => profile,
                        Ok(None) => return Err(RoutingRejection::UnlistedProfile),
                        Err(_) => return Err(RoutingRejection::MetadataError),
                    }
                }
                crate::backend::InferenceTarget::Deterministic => {
                    return Err(RoutingRejection::UnlistedProfile)
                }
            };
            (Some(profile.id), Some(profile.allocation))
        };
        let resident_coverage = if self.residents.is_empty() {
            None
        } else {
            match self.resident_coverage(Some(canonical_target.clone())) {
                Ok(coverage) => Some(
                    serde_json::to_value(coverage).map_err(|_| RoutingRejection::MetadataError)?,
                ),
                Err(_) => return Err(RoutingRejection::ResidentMiss),
            }
        };
        let metadata = json!({
            "target":canonical_target,
            "profile_id":profile_id,
            "allocation":allocation,
            "resident_coverage":resident_coverage,
            "eligible":true,
            "reason_code":"configured_resource_match",
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false,
        });
        Ok(RoutingCandidate {
            target: canonical_target,
            metadata,
        })
    }

    pub(crate) fn routing_identity(&self) -> Result<Value> {
        Ok(json!({
            "pool_id":self.id,
            "binding":self.configuration_binding_from_pool()?,
            "permissions_granted":false,
            "weights_attested":false,
            "physical_limits_enforced":false,
        }))
    }

    pub(crate) fn matches(&self, store: &Arc<Store>, configuration: &Configuration) -> bool {
        self.matches_ref(store, configuration)
    }
    pub(crate) fn matches_ref(&self, store: &Store, configuration: &Configuration) -> bool {
        std::ptr::eq(self.store.as_ref(), store)
            && self.settings == configuration.settings
            && resident::canonical(&configuration.residents, self.settings.capacity)
                .is_ok_and(|(catalog, _)| catalog == self.residents)
            && canonical_working_profiles(
                &configuration.working_profiles,
                self.settings.capacity,
                self.resident_allocated,
            )
            .is_ok_and(|profiles| profiles == self.working_profiles)
    }
    pub(crate) fn configuration_binding(configuration: &Configuration) -> Result<Value> {
        let (residents, allocated) =
            resident::canonical(&configuration.residents, configuration.settings.capacity)?;
        let mut binding = json!({"resident_catalog_sha256":crate::store::sha256(&serde_json::to_vec(&residents)?),"resident_allocated":allocated,"residency_attested":false});
        let profiles = canonical_working_profiles(
            &configuration.working_profiles,
            configuration.settings.capacity,
            allocated,
        )?;
        if !profiles.is_empty() {
            binding["working_profiles_sha256"] =
                json!(crate::store::sha256(&serde_json::to_vec(&profiles)?));
        }
        Ok(binding)
    }
    pub(crate) fn configuration_coverage(
        configuration: &Configuration,
        target: Option<crate::backend::InferenceTarget>,
    ) -> Result<Value> {
        let (residents, _) =
            resident::canonical(&configuration.residents, configuration.settings.capacity)?;
        Ok(serde_json::to_value(resident::coverage(
            &residents, target,
        )?)?)
    }
    pub fn new(actor: &str, store: Arc<Store>, settings: Settings) -> Result<Arc<Self>> {
        Self::with_residents(actor, store, settings, Vec::new())
    }
    /// Reserve operator-declared resident memory once for the lifetime of a pool.
    /// This neither loads weights nor proves provider residency or sharing.
    pub fn with_residents(
        actor: &str,
        store: Arc<Store>,
        settings: Settings,
        residents: Vec<ResidentAllocation>,
    ) -> Result<Arc<Self>> {
        Self::with_profiles(actor, store, settings, residents, Vec::new())
    }
    fn with_profiles(
        actor: &str,
        store: Arc<Store>,
        settings: Settings,
        residents: Vec<ResidentAllocation>,
        working_profiles: Vec<WorkingProfile>,
    ) -> Result<Arc<Self>> {
        security::authorize(actor, "resources.configure")?;
        settings.validate()?;
        let (residents, resident_allocated) = resident::canonical(&residents, settings.capacity)?;
        let working_profiles =
            canonical_working_profiles(&working_profiles, settings.capacity, resident_allocated)?;
        let working_profiles_sha256 = if working_profiles.is_empty() {
            None
        } else {
            Some(crate::store::sha256(&serde_json::to_vec(
                &working_profiles,
            )?))
        };
        let resident_catalog_sha256 = crate::store::sha256(&serde_json::to_vec(&residents)?);
        Ok(Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            settings,
            store,
            state: Mutex::new(State {
                used: resident_allocated,
                ..State::default()
            }),
            residents,
            resident_allocated,
            resident_catalog_sha256,
            working_profiles,
            working_profiles_sha256,
            _workspace_lease: None,
            _managed_ownership: None,
        }))
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| anyhow!("Resource pool poisoned"))
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let state = self.lock()?;
        Ok(Snapshot {
            format_version: 1,
            pool_id: self.id.clone(),
            settings: self.settings.clone(),
            allocated: state.used,
            resident_allocated: self.resident_allocated,
            working_allocated: state.used.available(self.resident_allocated),
            residents: self.residents.clone(),
            resident_catalog_sha256: self.resident_catalog_sha256.clone(),
            residency_attested: false,
            closed: state.closed,
            reservations: state
                .entries
                .iter()
                .map(|(id, entry)| ReservationStatus {
                    reservation_id: id.clone(),
                    task_id: entry.task.clone(),
                    allocation: entry.allocation,
                    phase: entry.phase,
                    queue_priority: self.settings.scheduling.as_ref().map(|_| entry.priority),
                    queue_bypasses: self.settings.scheduling.as_ref().map(|_| entry.bypasses),
                })
                .collect(),
            physical_limits_enforced: false,
            provider_stop_proven: false,
        })
    }
    /// The fixed allocation covers one dispatched operation, not resident weights.
    /// Wrapping does not load models, probe hardware, or grant inference authority.
    pub fn wrap_backend(
        self: &Arc<Self>,
        actor: &str,
        backend: Arc<dyn InferenceBackend>,
        allocation: Allocation,
    ) -> Result<Arc<dyn InferenceBackend>> {
        security::authorize(actor, "resources.configure")?;
        if allocation.compute_slots == 0
            || !allocation.fits(self.settings.capacity.available(self.resident_allocated))
        {
            bail!("Inference allocation cannot fit the resource pool")
        }
        if self.lock()?.closed {
            bail!("Resource pool closed")
        }
        Ok(Arc::new(adapter::ScheduledBackend {
            pool: self.clone(),
            backend,
            allocation,
            use_working_profiles: !self.working_profiles.is_empty(),
        }))
    }
    /// Wrap a backend using this pool's immutable configured role/target profiles.
    /// An empty profile table preserves the legacy uniform allocation behavior.
    pub(crate) fn wrap_configured_backend(
        self: &Arc<Self>,
        actor: &str,
        backend: Arc<dyn InferenceBackend>,
        configuration: &Configuration,
    ) -> Result<Arc<dyn InferenceBackend>> {
        security::authorize(actor, "resources.configure")?;
        configuration.validate()?;
        if !self.matches_ref(self.store.as_ref(), configuration) {
            bail!("Configured inference pool does not match the selected configuration")
        }
        if self.lock()?.closed {
            bail!("Resource pool closed")
        }
        Ok(Arc::new(adapter::ScheduledBackend {
            pool: self.clone(),
            backend,
            allocation: configuration.allocation,
            use_working_profiles: !self.working_profiles.is_empty(),
        }))
    }
    fn pump(&self, state: &mut State) {
        if state.closed {
            return;
        }
        while let Some(index) = self.queue_index(state) {
            let id = state.queue[index].clone();
            let entry = state
                .entries
                .get(&id)
                .expect("Queued resource entry exists");
            if !entry
                .allocation
                .fits(self.settings.capacity.available(state.used))
            {
                break;
            }
            if self.settings.scheduling.is_some() {
                for earlier in state.queue.iter().take(index) {
                    let entry = state
                        .entries
                        .get_mut(earlier)
                        .expect("Earlier queued entry exists");
                    entry.bypasses = entry.bypasses.saturating_add(1);
                }
            }
            let entry = state
                .entries
                .get_mut(&id)
                .expect("Selected resource entry exists");
            state.used.add(entry.allocation);
            entry.phase = Phase::Reserved;
            entry.notify.notify_one();
            state.queue.remove(index);
        }
    }

    fn remove(&self, state: &mut State, id: &str) {
        if let Some(entry) = state.entries.remove(id) {
            if entry.phase == Phase::Waiting {
                state.queue.retain(|queued| queued != id);
            } else {
                state.used.subtract(entry.allocation);
            }
            self.pump(state);
        }
    }
    #[cfg(test)]
    async fn acquire(
        self: &Arc<Self>,
        task: &str,
        allocation: Allocation,
        id: String,
    ) -> Result<Lease> {
        self.acquire_priority(task, allocation, id, 0).await
    }
    async fn acquire_priority(
        self: &Arc<Self>,
        task: &str,
        allocation: Allocation,
        id: String,
        priority: u8,
    ) -> Result<Lease> {
        let notify = Arc::new(Notify::new());
        {
            let mut state = self.lock()?;
            if state.closed {
                bail!("Resource pool closed")
            }
            // Pump first, so the bound counts waiting entries rather than running ones.
            self.pump(&mut state);
            if state.queue.len() >= self.settings.max_pending {
                bail!("Resource queue full")
            }
            state.entries.insert(
                id.clone(),
                Entry {
                    task: task.into(),
                    allocation,
                    phase: Phase::Waiting,
                    notify: notify.clone(),
                    priority,
                    bypasses: 0,
                },
            );
            state.queue.push_back(id.clone());
            self.pump(&mut state);
        }
        let lease = Lease {
            pool: self.clone(),
            id,
            live: true,
        };
        let wait = async {
            loop {
                // Notify holds a permit if admission/shutdown raced this future.
                let notified = notify.notified();
                {
                    let state = self.lock()?;
                    if state.closed {
                        bail!("Resource pool closed")
                    }
                    if state.entries[&lease.id].phase == Phase::Reserved {
                        return Ok(());
                    }
                }
                notified.await;
            }
        };
        tokio::time::timeout(Duration::from_millis(self.settings.max_wait_ms), wait)
            .await
            .map_err(|_| anyhow!("Resource admission deadline exhausted"))??;
        Ok(lease)
    }
    /// Stop new admissions and wake every pending caller. Running work is not killed.
    pub fn shutdown(&self, actor: &str) -> Result<()> {
        security::authorize(actor, "resources.configure")?;
        let mut state = self.lock()?;
        state.closed = true;
        for entry in state.entries.values() {
            entry.notify.notify_one();
        }
        Ok(())
    }
    /// Explicit operator acknowledgement, after independently stopping/checking
    /// the provider. Only a quarantined reservation may be released this way.
    pub fn acknowledge_stopped(
        &self,
        actor: &str,
        reservation: &str,
        confirmed: bool,
    ) -> Result<()> {
        security::authorize(actor, "resources.resolve")?;
        if !confirmed {
            bail!("Resource release requires explicit provider-stop acknowledgement")
        }
        uuid::Uuid::parse_str(reservation)?;
        let (task, allocation) = {
            let mut state = self.lock()?;
            let entry = state
                .entries
                .get_mut(reservation)
                .ok_or_else(|| anyhow!("Unknown resource reservation"))?;
            if entry.phase != Phase::Quarantined {
                bail!("Resource reservation is not quarantined")
            }
            entry.phase = Phase::Resolving;
            (entry.task.clone(), entry.allocation)
        };
        // No callbacks/Store publication while holding the resource lock.
        let result = self.store.event(&task, "resource_release", "human-cli", json!({
            "format_version":1,"pool_id":self.id,"reservation_id":reservation,
            "allocation":allocation,"operator_acknowledged_stop":true,"provider_stop_proven":false,
        }));
        let mut state = self.lock()?;
        if let Err(error) = result {
            state
                .entries
                .get_mut(reservation)
                .expect("Resolving entry remains owned")
                .phase = Phase::Quarantined;
            return Err(error);
        }
        self.remove(&mut state, reservation);
        Ok(())
    }
}

struct Lease {
    pool: Arc<ResourcePool>,
    id: String,
    live: bool,
}
impl Lease {
    fn dispatched(&self) -> Result<()> {
        let mut state = self.pool.lock()?;
        if state.closed {
            bail!("Resource pool closed")
        }
        let entry = state.entries.get_mut(&self.id).expect("Lease entry exists");
        if entry.phase != Phase::Reserved {
            bail!("Resource reservation is not ready")
        }
        entry.phase = Phase::Dispatched;
        Ok(())
    }
    fn release(mut self) -> Result<()> {
        self.pool.remove(&mut *self.pool.lock()?, &self.id);
        self.live = false;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.live {
            return;
        }
        if let Ok(mut state) = self.pool.lock() {
            if state
                .entries
                .get(&self.id)
                .is_some_and(|entry| entry.phase == Phase::Dispatched)
            {
                state.entries.get_mut(&self.id).unwrap().phase = Phase::Quarantined;
            } else {
                self.pool.remove(&mut state, &self.id);
            }
        }
    }
}

#[cfg(test)]
mod priority_tests;
