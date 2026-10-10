//! Jev+ routing and task lifecycle, separated from the bounded Jev-R role graph.
use crate::{
    backend::{InferenceBackend, LocalBackend},
    config::Config,
    math,
    reasoning::{Resources, Session},
    store::{self, Store},
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub task_id: String,
    pub answer: String,
    pub status: String,
    pub calls: u32,
    pub cycles: u32,
    pub elapsed_ms: f64,
    pub evidence: Vec<String>,
    pub skills: Vec<String>,
    #[serde(default)]
    pub resources: Resources,
    #[serde(default)]
    pub stop_reason: String,
}

#[derive(Clone)]
pub struct Engine {
    pub cfg: Config,
    pub store: Arc<Store>,
    backend: Arc<dyn InferenceBackend>,
}
impl Engine {
    pub fn new(cfg: Config, store: Arc<Store>) -> Self {
        let backend = Arc::new(LocalBackend(cfg.clone()));
        Self::with_backend(cfg, store, backend)
    }
    pub fn with_backend(
        cfg: Config,
        store: Arc<Store>,
        backend: Arc<dyn InferenceBackend>,
    ) -> Self {
        Self {
            cfg,
            store,
            backend,
        }
    }
    pub async fn run(&self, query: &str, mode: &str) -> Result<Outcome> {
        self.run_until(query, mode, std::future::pending()).await
    }
    /// Cancellation drops in-flight inference and persists a terminal task state.
    pub async fn run_until<F>(&self, query: &str, mode: &str, cancel: F) -> Result<Outcome>
    where
        F: std::future::Future<Output = ()>,
    {
        if query.trim().is_empty() || query.len() > 12_000 {
            bail!("Query is empty or exceeds 12,000 bytes")
        }
        if !matches!(mode, "fast" | "standard" | "deep" | "research") {
            bail!("Unknown reasoning mode")
        }
        let start = Instant::now();
        let id = self.store.create_task(query, mode)?;
        let mut session = Session::new(
            self.cfg.clone(),
            self.store.clone(),
            self.backend.clone(),
            id.clone(),
            mode,
        )?;
        self.store.event(
            &id,
            "intent",
            "interface",
            json!({"query":query,"mode":mode}),
        )?;
        let mut skills = Vec::new();
        let mut evidence = Vec::new();
        let execution = async {
            session.authorize_tool("knowledge.search")?;
            let pinned = self.store.task_context(query)?;
            skills = store::route(&pinned.skills, query);
            self.store.event(&id,"route","jev-plus",json!({"skills":skills,
                "versions":pinned.skills.iter().filter(|s|skills.contains(&s.id)).collect::<Vec<_>>(),
                "manifests":pinned.manifests.iter().filter(|m|skills.contains(&m.id)).collect::<Vec<_>>()}))?;
            let refs = pinned.documents;
            let context = refs
                .iter()
                .map(|d| {
                    format!(
                        "[{}] {}: {}",
                        d.id,
                        d.title,
                        d.body.chars().take(1200).collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            evidence.extend(refs.iter().map(|d| format!("document:{}", d.id)));
            self.store.event(
                &id,
                "knowledge",
                "memory",
                json!({"documents":refs.iter().map(|r|json!({"id":r.id,"checksum":store::sha256(r.body.as_bytes())})).collect::<Vec<_>>()}),
            )?;
            if let Some(expression) = math::extract(query) {
                session.authorize_tool("math.evaluate")?;
                return match math::calculate(expression) {
                    Ok(answer) => {
                        evidence.push("verified:arithmetic_parser".into());
                        session.stop_reason = "verified".into();
                        self.store.event(
                            &id,
                            "verification",
                            "verifier-r",
                            json!({"expression":expression,"result":answer,"verified":true}),
                        )?;
                        Ok((answer, "completed".to_string()))
                    }
                    Err(err) => {
                        session.stop_reason = "invalid_expression".into();
                        Ok((
                            format!("Arithmetic verification rejected: {err}"),
                            "rejected".into(),
                        ))
                    }
                };
            }
            let answer = session.solve(query, &context, mode).await?;
            let status = if !session.has_solution {
                "exhausted"
            } else if self.cfg.backend == "demo" {
                "demo"
            } else {
                "unverified"
            };
            Ok((answer, status.to_string()))
        };
        let result: Result<(String, String)> = tokio::select! {
            biased;
            _=cancel=>Ok(("Task cancelled".into(),"cancelled".into())),
            _=tokio::time::sleep(Duration::from_secs(self.cfg.max_wall_seconds))=>Ok(("Task wall clock budget exhausted".into(),"exhausted".into())),
            result=execution=>result,
        };
        session.finish_metrics();
        let (answer, status) = match result {
            Ok(value) => value,
            Err(err) => {
                self.store.event(
                    &id,
                    "error",
                    "system",
                    json!({"message":err.to_string().chars().take(300).collect::<String>()}),
                )?;
                session.stop_reason = "failure".into();
                (format!("Task failed: {err}"), "failed".into())
            }
        };
        if status == "cancelled" {
            session.stop_reason = "cancelled".into();
        }
        if status == "exhausted" {
            session.stop_reason = "budget_exhausted".into();
        }
        self.store.finish_task(&id, &status, &answer)?;
        self.store.event(&id,"outcome","jev-plus",json!({"status":status,"resources":session.resources,"stop_reason":session.stop_reason,"evidence":evidence}))?;
        Ok(Outcome {
            task_id: id,
            answer,
            status,
            calls: session.resources.calls,
            cycles: session.resources.cycles,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            evidence,
            skills,
            resources: session.resources,
            stop_reason: session.stop_reason,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn immediate_cancellation_is_persisted_before_tool_execution() {
        let path = std::env::temp_dir().join(format!("fabric-cancel-{}.db", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&path).unwrap());
        let engine = Engine::new(
            Config::load(None, Some("demo".into())).unwrap(),
            store.clone(),
        );
        let outcome = engine
            .run_until("calc: 6 * 7", "fast", std::future::ready(()))
            .await
            .unwrap();
        assert_eq!(outcome.status, "cancelled");
        assert_eq!(outcome.calls, 0);
        assert_eq!(
            store.task(&outcome.task_id).unwrap()["task"]["status"],
            "cancelled"
        );
        drop(engine);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn verified_math_uses_no_model_calls() {
        let path = std::env::temp_dir().join(format!("fabric-test-{}.db", uuid::Uuid::new_v4()));
        let store = Arc::new(Store::open(&path).unwrap());
        store::bootstrap(&store).unwrap();
        let engine = Engine::new(Config::load(None, Some("demo".into())).unwrap(), store);
        let result = engine.run("calc: (7 + 5) * 3", "fast").await.unwrap();
        assert_eq!(result.answer, "36");
        assert_eq!(result.calls, 0);
        let _ = std::fs::remove_file(path);
    }
}
