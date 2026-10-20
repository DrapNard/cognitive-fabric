use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    escalation::{Decision, Descriptor, EscalationPolicy, Request},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const PORTFOLIO: &str = r#"{"alternatives":[{"id":"b","strategy":"Use insertion sort","limitations":"Quadratic cost"},{"id":"a","strategy":"Use merge sort","limitations":"Needs scratch space"}]}"#;
struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);
impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self.0.lock().unwrap().pop_front().unwrap();
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
            })
        })
    }
}
struct Allow;
impl EscalationPolicy for Allow {
    fn descriptor(&self) -> Descriptor {
        Descriptor {
            id: "fixture.portfolio".into(),
            version: "1.0.0".into(),
        }
    }
    fn decide(&self, _: &Request) -> Result<Decision> {
        Ok(Decision {
            allow: true,
            reason_code: "fixture_allow".into(),
        })
    }
}
fn config(endpoint: String) -> Config {
    Config {
        home: std::env::temp_dir().join(format!(
            "fabric-strategy-portfolio-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        api_base: endpoint,
        interpret_goals: false,
        max_calls: 18,
        max_tokens: 4000,
        max_tool_calls: 8,
        ..Config::default()
    }
}
async fn http(listener: tokio::net::TcpListener, prose: bool) {
    for (i, answer) in if prose {
        vec![
            "second answer",
            r#"{"revise":false,"reason":"No useful revision"}"#,
        ]
    } else {
        vec!["[2,3,1]", "[1,2,3]"]
    }
    .into_iter()
    .enumerate()
    {
        let (mut stream, _) =
            tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
        let mut bytes = Vec::new();
        let request = loop {
            let mut chunk = [0u8; 4096];
            let n =
                tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut chunk))
                    .await
                    .unwrap()
                    .unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 64000);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                }
            }
        };
        let prompt = request["messages"][1]["content"].as_str().unwrap();
        if i == 0 {
            assert!(prompt.contains("Use insertion sort"));
            assert!(!prompt.contains("Use merge sort"));
            assert_eq!(prompt.matches("Selected unverified approach").count(), 1);
            if prose {
                assert!(prompt.contains("New reflection"));
            }
        } else {
            assert!(!prompt.contains("Selected unverified approach"));
        }
        let body=json!({"choices":[{"message":{"content":answer}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
    }
}
#[tokio::test]
async fn structured_resume_restores_unused_alternative_with_original_custom_policy_prompt_budget() {
    run(false).await;
}
#[tokio::test]
async fn prose_resume_preserves_reflection_and_next_alternative_without_replaying_explorer() {
    run(true).await;
}
async fn run(prose: bool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cfg = config(format!("http://{}/v1", listener.local_addr().unwrap()));
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let mut script = vec![("planner", "invalid"), ("explorer", PORTFOLIO)];
    if prose {
        script.extend([
            ("solver", "first answer"),
            ("critic", r#"{"revise":true,"reason":"Specific flaw"}"#),
            ("reflector", "New reflection"),
            ("solver", "second answer"),
            (
                "critic",
                r#"{"revise":false,"reason":"No useful revision"}"#,
            ),
        ]);
    } else {
        script.extend([
            ("solver", "[3,2,1]"),
            ("solver", "[2,3,1]"),
            ("solver", "[1,2,3]"),
        ]);
    }
    let mut engine = Engine::with_backend(
        cfg.clone(),
        store.clone(),
        Arc::new(Script(Mutex::new(script.into()))),
    );
    if !prose {
        engine = engine.with_escalation_policy(Arc::new(Allow)).unwrap();
    }
    let query = if prose {
        "Explain the original objective"
    } else {
        r#"sort:{"values":[3,2,1]}"#
    };
    let original = engine.run(query, "research").await.unwrap();
    assert_eq!(
        original.status,
        if prose { "unverified" } else { "completed" }
    );
    let task = &original.task_id;
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let cutoff:u64=db.query_row("SELECT id FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.cycles_before')=1",[task],|r|r.get(0)).unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    // Reconstruction uses original validated explorer output, not a new advisory audit field.
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND kind='exploration_portfolio'",
        [task],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [task],
    )
    .unwrap();
    drop(db);
    if prose {
        cogfabric::recovery::continuation::preview(&store, task).unwrap();
    } else {
        cogfabric::recovery::continuation::preview_with_escalation_policy(
            &store,
            task,
            Arc::new(Allow),
        )
        .unwrap();
    }
    let server = tokio::spawn(http(listener, prose));
    let resumed = if prose {
        cogfabric::recovery::continuation::resume(store.clone(), task, true, std::future::pending())
            .await
            .unwrap()
    } else {
        cogfabric::recovery::continuation::resume_with_escalation_policy(
            store.clone(),
            task,
            true,
            Arc::new(Allow),
            std::future::pending(),
        )
        .await
        .unwrap()
    };
    server.await.unwrap();
    assert_eq!(resumed["new_calls"], 2);
    assert_eq!(
        resumed["status"],
        if prose { "unverified" } else { "completed" }
    );
    let trace = store.task(task).unwrap();
    let events = trace["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["kind"] == "reasoning" && e["payload"]["node"]["role"] == "explorer")
            .count(),
        1
    );
    assert!(events.iter().any(|e| e["kind"] == "hypothesis_selection"
        && e["payload"]["cycle"] == 2
        && e["payload"]["selected_id"] == "b"));
    assert_eq!(
        resumed["charged_resources"]["calls"],
        if prose { 7 } else { 5 }
    );
    drop(engine);
    drop(store);
    std::fs::remove_dir_all(cfg.home).unwrap();
}
