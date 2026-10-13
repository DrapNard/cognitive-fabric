use super::*;
use crate::{
    config::Config,
    engine::{Engine, ExecutionProfile},
    store::Store,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Fixed {
    version: String,
    selected: Vec<String>,
    calls: AtomicUsize,
    store: Option<Arc<Store>>,
    disable: AtomicBool,
}
impl ExpertRouter for Fixed {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "test.router".into(),
            version: self.version.clone(),
        }
    }
    fn select(&self, request: Request<'_>) -> Result<Vec<String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.max_routes, 2);
        if let Some(store) = &self.store {
            let store = store.clone();
            let disable = self.disable.swap(false, Ordering::SeqCst);
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = (|| {
                    let _ = store.skills()?;
                    if disable {
                        crate::registry::disable(&store, "rust.s1")?;
                    }
                    Ok::<_, anyhow::Error>(())
                })();
                let _ = send.send(result);
            });
            receive
                .recv_timeout(std::time::Duration::from_secs(2))
                .map_err(|_| anyhow::anyhow!("Router callback observed a held database lock"))??;
        }
        Ok(self.selected.clone())
    }
}
fn policy(version: &str, selected: &[&str]) -> Arc<Fixed> {
    Arc::new(Fixed {
        version: version.into(),
        selected: selected.iter().map(|s| (*s).into()).collect(),
        calls: AtomicUsize::new(0),
        store: None,
        disable: AtomicBool::new(false),
    })
}
fn engine() -> Engine {
    let store = Arc::new(Store::in_memory().unwrap());
    store::bootstrap(&store).unwrap();
    Engine::new(
        Config {
            max_tool_calls: 1,
            ..Default::default()
        },
        store,
    )
}
#[test]
fn routing_identity_and_decision_bounds_fail_closed() {
    let store = Store::in_memory().unwrap();
    store::bootstrap(&store).unwrap();
    let mut skills = store.skills().unwrap();
    skills
        .iter_mut()
        .find(|s| s.id == "linux.s1")
        .unwrap()
        .enabled = false;
    for selected in [
        &["unknown.s1"][..],
        &["linux.s1"][..],
        &["rust.s1", "rust.s1"][..],
        &["rust.s1", "git.s1", "trainer.s1"][..],
    ] {
        let p = policy("1.0.0", selected);
        let handle = Handle::new(p).unwrap();
        assert!(handle.select("Rust", &skills).is_err());
    }
    for version in ["", "v1.0.0", "01.0.0", "1.0", "1.0.0+"] {
        assert!(Handle::new(policy(version, &[])).is_err());
    }
    let p = policy("1.0.0", &[]);
    let handle = Handle::new(p.clone()).unwrap();
    assert!(handle.select("", &skills).is_err());
    assert!(handle.select(&"x".repeat(12001), &skills).is_err());
    assert!(handle
        .select("Rust", &vec![skills[0].clone(); MAX_ROUTING_SKILLS + 1])
        .is_err());
    assert_eq!(p.calls.load(Ordering::SeqCst), 0);
    let default = Handle::default();
    assert_eq!(default.select("Rust", &skills).unwrap(), vec!["rust.s1"]);
}
#[tokio::test]
async fn router_reentry_is_lock_free_and_decisions_use_the_original_skill_snapshot() {
    let engine = engine();
    let p = Arc::new(Fixed {
        version: "1.0.0".into(),
        selected: vec!["rust.s1".into()],
        calls: AtomicUsize::new(0),
        store: Some(engine.store.clone()),
        disable: AtomicBool::new(true),
    });
    let engine = engine.with_expert_router(p.clone()).unwrap();
    let first = engine.run("calc:6*7", "fast").await.unwrap();
    assert_eq!(first.status, "completed");
    assert_eq!(first.answer, "42");
    assert_eq!(first.calls, 0);
    let trace = engine.store.task(&first.task_id).unwrap();
    let route = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "route")
        .unwrap();
    assert_eq!(route["payload"]["skills"], json!(["rust.s1"]));
    assert_eq!(route["payload"]["versions"][0]["enabled"], true);
    assert_eq!(route["payload"]["router"]["version"], "1.0.0");
    assert!(
        !engine
            .store
            .skills()
            .unwrap()
            .iter()
            .find(|s| s.id == "rust.s1")
            .unwrap()
            .enabled
    );
    let next = engine.run("calc:6*7", "fast").await.unwrap();
    assert_eq!(next.status, "failed");
    assert_eq!(next.resources.tool_calls, 0);
    assert_eq!(p.calls.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn route_audit_failure_prevents_callback_and_tool_dispatch() {
    let engine = engine();
    let p = policy("1.0.0", &["rust.s1"]);
    engine.store.lock().unwrap().execute_batch("CREATE TRIGGER fail_route_policy BEFORE INSERT ON events WHEN NEW.kind='routing_policy' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    let engine = engine.with_expert_router(p.clone()).unwrap();
    let out = engine.run("calc:6*7", "fast").await.unwrap();
    assert_eq!(out.status, "failed");
    assert_eq!(out.calls, 0);
    assert_eq!(out.resources.tool_calls, 0);
    assert_eq!(p.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn engine_policy_replacement_preserves_old_clones_and_ablation_exclusions() {
    let old = engine()
        .with_expert_router(policy("1.0.0", &["rust.s1"]))
        .unwrap();
    let p = policy("2.0.0", &["linux.s1"]);
    let new = old.clone().with_expert_router(p.clone()).unwrap();
    for (engine, id, version) in [(&old, "rust.s1", "1.0.0"), (&new, "linux.s1", "2.0.0")] {
        let out = engine.run("calc:6*7", "fast").await.unwrap();
        let trace = engine.store.task(&out.task_id).unwrap();
        let route = trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "route")
            .unwrap();
        assert_eq!(route["payload"]["skills"], json!([id]));
        assert_eq!(route["payload"]["router"]["version"], version);
    }
    let count = p.calls.load(Ordering::SeqCst);
    let out = new
        .run_profile("calc:6*7", "fast", ExecutionProfile::Tools)
        .await
        .unwrap();
    assert_eq!(out.answer, "42");
    assert_eq!(p.calls.load(Ordering::SeqCst), count);
    let trace = new.store.task(&out.task_id).unwrap();
    assert!(trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["kind"] != "routing_policy"));
    assert_eq!(
        trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "route")
            .unwrap()["payload"]["router_invoked"],
        false
    );
}
