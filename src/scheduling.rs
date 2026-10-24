//! Operator-trusted dependency scheduling with persisted, validated decisions.
use crate::planning::Plan;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub id: String,
    pub version: String,
}
impl Descriptor {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.id.len() > 64
            || !crate::registry::identifier(&self.id)
            || self.version.len() > 64
            || semver::Version::parse(&self.version)?.to_string() != self.version
        {
            bail!("Invalid subtask scheduler identity/version")
        }
        Ok(())
    }
}
/// Short nonblocking metadata decisions only; native callbacks are not sandboxed.
/// Scheduling cannot change steps, dependencies, budgets or permissions.
pub trait SubtaskScheduler: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    fn order(&self, plan: &Plan) -> Result<Vec<usize>>;
}
/// Operator-selected built-in ordering. Omission preserves legacy stable order.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Stable,
    CriticalPath,
}

/// Prioritize ready steps by longest downstream chain, then canonical step ID.
/// Unit edge costs are a graph heuristic, not inferred runtime or utility.
pub struct CriticalPathScheduler;
impl SubtaskScheduler for CriticalPathScheduler {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "jev-plus.critical-path".into(),
            version: "1.0.0".into(),
        }
    }
    fn order(&self, plan: &Plan) -> Result<Vec<usize>> {
        Plan::parse(&serde_json::to_string(plan)?)?;
        let topological = plan.order()?;
        let mut height = vec![1usize; plan.steps.len()];
        for &index in topological.iter().rev() {
            for (child, step) in plan.steps.iter().enumerate() {
                if step.depends_on.contains(&plan.steps[index].id) {
                    height[index] = height[index].max(1 + height[child]);
                }
            }
        }
        let mut completed = BTreeSet::new();
        let mut order = Vec::with_capacity(plan.steps.len());
        while order.len() < plan.steps.len() {
            let (index, step) = plan
                .steps
                .iter()
                .enumerate()
                .filter(|(_, step)| {
                    !completed.contains(&step.id)
                        && step.depends_on.iter().all(|id| completed.contains(id))
                })
                .min_by(|(a, sa), (b, sb)| {
                    height[*b].cmp(&height[*a]).then_with(|| sa.id.cmp(&sb.id))
                })
                .ok_or_else(|| anyhow::anyhow!("Subtask dependency cycle"))?;
            completed.insert(step.id.clone());
            order.push(index);
        }
        validate_order(plan, &order)?;
        Ok(order)
    }
}

pub struct StableScheduler;
impl SubtaskScheduler for StableScheduler {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "jev-plus.stable".into(),
            version: "1.0.0".into(),
        }
    }
    fn order(&self, plan: &Plan) -> Result<Vec<usize>> {
        plan.order()
    }
}
pub(crate) fn validate_order(plan: &Plan, order: &[usize]) -> Result<()> {
    // Validate public struct callers too, before invoking any native callback.
    Plan::parse(&serde_json::to_string(plan)?)?;
    let mut completed = BTreeSet::new();
    if order.len() != plan.steps.len() {
        bail!("Scheduler must select every subtask exactly once")
    }
    for index in order {
        let step = plan
            .steps
            .get(*index)
            .ok_or_else(|| anyhow::anyhow!("Unknown scheduled subtask"))?;
        if !step.depends_on.iter().all(|id| completed.contains(id))
            || !completed.insert(step.id.clone())
        {
            bail!("Scheduler returned duplicate or dependency-violating subtasks")
        }
    }
    Ok(())
}
#[derive(Clone)]
pub(crate) struct Handle {
    backend: Arc<dyn SubtaskScheduler>,
    descriptor: Descriptor,
}
impl Default for Handle {
    fn default() -> Self {
        Self {
            backend: Arc::new(StableScheduler),
            descriptor: StableScheduler.descriptor(),
        }
    }
}
impl Handle {
    pub(crate) fn configured(policy: Option<Policy>) -> Self {
        match policy {
            None | Some(Policy::Stable) => Self::default(),
            Some(Policy::CriticalPath) => Self {
                backend: Arc::new(CriticalPathScheduler),
                descriptor: CriticalPathScheduler.descriptor(),
            },
        }
    }
    pub(crate) fn new(backend: Arc<dyn SubtaskScheduler>) -> Result<Self> {
        let descriptor = backend.descriptor();
        descriptor.validate()?;
        Ok(Self {
            backend,
            descriptor,
        })
    }
    pub(crate) fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub(crate) fn order(&self, plan: &Plan) -> Result<Vec<usize>> {
        Plan::parse(&serde_json::to_string(plan)?)?;
        let order = self.backend.order(plan)?;
        validate_order(plan, &order)?;
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::{Generation, InferenceBackend},
        config::Config,
        engine::Engine,
        store::Store,
    };
    use std::{future::Future, pin::Pin, sync::Mutex};
    const PLAN: &str = r#"{"steps":[{"id":"a","objective":"First independent subgoal","depends_on":[]},{"id":"b","objective":"Second independent subgoal","depends_on":[]}]}"#;
    struct Policy(Vec<usize>);
    impl SubtaskScheduler for Policy {
        fn descriptor(&self) -> Descriptor {
            Descriptor {
                id: "fixture.scheduler".into(),
                version: "1.0.0".into(),
            }
        }
        fn order(&self, _: &Plan) -> Result<Vec<usize>> {
            Ok(self.0.clone())
        }
    }
    #[test]
    fn rejects_missing_duplicate_unknown_and_dependency_violating_selections() {
        let plan = Plan::parse(PLAN).unwrap();
        for order in [vec![], vec![0], vec![0, 0], vec![0, 2], vec![0, 1, 2]] {
            assert!(Handle::new(Arc::new(Policy(order)))
                .unwrap()
                .order(&plan)
                .is_err());
        }
        let plan = Plan::parse(r#"{"steps":[{"id":"a","objective":"A","depends_on":[]},{"id":"b","objective":"B","depends_on":["a"]}]}"#).unwrap();
        assert!(validate_order(&plan, &[1, 0]).is_err());
        validate_order(&plan, &[0, 1]).unwrap();
        for (id, version) in [
            ("invalid/id", "1.0.0"),
            ("valid.id", "not-semver"),
            ("valid.id", "01.0.0"),
        ] {
            assert!(Descriptor {
                id: id.into(),
                version: version.into()
            }
            .validate()
            .is_err());
        }
    }
    struct Script(Mutex<Vec<&'static str>>);
    impl InferenceBackend for Script {
        fn generate<'a>(
            &'a self,
            _role: &'a str,
            prompt: &'a str,
            _allocation: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
            Box::pin(async move {
                let mut script = self.0.lock().unwrap();
                let text = script.remove(0);
                if text == "B result" {
                    assert!(prompt.contains("Second independent subgoal"));
                }
                if text == "A result" {
                    assert!(prompt.contains("First independent subgoal"));
                }
                Ok(Generation {
                    text: text.into(),
                    model: "fixture/v1".into(),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    elapsed_ms: 1.0,
                    queue_wait_ms: None,
                    provider_metrics: None,
                })
            })
        }
    }
    #[tokio::test]
    async fn custom_order_executes_and_recovery_reads_committed_order_without_callback() {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(Script(Mutex::new(vec![
            PLAN,
            "B result",
            "A result",
            "Final answer",
        ])));
        let base = Engine::with_backend(
            Config {
                backend: "openai-local".into(),
                interpret_goals: false,
                max_calls: 4,
                ..Config::default()
            },
            store.clone(),
            backend.clone(),
        );
        let original = base.clone();
        let engine = base
            .with_subtask_scheduler(Arc::new(Policy(vec![1, 0])))
            .unwrap();
        assert_eq!(
            original.subtask_scheduler_descriptor().id,
            "jev-plus.stable"
        );
        let outcome = engine
            .run("Solve two independent subgoals", "deep")
            .await
            .unwrap();
        assert_eq!(outcome.answer, "Final answer");
        assert_eq!(outcome.calls, 4);
        assert!(backend.0.lock().unwrap().is_empty());
        assert!(store.graph_audit(&outcome.task_id).is_ok());
        let plan = Plan::parse(PLAN).unwrap();
        {
            let db = store.lock().unwrap();
            assert_eq!(
                crate::recovery::graph::order_from(&db, &outcome.task_id, &plan).unwrap(),
                vec![1, 0]
            );
        }
        let trace = store.task(&outcome.task_id).unwrap();
        for mutation in [
            "UPDATE events SET payload=json_set(payload,'$.order',json('[0,0]')) WHERE kind='subtask_schedule'",
            "UPDATE events SET payload=json_set(payload,'$.order',json('[0,1]')) WHERE kind='subtask_schedule'",
            "UPDATE events SET payload=json_set(payload,'$.graph_sha256','wrong') WHERE kind='subtask_schedule'",
            "UPDATE events SET payload=json_set(payload,'$.scheduler.version','2.0.0') WHERE kind='subtask_schedule'",
            "UPDATE events SET source='forged' WHERE kind='subtask_schedule'",
            "DELETE FROM events WHERE kind='subtask_schedule'",
            "DELETE FROM events WHERE kind='subtask_schedule_intent'",
            "INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE kind='subtask_schedule'",
        ] {
            let copied = Store::in_memory().unwrap();
            let task = copied.create_task("fixture", "deep").unwrap();
            for event in trace["events"].as_array().unwrap().iter().filter(|event| matches!(event["kind"].as_str(),Some("plan"|"subtask"|"subtask_schedule"|"subtask_schedule_intent"))) {
                copied.event(&task,event["kind"].as_str().unwrap(),event["source"].as_str().unwrap(),event["payload"].clone()).unwrap();
            }
            assert!(copied.graph_audit(&task).is_ok());
            copied.lock().unwrap().execute_batch(mutation).unwrap();
            assert!(copied.graph_audit(&task).is_err(), "{mutation}");
        }
    }
    #[tokio::test]
    async fn invalid_custom_order_never_dispatches_subtasks() {
        let store = Arc::new(Store::in_memory().unwrap());
        let backend = Arc::new(Script(Mutex::new(vec![PLAN])));
        let engine = Engine::with_backend(
            Config {
                backend: "openai-local".into(),
                interpret_goals: false,
                ..Config::default()
            },
            store.clone(),
            backend.clone(),
        )
        .with_subtask_scheduler(Arc::new(Policy(vec![])))
        .unwrap();
        let outcome = engine.run("Solve subgoals", "deep").await.unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.calls, 1);
        assert!(backend.0.lock().unwrap().is_empty());
        assert!(!store.task(&outcome.task_id).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "subtask" || e["kind"] == "subtask_schedule"));
    }

    #[tokio::test]
    async fn audit_failure_prevents_callback_and_subtask_dispatch() {
        struct Never;
        impl SubtaskScheduler for Never {
            fn descriptor(&self) -> Descriptor {
                StableScheduler.descriptor()
            }
            fn order(&self, _: &Plan) -> Result<Vec<usize>> {
                panic!("Unaudited callback")
            }
        }
        let store = Arc::new(Store::in_memory().unwrap());
        store.lock().unwrap().execute_batch("CREATE TRIGGER refuse_schedule BEFORE INSERT ON events WHEN NEW.kind='subtask_schedule_intent' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
        let backend = Arc::new(Script(Mutex::new(vec![PLAN])));
        let engine = Engine::with_backend(
            Config {
                backend: "openai-local".into(),
                interpret_goals: false,
                ..Config::default()
            },
            store.clone(),
            backend.clone(),
        )
        .with_subtask_scheduler(Arc::new(Never))
        .unwrap();
        let outcome = engine.run("Solve subgoals", "deep").await.unwrap();
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.calls, 1);
        assert!(backend.0.lock().unwrap().is_empty());
    }
}

#[cfg(test)]
#[path = "scheduling/critical_path_tests.rs"]
mod critical_path_tests;
