//! Host-configured, process-local FIFO admission across inference modules.
//! Declared allocations are not physical memory limits or provider-stop proof.
use crate::{backend::InferenceBackend, security, store::Store};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

mod adapter;
mod admission;
mod configured;
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
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub settings: Settings,
    pub allocation: Allocation,
}
impl Configuration {
    pub fn validate(&self) -> Result<()> {
        self.settings.validate()?;
        if self.allocation.compute_slots == 0 || !self.allocation.fits(self.settings.capacity) {
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
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub format_version: u32,
    pub pool_id: String,
    pub settings: Settings,
    pub allocated: Allocation,
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
    _workspace_lease: Option<crate::recovery::ExecutionLease>,
}
impl ResourcePool {
    pub(crate) fn matches(&self, store: &Arc<Store>, configuration: &Configuration) -> bool {
        Arc::ptr_eq(&self.store, store) && self.settings == configuration.settings
    }
    pub fn new(actor: &str, store: Arc<Store>, settings: Settings) -> Result<Arc<Self>> {
        security::authorize(actor, "resources.configure")?;
        settings.validate()?;
        Ok(Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            settings,
            store,
            state: Mutex::new(State::default()),
            _workspace_lease: None,
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
            closed: state.closed,
            reservations: state
                .entries
                .iter()
                .map(|(id, entry)| ReservationStatus {
                    reservation_id: id.clone(),
                    task_id: entry.task.clone(),
                    allocation: entry.allocation,
                    phase: entry.phase,
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
        if allocation.compute_slots == 0 || !allocation.fits(self.settings.capacity) {
            bail!("Inference allocation cannot fit the resource pool")
        }
        if self.lock()?.closed {
            bail!("Resource pool closed")
        }
        Ok(Arc::new(adapter::ScheduledBackend {
            pool: self.clone(),
            backend,
            allocation,
        }))
    }
    fn pump(&self, state: &mut State) {
        if state.closed {
            return;
        }
        // Strict FIFO: a smaller newcomer cannot starve an older large request.
        while let Some(id) = state.queue.front() {
            let entry = state
                .entries
                .get_mut(id)
                .expect("Queued resource entry exists");
            if !entry
                .allocation
                .fits(self.settings.capacity.available(state.used))
            {
                break;
            }
            state.used.add(entry.allocation);
            entry.phase = Phase::Reserved;
            entry.notify.notify_one();
            state.queue.pop_front();
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
    async fn acquire(
        self: &Arc<Self>,
        task: &str,
        allocation: Allocation,
        id: String,
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
