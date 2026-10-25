//! Shared configured-runtime ownership and conservative startup audit validation.
use super::*;
use std::sync::{OnceLock, Weak};
static POOLS: OnceLock<Mutex<BTreeMap<usize, Weak<ResourcePool>>>> = OnceLock::new();
impl ResourcePool {
    pub(crate) fn configured(
        store: Arc<Store>,
        configuration: &Configuration,
    ) -> Result<Arc<Self>> {
        Self::configured_owned(store, configuration, None)
    }
    pub(crate) fn configured_owned(
        store: Arc<Store>,
        configuration: &Configuration,
        ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
    ) -> Result<Arc<Self>> {
        if let Some(owner) = &ownership {
            owner.validate(&store)?;
        }
        configuration.validate()?;
        let mut pools = POOLS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| anyhow!("Configured resource registry poisoned"))?;
        pools.retain(|_, pool| pool.strong_count() > 0);
        let key = Arc::as_ptr(&store) as usize;
        if let Some(pool) = pools.get(&key).and_then(Weak::upgrade) {
            if !pool.matches(&store, configuration) {
                bail!("Configured resource pool settings changed while live")
            }
            return Ok(pool);
        }
        if pools.len() >= 128 {
            bail!("Configured resource pool registry full")
        }
        let lease = if ownership.is_some() {
            None
        } else {
            store
                .runtime_lock
                .as_ref()
                .map(|path| {
                    let mut resource_path = path.as_os_str().to_os_string();
                    resource_path.push(".resources");
                    crate::recovery::ExecutionLease::acquire(
                        std::path::Path::new(&resource_path),
                        true,
                    )
                })
                .transpose()?
        };
        if !Self::unresolved(&store)?.is_empty() {
            bail!("Unresolved prior resource dispatch; independently stop the provider and perform reviewed resource reconciliation before restarting")
        }
        let mut pool = Self::with_profiles(
            "human-cli",
            store,
            configuration.settings.clone(),
            configuration.residents.clone(),
            configuration.working_profiles.clone(),
            configuration.gpu_observation.clone(),
        )?;
        Arc::get_mut(&mut pool)
            .expect("Fresh pool is uniquely owned")
            ._workspace_lease = lease;
        Arc::get_mut(&mut pool)
            .expect("Fresh pool is uniquely owned")
            ._managed_ownership = ownership;
        pools.insert(key, Arc::downgrade(&pool));
        Ok(pool)
    }
    fn unresolved(store: &Store) -> Result<BTreeMap<String, (String, String)>> {
        let db = store.lock()?;
        Self::ledger(&db, None)
    }
    pub(crate) fn retention_safe(db: &rusqlite::Connection, task: &str) -> Result<bool> {
        let relevant: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE task_id=?1 AND kind IN ('resource_dispatch','resource_completed','resource_release'))", [task], |row| row.get(0))?;
        if !relevant {
            return Ok(true);
        }
        Ok(Self::ledger(db, Some(task))?.is_empty())
    }
    fn ledger(
        db: &rusqlite::Connection,
        task: Option<&str>,
    ) -> Result<BTreeMap<String, (String, String)>> {
        let mut instructions = 0;
        db.progress_handler(
            1000,
            Some(move || {
                instructions += 1;
                instructions > 30_000
            }),
        );
        let result = Self::ledger_rows(db, task);
        db.progress_handler(0, None::<fn() -> bool>);
        result
    }

    pub(crate) fn validate_resume_ledger(db: &rusqlite::Connection) -> Result<()> {
        if !Self::ledger_rows(db, None)?.is_empty() {
            bail!("Unresolved shared resource dispatch prevents continuation")
        }
        Ok(())
    }
    fn ledger_rows(
        db: &rusqlite::Connection,
        task: Option<&str>,
    ) -> Result<BTreeMap<String, (String, String)>> {
        let filter = if task.is_some() {
            "AND task_id=?1"
        } else {
            "AND ?1 IS NULL"
        };
        let mut statement=db.prepare(&format!("SELECT CASE WHEN length(CAST(task_id AS BLOB))<=36 THEN task_id END,kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload END FROM events WHERE kind IN ('resource_dispatch','resource_completed','resource_release') {filter} ORDER BY id LIMIT 4097"))?;
        let mut rows = statement.query([task])?;
        let mut records = BTreeMap::new();
        let mut completed = BTreeMap::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            if count > 4096 {
                bail!("Resource startup history exceeds bounded validation; reviewed retention is required")
            }
            let task: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let source: String = row.get(2)?;
            let bytes: String = row.get(3)?;
            uuid::Uuid::parse_str(&task)?;
            let value = crate::strict_json::value(bytes.as_bytes(), 4096)?;
            let id = value["reservation_id"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing resource reservation identity"))?;
            let pool = value["pool_id"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing resource pool identity"))?;
            uuid::Uuid::parse_str(id)?;
            uuid::Uuid::parse_str(pool)?;
            if value["format_version"] != 1 {
                bail!("Unsupported resource audit format")
            }
            match kind.as_str() {
                "resource_dispatch"
                    if source == "jev-plus" && value["provider_stop_proven"] == false =>
                {
                    if !seen.insert(id.to_string()) {
                        bail!("Duplicate resource dispatch")
                    }
                    records.insert(id.to_string(), (task, pool.to_string()));
                }
                "resource_completed"
                    if source == "jev-plus" && value["provider_request_completed"] == true =>
                {
                    if records.remove(id) != Some((task.clone(), pool.to_string())) {
                        bail!("Unmatched resource completion")
                    }
                    completed.insert(id.to_string(), (task, pool.to_string()));
                }
                "resource_release"
                    if source == "human-cli"
                        && value["operator_acknowledged_stop"] == true
                        && value["provider_stop_proven"] == false =>
                {
                    if records.remove(id).or_else(|| completed.remove(id))
                        != Some((task, pool.to_string()))
                    {
                        bail!("Unmatched resource release")
                    }
                }
                _ => bail!("Invalid resource lifecycle audit"),
            }
        }
        Ok(records)
    }

    /// Read sampled audit state without claiming provider health or stop proof.
    pub fn abandoned_status(actor: &str, store: &Store) -> Result<serde_json::Value> {
        security::authorize(actor, "resources.resolve")?;
        let records = Self::unresolved(store)?;
        Ok(
            json!({"format_version":1,"unresolved":records.into_iter().map(|(id,(task,pool))|
            json!({"reservation_id":id,"task_id":task,"pool_id":pool})).collect::<Vec<_>>(),
            "sampled_only":true,"provider_stop_proven":false,"mutation_performed":false}),
        )
    }

    /// Release an exact abandoned reservation after external operator review.
    /// Both leases exclude active configured hosts and ordinary task execution.
    pub fn release_abandoned(actor: &str, store: &Store, id: &str, confirmed: bool) -> Result<()> {
        security::authorize(actor, "resources.resolve")?;
        if !confirmed {
            bail!("Resource reconciliation requires explicit provider-stop acknowledgement")
        }
        uuid::Uuid::parse_str(id)?;
        let path = store
            .runtime_lock
            .as_ref()
            .ok_or_else(|| anyhow!("Resource reconciliation requires a persistent workspace"))?;
        let mut resource_path = path.as_os_str().to_os_string();
        resource_path.push(".resources");
        let _resources =
            crate::recovery::ExecutionLease::acquire(std::path::Path::new(&resource_path), true)?;
        let _runtime = crate::recovery::ExecutionLease::acquire(path, true)?;
        let mut unresolved = Self::unresolved(store)?;
        let (task, pool) = unresolved
            .remove(id)
            .ok_or_else(|| anyhow!("Unknown abandoned resource reservation"))?;
        store.event(&task,"resource_release","human-cli",json!({"format_version":1,"pool_id":pool,
            "reservation_id":id,"operator_acknowledged_stop":true,"provider_stop_proven":false,"abandoned_reconciliation":true}))?;
        Ok(())
    }
}

pub(crate) struct RestorationProbe {
    _pool: Option<Arc<ResourcePool>>,
    _lease: Option<crate::recovery::ExecutionLease>,
}
impl ResourcePool {
    /// Called outside Store locks after a read-only checkpoint validation.
    pub(crate) fn probe_restoration(
        store: &Store,
        task: &str,
        configuration: &Configuration,
    ) -> Result<RestorationProbe> {
        configuration.validate()?;
        let cached = {
            let pools = POOLS
                .get_or_init(|| Mutex::new(BTreeMap::new()))
                .lock()
                .map_err(|_| anyhow!("Configured resource registry poisoned"))?;
            pools
                .get(&(store as *const Store as usize))
                .and_then(Weak::upgrade)
        };
        if let Some(pool) = cached {
            if !pool.matches_ref(store, configuration) {
                bail!("Original resource configuration differs from the live pool")
            }
            let snapshot = pool.snapshot()?;
            if snapshot.closed
                || snapshot
                    .reservations
                    .iter()
                    .any(|entry| entry.task_id == task)
            {
                bail!("Original task has a closed or unresolved live resource pool")
            }
            return Ok(RestorationProbe {
                _pool: Some(pool),
                _lease: None,
            });
        }
        let path = store
            .runtime_lock
            .as_ref()
            .ok_or_else(|| anyhow!("Resource restoration requires persistent ownership"))?;
        let mut resource_path = path.as_os_str().to_os_string();
        resource_path.push(".resources");
        let lease =
            crate::recovery::ExecutionLease::acquire(std::path::Path::new(&resource_path), true)?;
        Ok(RestorationProbe {
            _pool: None,
            _lease: Some(lease),
        })
    }
}
