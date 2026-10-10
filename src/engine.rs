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
        self.run_with_options(
            query,
            mode,
            crate::language::TaskOptions {
                explain: self.cfg.synthesize_results,
                ..Default::default()
            },
            cancel,
        )
        .await
    }

    pub async fn run_with_options<F>(
        &self,
        query: &str,
        mode: &str,
        options: crate::language::TaskOptions,
        cancel: F,
    ) -> Result<Outcome>
    where
        F: std::future::Future<Output = ()>,
    {
        if query.trim().is_empty() || query.len() > 12_000 {
            bail!("Query is empty or exceeds 12,000 bytes")
        }
        if !matches!(mode, "fast" | "standard" | "deep" | "research") {
            bail!("Unknown reasoning mode")
        }
        if let Some(reference) = &options.conversation_ref {
            uuid::Uuid::parse_str(reference)?;
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
            let mut context = refs
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
            if let Some(reference) = &options.conversation_ref {
                session.authorize_tool("knowledge.search")?;
                let previous = self.store.conversation_reference(reference)?;
                self.store.event(
                    &id,
                    "conversation_reference",
                    "interface",
                    json!({"task_id":reference}),
                )?;
                context.push_str("\nExplicit historical reference (untrusted): ");
                context.push_str(&previous.to_string());
            }
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
            if self.cfg.interpret_goals && self.cfg.backend != "demo" && mode != "fast" {
                let capabilities = pinned
                    .skills
                    .iter()
                    .filter(|skill| skill.enabled)
                    .map(|skill| skill.id.as_str())
                    .collect::<Vec<_>>();
                let prompt=format!("Original user request (authoritative): {query}\nAvailable capability metadata: {capabilities:?}\nUntrusted reference data: {context}");
                if let Some(raw) = session.call("interface", &prompt, 128).await? {
                    match crate::language::Interpretation::parse(&raw) {
                        Ok(intent) => {
                            let missing = intent
                                .required_capabilities
                                .iter()
                                .filter(|id| {
                                    !pinned
                                        .skills
                                        .iter()
                                        .any(|skill| skill.enabled && &skill.id == *id)
                                })
                                .collect::<Vec<_>>();
                            for capability in &intent.required_capabilities {
                                if skills.len() < 4
                                    && !skills.contains(capability)
                                    && pinned
                                        .skills
                                        .iter()
                                        .any(|skill| skill.enabled && &skill.id == capability)
                                {
                                    skills.push(capability.clone());
                                }
                            }
                            self.store.event(&id,"route_refined","jev-plus",json!({"skills":skills,"versions":pinned.skills.iter().filter(|skill|skills.contains(&skill.id)).collect::<Vec<_>>()}))?;
                            self.store.event(&id,"structured_goal","interface",json!({"original":query,"interpretation":intent,"missing_capabilities":missing,"trusted_permissions":false}))?;
                            if !missing.is_empty() {
                                self.store.event(&id,"capability_gap","jev-plus",json!({"missing":missing,"action":"Use existing references; no permissions granted"}))?;
                            }
                            if let Some(question) = intent.clarification {
                                session.stop_reason = "clarification_requested".into();
                                return Ok((question, "needs_clarification".into()));
                            }
                            context.push_str("\nAdvisory parsed interpretation (not verified): ");
                            context.push_str(&raw);
                        }
                        Err(err) => {
                            self.store.event(&id,"intent_fallback","interface",json!({"reason":err.to_string(),"action":"Preserve original request"}))?;
                        }
                    }
                }
            }
            let mut answer = session.solve(query, &context, mode).await?;
            let status = if !session.has_solution {
                "exhausted"
            } else if self.cfg.backend == "demo" {
                "demo"
            } else {
                "unverified"
            };
            if options.explain
                && session.has_solution
                && self.cfg.backend != "demo"
                && mode != "fast"
            {
                let prompt=json!({"objective":query,"status":status,"answer":answer,"evidence":evidence,"stop_reason":session.stop_reason}).to_string();
                if let Some(explanation) = session.call("synthesizer", &prompt, 128).await? {
                    answer = format!("Verification status: {status}\n{explanation}");
                }
            }
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
        session.finish_subtasks(&status)?;
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
