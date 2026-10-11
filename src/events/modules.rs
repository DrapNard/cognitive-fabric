//! Host-owned, versioned routing of content-minimized task milestones.
use super::{Progress, Stage, Subscription};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct EventModule {
    pub id: String,
    pub version: String,
    pub stages: Vec<Stage>,
}
impl EventModule {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || self.version.len() > 128
            || self.stages.is_empty()
            || self.stages.len() > 8
        {
            bail!("Invalid event module descriptor")
        }
        semver::Version::parse(&self.version)?;
        if self.stages.iter().collect::<BTreeSet<_>>().len() != self.stages.len() {
            bail!("Duplicate event module stage")
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Registry {
    active: BTreeMap<String, EventModule>,
    seen: BTreeSet<(String, String)>,
}

pub struct ModuleSubscription {
    descriptor: EventModule,
    subscription: Subscription,
}
impl ModuleSubscription {
    pub fn descriptor(&self) -> &EventModule {
        &self.descriptor
    }
    pub async fn receive(&mut self) -> Result<Option<Progress>> {
        self.subscription.receive().await
    }
}
impl crate::store::Store {
    /// Trusted embedding-host API. Descriptors are not accepted from model output or IPC.
    pub fn register_event_module(&self, descriptor: EventModule) -> Result<()> {
        descriptor.validate()?;
        let mut registry = self
            .events
            .modules
            .lock()
            .map_err(|_| anyhow::anyhow!("Event registry poisoned"))?;
        if registry.seen.len() >= 128
            || registry
                .seen
                .contains(&(descriptor.id.clone(), descriptor.version.clone()))
        {
            bail!("Event module version is immutable or registry capacity exhausted")
        }
        if !registry.active.contains_key(&descriptor.id) && registry.active.len() >= 32 {
            bail!("Active event module capacity exhausted")
        }
        registry
            .seen
            .insert((descriptor.id.clone(), descriptor.version.clone()));
        registry.active.insert(descriptor.id.clone(), descriptor);
        Ok(())
    }
    pub fn unregister_event_module(&self, id: &str) -> Result<()> {
        let mut registry = self
            .events
            .modules
            .lock()
            .map_err(|_| anyhow::anyhow!("Event registry poisoned"))?;
        if registry.active.remove(id).is_none() {
            bail!("Unknown event module")
        }
        Ok(())
    }
    pub fn discover_event_modules(&self) -> Result<Vec<EventModule>> {
        let registry = self
            .events
            .modules
            .lock()
            .map_err(|_| anyhow::anyhow!("Event registry poisoned"))?;
        Ok(registry.active.values().cloned().collect())
    }
    pub fn subscribe_module_progress(&self, task: &str, id: &str) -> Result<ModuleSubscription> {
        crate::security::authorize("human-cli", "task.observe")?;
        uuid::Uuid::parse_str(task)?;
        let exists: bool = self.lock()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [task],
            |r| r.get(0),
        )?;
        if !exists {
            bail!("Unknown task")
        }
        let registry = self
            .events
            .modules
            .lock()
            .map_err(|_| anyhow::anyhow!("Event registry poisoned"))?;
        let descriptor = registry
            .active
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Unknown active event module"))?
            .clone();
        let subscription = self
            .events
            .subscribe_filtered(task, Some(descriptor.stages.clone()))?;
        Ok(ModuleSubscription {
            descriptor,
            subscription,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(version: &str, stage: Stage) -> EventModule {
        EventModule {
            id: "scheduler.local".into(),
            version: version.into(),
            stages: vec![stage],
        }
    }
    #[tokio::test]
    async fn routes_persisted_events_by_task_and_pinned_module_version() {
        let store = crate::store::Store::in_memory().unwrap();
        let task = store.create_task("private objective", "fast").unwrap();
        let other = store.create_task("other objective", "fast").unwrap();
        store
            .register_event_module(descriptor("1.0.0", Stage::Finished))
            .unwrap();
        let mut old = store
            .subscribe_module_progress(&task, "scheduler.local")
            .unwrap();
        store
            .register_event_module(descriptor("2.0.0", Stage::ModelDispatched))
            .unwrap();
        let mut current = store
            .subscribe_module_progress(&task, "scheduler.local")
            .unwrap();
        assert_eq!(old.descriptor().version, "1.0.0");
        assert_eq!(current.descriptor().version, "2.0.0");
        for _ in 0..20 {
            store
                .event(
                    &other,
                    "outcome",
                    "runtime",
                    serde_json::json!({"status":"completed"}),
                )
                .unwrap();
        }
        for _ in 0..20 {
            store
                .event(&task, "route", "jev-plus", serde_json::json!({}))
                .unwrap();
        }
        store
            .event(
                &task,
                "model_dispatch",
                "jev-r",
                serde_json::json!({"role":"solver","call":1,"prompt":"private prompt"}),
            )
            .unwrap();
        let event = current.receive().await.unwrap().unwrap();
        assert_eq!(event.stage, Stage::ModelDispatched);
        assert_eq!(event.task_id, task);
        assert!(!serde_json::to_string(&event).unwrap().contains("private"));
        store.unregister_event_module("scheduler.local").unwrap();
        assert!(store
            .subscribe_module_progress(&task, "scheduler.local")
            .is_err());
        store
            .event(
                &task,
                "outcome",
                "runtime",
                serde_json::json!({"status":"unverified"}),
            )
            .unwrap();
        assert_eq!(old.receive().await.unwrap().unwrap().stage, Stage::Finished);
        assert!(store.discover_event_modules().unwrap().is_empty());
        assert!(store
            .register_event_module(descriptor("1.0.0", Stage::Finished))
            .is_err());
        assert!(store
            .subscribe_module_progress(&uuid::Uuid::new_v4().to_string(), "scheduler.local")
            .is_err());
    }
    #[tokio::test]
    async fn filters_before_queue_admission_and_reports_module_overflow() {
        let store = crate::store::Store::in_memory().unwrap();
        let task = store.create_task("bounded", "fast").unwrap();
        let mut invalid = descriptor("1.0.0", Stage::Finished);
        invalid.stages.push(Stage::Finished);
        assert!(store.register_event_module(invalid).is_err());
        let mut invalid = descriptor("not-semver", Stage::Finished);
        assert!(store.register_event_module(invalid.clone()).is_err());
        invalid.version = "1.0.0".into();
        invalid.id = "../bad".into();
        assert!(store.register_event_module(invalid).is_err());
        store
            .register_event_module(descriptor("1.0.0", Stage::Finished))
            .unwrap();
        let mut subscription = store
            .subscribe_module_progress(&task, "scheduler.local")
            .unwrap();
        for _ in 0..17 {
            store
                .event(
                    &task,
                    "outcome",
                    "runtime",
                    serde_json::json!({"status":"completed"}),
                )
                .unwrap();
        }
        assert!(subscription
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("lost"));
        assert_eq!(store.progress_since(&task, 0).unwrap().events.len(), 17);
        let mut subscriptions = Vec::new();
        for _ in 0..64 {
            subscriptions.push(
                store
                    .subscribe_module_progress(&task, "scheduler.local")
                    .unwrap(),
            );
        }
        assert!(store
            .subscribe_module_progress(&task, "scheduler.local")
            .is_err());
        subscriptions.pop();
        assert!(store
            .subscribe_module_progress(&task, "scheduler.local")
            .is_ok());
    }
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    #[test]
    fn limits_active_modules_and_retains_immutable_version_history() {
        let store = crate::store::Store::in_memory().unwrap();
        for index in 0..32 {
            store
                .register_event_module(EventModule {
                    id: format!("module.{index}"),
                    version: "1.0.0".into(),
                    stages: vec![Stage::Finished],
                })
                .unwrap();
        }
        assert!(store
            .register_event_module(EventModule {
                id: "overflow".into(),
                version: "1.0.0".into(),
                stages: vec![Stage::Finished]
            })
            .is_err());
        for version in 2..=97 {
            store
                .register_event_module(EventModule {
                    id: "module.0".into(),
                    version: format!("{version}.0.0"),
                    stages: vec![Stage::Started],
                })
                .unwrap();
        }
        assert_eq!(store.discover_event_modules().unwrap().len(), 32);
        store.unregister_event_module("module.0").unwrap();
        assert!(store
            .register_event_module(EventModule {
                id: "replacement".into(),
                version: "1.0.0".into(),
                stages: vec![Stage::Finished]
            })
            .is_err());
    }
}
