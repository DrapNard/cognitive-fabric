//! Typed owner-local event subscriptions with explicit loss and bounded fanout.
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::mpsc;
pub mod modules;
pub mod unix;
#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/events_generated.rs"));
}
use generated::events as fb;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Stage {
    Started,
    Routing,
    ModelDispatched,
    ReasoningStep,
    SubtaskUpdated,
    Verification,
    Finished,
    Recovery,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub sequence: u64,
    pub task_id: String,
    pub stage: Stage,
    pub source: String,
    pub state: String,
    pub role: Option<String>,
    pub subtask_id: Option<String>,
    pub model_calls: Option<u32>,
    pub tool_calls: Option<u32>,
    pub cycles: Option<u32>,
    pub verified: Option<bool>,
}
impl Progress {
    fn validate(&self) -> Result<()> {
        uuid::Uuid::parse_str(&self.task_id)?;
        if self.sequence == 0
            || !matches!(
                self.source.as_str(),
                "interface"
                    | "jev-plus"
                    | "jev-r"
                    | "verifier-r"
                    | "memory"
                    | "security-kernel"
                    | "runtime"
                    | "fabric"
                    | "human-cli"
            )
            || !matches!(
                self.state.as_str(),
                "pending"
                    | "running"
                    | "completed"
                    | "unverified"
                    | "demo"
                    | "failed"
                    | "rejected"
                    | "cancelled"
                    | "exhausted"
                    | "needs_clarification"
                    | "interrupted"
                    | "skipped"
            )
            || self.role.as_ref().is_some_and(|role| {
                !matches!(
                    role.as_str(),
                    "interface"
                        | "planner"
                        | "solver"
                        | "explorer"
                        | "critic"
                        | "reflector"
                        | "synthesizer"
                )
            })
            || self.subtask_id.as_ref().is_some_and(|id| {
                id.is_empty()
                    || id.len() > 32
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            })
        {
            bail!("Invalid typed progress event")
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let task = builder.create_string(&self.task_id);
        let source = builder.create_string(&self.source);
        let state = builder.create_string(&self.state);
        let role = self.role.as_ref().map(|role| builder.create_string(role));
        let subtask = self.subtask_id.as_ref().map(|id| builder.create_string(id));
        let event = fb::Progress::create(
            &mut builder,
            &fb::ProgressArgs {
                version: 1,
                sequence: self.sequence,
                task_id: Some(task),
                stage: fb::Stage(self.stage as u8),
                source: Some(source),
                state: Some(state),
                role,
                subtask_id: subtask,
                model_calls: self.model_calls,
                tool_calls: self.tool_calls,
                cycles: self.cycles,
                verified: self.verified,
            },
        );
        fb::finish_progress_buffer(&mut builder, event);
        let bytes = builder.finished_data().to_vec();
        if bytes.len() > 4096 {
            bail!("Progress frame exceeds limit")
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 || bytes.len() > 4096 || !fb::progress_buffer_has_identifier(bytes) {
            bail!("Invalid progress frame")
        }
        let event = fb::root_as_progress(bytes)?;
        if event.version() != 1 {
            bail!("Unsupported progress protocol version")
        }
        let stage = match event.stage().0 {
            0 => Stage::Started,
            1 => Stage::Routing,
            2 => Stage::ModelDispatched,
            3 => Stage::ReasoningStep,
            4 => Stage::SubtaskUpdated,
            5 => Stage::Verification,
            6 => Stage::Finished,
            7 => Stage::Recovery,
            _ => bail!("Unknown progress stage"),
        };
        let progress = Self {
            sequence: event.sequence(),
            task_id: event.task_id().into(),
            stage,
            source: event.source().into(),
            state: event.state().into(),
            role: event.role().map(String::from),
            subtask_id: event.subtask_id().map(String::from),
            model_calls: event.model_calls(),
            tool_calls: event.tool_calls(),
            cycles: event.cycles(),
            verified: event.verified(),
        };
        progress.validate()?;
        Ok(progress)
    }
    pub(crate) fn from_audit(
        sequence: u64,
        task: &str,
        kind: &str,
        source: &str,
        payload: &Value,
    ) -> Result<Option<Self>> {
        let stage = match kind {
            "intent" => Stage::Started,
            "route" | "route_refined" => Stage::Routing,
            "model_dispatch" => Stage::ModelDispatched,
            "reasoning" => Stage::ReasoningStep,
            "subtask" => Stage::SubtaskUpdated,
            "verification" => Stage::Verification,
            "outcome" => Stage::Finished,
            "recovery" => Stage::Recovery,
            _ => return Ok(None),
        };
        let number = |value: &Value| -> Result<Option<u32>> {
            value
                .as_u64()
                .map(|number| u32::try_from(number).map_err(Into::into))
                .transpose()
        };
        let state = if stage == Stage::SubtaskUpdated {
            payload["node"]["state"].as_str()
        } else {
            payload["status"].as_str()
        }
        .unwrap_or("running");
        let event = Self {
            sequence,
            task_id: task.into(),
            stage,
            source: source.into(),
            state: state.into(),
            role: payload["role"].as_str().map(String::from),
            subtask_id: payload["node"]["step"]["id"].as_str().map(String::from),
            model_calls: number(if stage == Stage::ModelDispatched {
                &payload["call"]
            } else {
                &payload["resources"]["calls"]
            })?,
            tool_calls: number(&payload["resources"]["tool_calls"])?,
            cycles: number(&payload["resources"]["cycles"])?,
            verified: payload["verified"].as_bool(),
        };
        event.validate()?;
        Ok(Some(event))
    }
}
struct Subscriber {
    stages: Option<Vec<Stage>>,
    task: String,
    sender: mpsc::Sender<Arc<Vec<u8>>>,
    lost: Arc<AtomicBool>,
}
#[derive(Default)]
pub(crate) struct Hub {
    modules: Mutex<modules::Registry>,
    subscribers: Mutex<Vec<Subscriber>>,
}
pub struct Subscription {
    cancelled: bool,
    receiver: mpsc::Receiver<Arc<Vec<u8>>>,
    lost: Arc<AtomicBool>,
}
impl Hub {
    pub(crate) fn subscribe(&self, task: &str) -> Result<Subscription> {
        self.subscribe_filtered(task, None)
    }
    fn subscribe_filtered(&self, task: &str, stages: Option<Vec<Stage>>) -> Result<Subscription> {
        uuid::Uuid::parse_str(task)?;
        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| anyhow::anyhow!("Event hub poisoned"))?;
        subscribers.retain(|subscriber| !subscriber.sender.is_closed());
        if subscribers.len() >= 64 {
            bail!("Event subscription capacity exhausted")
        }
        let (sender, receiver) = mpsc::channel(16);
        let lost = Arc::new(AtomicBool::new(false));
        subscribers.push(Subscriber {
            stages,
            task: task.into(),
            sender,
            lost: lost.clone(),
        });
        Ok(Subscription {
            receiver,
            lost,
            cancelled: false,
        })
    }
    pub(crate) fn publish(&self, event: &Progress) -> Result<()> {
        let bytes = Arc::new(event.encode()?);
        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| anyhow::anyhow!("Event hub poisoned"))?;
        subscribers.retain(|subscriber| {
            if subscriber.sender.is_closed() {
                return false;
            }
            if subscriber.task != event.task_id
                || subscriber
                    .stages
                    .as_ref()
                    .is_some_and(|stages| !stages.contains(&event.stage))
            {
                return true;
            }
            match subscriber.sender.try_send(bytes.clone()) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    subscriber.lost.store(true, Ordering::Release);
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            }
        });
        Ok(())
    }
}
impl Subscription {
    /// Close this observer and release queued data without cancelling its task.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.receiver.close();
        while self.receiver.try_recv().is_ok() {}
    }
    pub async fn receive_until(
        &mut self,
        cancel: impl std::future::Future<Output = ()>,
    ) -> Result<Option<Progress>> {
        tokio::select! {
            biased;
            _ = cancel => { self.cancel(); Ok(None) },
            event = self.receive() => event,
        }
    }
    pub async fn receive(&mut self) -> Result<Option<Progress>> {
        if self.cancelled {
            return Ok(None);
        }
        if self.lost.load(Ordering::Acquire) {
            bail!("Progress events lost; reload durable task history and resubscribe")
        }
        let bytes = self.receiver.recv().await;
        if self.lost.load(Ordering::Acquire) {
            bail!("Progress events lost; reload durable task history and resubscribe")
        }
        bytes.map(|bytes| Progress::decode(&bytes)).transpose()
    }
}
impl crate::store::Store {
    pub fn subscribe_progress(&self, task: &str) -> Result<Subscription> {
        crate::security::authorize("human-cli", "task.observe")?;
        uuid::Uuid::parse_str(task)?;
        let exists: bool = self.lock()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [task],
            |row| row.get(0),
        )?;
        if !exists {
            bail!("Unknown task")
        }
        self.events.subscribe(task)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProgressPage {
    pub cursor: u64,
    pub more: bool,
    pub events: Vec<Progress>,
}
impl crate::store::Store {
    /// Durable bounded catch-up also works across processes and after subscriber overflow.
    pub fn progress_since(&self, task: &str, after: u64) -> Result<ProgressPage> {
        crate::security::authorize("human-cli", "task.observe")?;
        uuid::Uuid::parse_str(task)?;
        let after = i64::try_from(after)?;
        let db = self.lock()?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [task],
            |row| row.get(0),
        )?;
        if !exists {
            bail!("Unknown task")
        }
        let rows=db.prepare("SELECT id,kind,source,CASE WHEN length(CAST(payload AS BLOB))>262144 THEN NULL ELSE payload END FROM events WHERE task_id=?1 AND id>?2 ORDER BY id LIMIT 33")?
            .query_map(rusqlite::params![task,after],|row|Ok((row.get::<_,u64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = rows.len() > 32;
        let mut cursor = after as u64;
        let mut events = Vec::new();
        for (sequence, kind, source, raw) in rows.into_iter().take(32) {
            if raw.len() > 262144 || kind.len() > 64 || source.len() > 64 {
                bail!("Stored audit event exceeds bounds")
            }
            let payload: Value = serde_json::from_str(&raw)?;
            if let Some(event) = Progress::from_audit(sequence, task, &kind, &source, &payload)? {
                events.push(event)
            }
            cursor = sequence;
        }
        Ok(ProgressPage {
            cursor,
            more,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    fn progress(task: &str, sequence: u64) -> Progress {
        Progress {
            task_id: task.into(),
            sequence,
            stage: Stage::ModelDispatched,
            source: "jev-r".into(),
            state: "running".into(),
            role: Some("solver".into()),
            subtask_id: None,
            model_calls: Some(1),
            tool_calls: None,
            cycles: None,
            verified: None,
        }
    }
    #[test]
    fn typed_frames_preserve_unknown_metrics_and_reject_malformed_inputs() {
        let task = uuid::Uuid::new_v4().to_string();
        let event = progress(&task, 1);
        let bytes = event.encode().unwrap();
        let decoded = Progress::decode(&bytes).unwrap();
        assert_eq!(decoded.model_calls, Some(1));
        assert_eq!(decoded.tool_calls, None);
        assert_eq!(decoded.verified, None);
        assert!(Progress::decode(b"garbage").is_err());
        assert!(Progress::decode(&vec![0; 4097]).is_err());
        assert!(Progress::decode(&bytes[..bytes.len() / 2]).is_err());
        let mut malformed = bytes.clone();
        malformed[..4].fill(255);
        assert!(Progress::decode(&malformed).is_err());
        let mut invalid = event.clone();
        invalid.source = "untrusted-model".into();
        assert!(invalid.encode().is_err());
        invalid = event;
        invalid.task_id = "not-a-task".into();
        assert!(invalid.encode().is_err());
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let task = builder.create_string(&task);
        let source = builder.create_string("jev-r");
        let state = builder.create_string("running");
        let root = fb::Progress::create(
            &mut builder,
            &fb::ProgressArgs {
                task_id: Some(task),
                source: Some(source),
                state: Some(state),
                sequence: 1,
                stage: fb::Stage(255),
                ..Default::default()
            },
        );
        fb::finish_progress_buffer(&mut builder, root);
        assert!(Progress::decode(builder.finished_data()).is_err());
    }
    #[tokio::test]
    async fn subscriptions_filter_tasks_bound_capacity_and_report_loss() {
        let hub = Hub::default();
        let first = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let mut subscriber = hub.subscribe(&first).unwrap();
        hub.publish(&progress(&other, 1)).unwrap();
        hub.publish(&progress(&first, 2)).unwrap();
        assert_eq!(subscriber.receive().await.unwrap().unwrap().sequence, 2);
        for sequence in 3..20 {
            hub.publish(&progress(&first, sequence)).unwrap();
        }
        assert!(subscriber
            .receive()
            .await
            .unwrap_err()
            .to_string()
            .contains("lost"));
        let mut subscriptions = (0..64)
            .map(|_| hub.subscribe(&other).unwrap())
            .collect::<Vec<_>>();
        assert!(hub.subscribe(&other).is_err());
        subscriptions.pop();
        assert!(hub.subscribe(&other).is_ok());
    }
    #[tokio::test]
    async fn durable_catchup_matches_live_events_without_exposing_audit_content() {
        let store = Store::in_memory().unwrap();
        let task = store.create_task("private goal", "fast").unwrap();
        let mut subscription = store.subscribe_progress(&task).unwrap();
        store
            .event(
                &task,
                "model_dispatch",
                "jev-r",
                serde_json::json!({"role":"solver","call":1,"prompt":"secret prompt"}),
            )
            .unwrap();
        let live = subscription.receive().await.unwrap().unwrap();
        let page = store.progress_since(&task, 0).unwrap();
        assert_eq!(page.events[0].encode().unwrap(), live.encode().unwrap());
        assert!(!serde_json::to_string(&page).unwrap().contains("secret"));
        for _ in 0..40 {
            store
                .event(
                    &task,
                    "policy",
                    "security-kernel",
                    serde_json::json!({"allowed":true}),
                )
                .unwrap();
        }
        let first = store.progress_since(&task, live.sequence).unwrap();
        assert!(first.more);
        assert!(first.events.is_empty());
        let second = store.progress_since(&task, first.cursor).unwrap();
        assert!(!second.more);
        assert!(second.cursor > first.cursor);
        assert!(store
            .subscribe_progress(&uuid::Uuid::new_v4().to_string())
            .is_err());
        assert!(crate::security::authorize("jev-r", "task.observe").is_err());
    }
    #[tokio::test]
    async fn engine_outcomes_produce_typed_progress_with_actual_counters() {
        let store = Arc::new(Store::in_memory().unwrap());
        crate::store::bootstrap(&store).unwrap();
        let engine = crate::engine::Engine::new(crate::config::Config::default(), store.clone());
        let result = engine.run("calc: 6 * 8", "fast").await.unwrap();
        let page = store.progress_since(&result.task_id, 0).unwrap();
        assert!(page
            .events
            .iter()
            .any(|event| event.stage == Stage::Started));
        assert!(page
            .events
            .iter()
            .any(|event| event.stage == Stage::Verification && event.verified == Some(true)));
        let last = page.events.last().unwrap();
        assert_eq!(last.stage, Stage::Finished);
        assert_eq!(last.state, "completed");
        assert_eq!(last.model_calls, Some(0));
        assert_eq!(last.tool_calls, Some(2));
        for event in page.events {
            assert!(Progress::decode(&event.encode().unwrap()).is_ok());
        }
        let rejected = engine.run("calc: 1 / 0", "fast").await.unwrap();
        assert_eq!(rejected.status, "rejected");
        let page = store.progress_since(&rejected.task_id, 0).unwrap();
        let last = page.events.last().unwrap();
        assert_eq!(last.stage, Stage::Finished);
        assert_eq!(last.state, "rejected");
        assert_eq!(
            Progress::decode(&last.encode().unwrap()).unwrap().state,
            "rejected"
        );
    }
}
