use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    reasoning::finite_search::Settings,
    store::Store,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const QUERY: &str = r#"csp:{"variables":[{"id":"a","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"a","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const WITNESS: &str = r#"{"a":1}"#;

struct Script(Mutex<VecDeque<(&'static str, &'static str)>>);

impl InferenceBackend for Script {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        let (expected, text) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected inference during original task");
        assert_eq!(role, expected);
        Box::pin(async move {
            Ok(Generation {
                text: text.into(),
                model: "fixture-model".into(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_ms: 0.0,
                queue_wait_ms: Some(0.0),
                provider_metrics: None,
            })
        })
    }
}

fn config(endpoint: String) -> Config {
    Config {
        home: std::env::temp_dir().join(format!(
            "fabric-finite-search-resume-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        model: "pinned-original-model".into(),
        api_base: endpoint,
        interpret_goals: false,
        max_calls: 8,
        max_cycles: 4,
        max_tokens: 2400,
        max_tool_calls: 8,
        timeout_seconds: 120,
        finite_search: Some(Settings { max_nodes: 1 }),
        ..Config::default()
    }
}

fn event<'a>(trace: &'a Value, kind: &str) -> Vec<&'a Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == kind)
        .collect()
}

async fn http_answers(listener: Arc<tokio::net::TcpListener>, answers: Vec<(u64, &'static str)>) {
    for (tokens, answer) in answers {
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .expect("timed out waiting for resumed inference")
            .unwrap();
        let mut bytes = Vec::new();
        let request = loop {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
                .await
                .expect("timed out reading resumed inference")
                .unwrap();
            assert!(n > 0, "provider closed before a complete request");
            bytes.extend_from_slice(&chunk[..n]);
            assert!(
                bytes.len() < 64_000,
                "provider request exceeded fixture bound"
            );
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .expect("missing request content-length")
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                }
            }
        };
        assert_eq!(request["model"], "pinned-original-model");
        assert_eq!(request["max_tokens"], tokens);
        let body = json!({
            "model":"pinned-original-model",
            "choices":[{"message":{"content":answer}}],
            "usage":{"prompt_tokens":1,"completion_tokens":1}
        })
        .to_string();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }
}

async fn http_once(listener: Arc<tokio::net::TcpListener>, answer: &'static str) {
    http_answers(listener, vec![(384, answer)]).await;
}

fn ready_boundary_id(db: &rusqlite::Connection, task: &str) -> i64 {
    db.query_row(
        "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary' AND json_extract(payload,'$.phase')='ready_solver'",
        [task],
        |row| row.get(0),
    )
    .unwrap()
}

fn rewind_after_boundary(cfg: &Config, task: &str) -> (u64, u64, u64) {
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let cutoff = ready_boundary_id(&db, task);
    let boundary: Value = db
        .query_row(
            "SELECT payload FROM events WHERE task_id=?1 AND id=?2",
            rusqlite::params![task, cutoff],
            |row| row.get::<_, String>(0),
        )
        .map(|raw| serde_json::from_str(&raw).unwrap())
        .unwrap();
    let counters = (
        boundary["calls_before"].as_u64().unwrap(),
        boundary["cycles_before"].as_u64().unwrap(),
        boundary["tool_calls_before"].as_u64().unwrap(),
    );
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [task],
    )
    .unwrap();
    counters
}

fn remove_home(cfg: &Config) {
    let _ = std::fs::remove_dir_all(&cfg.home);
}

async fn limit_reached_checkpoint() -> (Config, Arc<Store>, String) {
    let cfg = config("http://127.0.0.1:1/v1".into());
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(vec![("solver", WITNESS)].into())));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), script)
        .run(QUERY, "fast")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    let task = outcome.task_id;
    let trace = store.task(&task).unwrap();
    assert_eq!(event(&trace, "finite_search").len(), 1);
    assert_eq!(
        event(&trace, "finite_search")[0]["payload"]["outcome"],
        "limit_reached"
    );
    assert_eq!(
        event(&trace, "tool_reservation")
            .iter()
            .filter(|item| item["payload"]["capability"] == "constraints.solve")
            .count(),
        1
    );
    rewind_after_boundary(&cfg, &task);
    (cfg, store, task)
}

async fn graph_limit_reached_checkpoint(endpoint: String) -> (Config, Arc<Store>, String) {
    const PLAN: &str = r#"{"steps":[{"id":"a","objective":"Inspect the objective","depends_on":[]},{"id":"b","objective":"Review the first result","depends_on":["a"]}]}"#;
    let cfg = config(endpoint);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let backend = Arc::new(Script(Mutex::new(
        vec![
            ("planner", PLAN),
            ("solver", "Original A advice"),
            ("solver", "Original B advice"),
            ("solver", WITNESS),
        ]
        .into(),
    )));
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), backend.clone())
        .run_profile(
            QUERY,
            "deep",
            cogfabric::engine::ExecutionProfile::Collective,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert!(backend.0.lock().unwrap().is_empty());
    let task = outcome.task_id;
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let cutoff: i64 = db
        .query_row(
            "SELECT min(id)-1 FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='b' AND json_extract(payload,'$.node.state')='running'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "DELETE FROM events WHERE task_id=?1 AND id>?2",
        rusqlite::params![task, cutoff],
    )
    .unwrap();
    db.execute(
        "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
        [&task],
    )
    .unwrap();
    drop(db);
    let trace = store.task(&task).unwrap();
    assert_eq!(event(&trace, "finite_search").len(), 1);
    assert_eq!(
        event(&trace, "finite_search")[0]["payload"]["outcome"],
        "limit_reached"
    );
    assert_eq!(event(&trace, "reasoning_boundary").len(), 0);
    (cfg, store, task)
}

fn search_event_id(db: &rusqlite::Connection, task: &str, kind: &str) -> i64 {
    db.query_row(
        "SELECT id FROM events WHERE task_id=?1 AND kind=?2 ORDER BY id LIMIT 1",
        rusqlite::params![task, kind],
        |row| row.get(0),
    )
    .unwrap()
}

fn tamper_checkpoint(db: &rusqlite::Connection, task: &str, tamper: &str) {
    let receipt = search_event_id(db, task, "finite_search");
    let charge = db
        .query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='tool_reservation' AND json_extract(payload,'$.capability')='constraints.solve'",
            [task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    match tamper {
        "version" => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.format_version',2) WHERE id=?1",
                [receipt],
            )
            .unwrap();
        }
        "criterion" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.criterion','sorting/v1') WHERE id=?1", [receipt]).unwrap();
        }
        "objective_hash" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.objective_sha256','bad') WHERE id=?1", [receipt]).unwrap();
        }
        "node_limit" => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.max_nodes',2) WHERE id=?1",
                [receipt],
            )
            .unwrap();
        }
        "explored_nodes" => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.explored_nodes',0) WHERE id=?1",
                [receipt],
            )
            .unwrap();
        }
        "outcome" => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.outcome','witness') WHERE id=?1",
                [receipt],
            )
            .unwrap();
        }
        "verified" => {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.verified',true) WHERE id=?1",
                [receipt],
            )
            .unwrap();
        }
        "permissions" => {
            db.execute("UPDATE events SET payload=json_set(payload,'$.permissions_granted',true) WHERE id=?1", [receipt]).unwrap();
        }
        "source" => {
            db.execute("UPDATE events SET source='attacker' WHERE id=?1", [receipt])
                .unwrap();
        }
        "missing_receipt" => {
            db.execute("DELETE FROM events WHERE id=?1", [receipt])
                .unwrap();
        }
        "receipt_without_charge" => {
            db.execute("DELETE FROM events WHERE id=?1", [charge])
                .unwrap();
        }
        "missing_policy" => {
            db.execute("DELETE FROM events WHERE task_id=?1 AND kind='policy' AND json_extract(payload,'$.capability')='constraints.solve'", [task]).unwrap();
        }
        "duplicate_receipt" => {
            db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE id=?1", [receipt]).unwrap();
        }
        "duplicate_charge" => {
            db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE id=?1", [charge]).unwrap();
        }
        "charge_after_receipt" => {
            let max_id: i64 = db
                .query_row("SELECT max(id) FROM events", [], |row| row.get(0))
                .unwrap();
            db.execute(
                "UPDATE events SET id=?2 WHERE id=?1",
                rusqlite::params![charge, max_id + 1],
            )
            .unwrap();
        }
        "receipt_after_boundary" => {
            let max_id: i64 = db
                .query_row("SELECT max(id) FROM events", [], |row| row.get(0))
                .unwrap();
            db.execute(
                "UPDATE events SET id=?2 WHERE id=?1",
                rusqlite::params![receipt, max_id + 1],
            )
            .unwrap();
        }
        "extra_charge_between_context_and_receipt" => {
            let context = search_event_id(db, task, "reasoning_context");
            let policy = db.query_row(
                "SELECT id FROM events WHERE task_id=?1 AND kind='policy' AND json_extract(payload,'$.capability')='constraints.solve'",
                [task], |row| row.get::<_, i64>(0)
            ).unwrap();
            // Move the charge and all later events far forward, leaving an empty
            // identifier range in which to insert an unrelated charged tool use.
            db.execute(
                "UPDATE events SET id=id+1000000000 WHERE task_id=?1 AND id>=?2",
                rusqlite::params![task, charge],
            )
            .unwrap();
            let moved_charge = charge + 1_000_000_000;
            let inserted_id = (policy + moved_charge) / 2;
            let invocation = db.query_row(
                "SELECT count(*)+1 FROM events WHERE task_id=?1 AND kind='tool_reservation' AND id<?2",
                rusqlite::params![task, inserted_id],
                |row| row.get::<_, u64>(0),
            ).unwrap();
            db.execute(
                "INSERT INTO events(id,task_id,kind,source,payload,created) VALUES(?1,?2,'tool_reservation','security-kernel',?3,unixepoch())",
                rusqlite::params![inserted_id,task,json!({"format_version":1,"invocation":invocation,"actor":"jev-plus","capability":"knowledge.search"}).to_string()]
            ).unwrap();
            let solve_invocation = invocation + 1;
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.invocation',?2) WHERE id=?1",
                rusqlite::params![moved_charge, solve_invocation],
            )
            .unwrap();
            assert!(inserted_id > context && inserted_id < moved_charge);
        }
        "boundary" => {
            let boundary = ready_boundary_id(db, task);
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.phase','unknown') WHERE id=?1",
                [boundary],
            )
            .unwrap();
        }
        other => panic!("unknown tamper case: {other}"),
    }
}

#[tokio::test]
async fn limit_receipt_resumes_from_ready_solver_without_replaying_search_and_keeps_original_pins()
{
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let mut cfg = config(endpoint);
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let script = Arc::new(Script(Mutex::new(vec![("solver", WITNESS)].into())));
    let original = Engine::with_backend(cfg.clone(), store.clone(), script.clone())
        .run(QUERY, "fast")
        .await
        .unwrap();
    assert_eq!(original.status, "completed");
    assert_eq!(original.calls, 1);
    assert_eq!(script.0.lock().unwrap().len(), 0);

    let task = original.task_id;
    let trace = store.task(&task).unwrap();
    let receipt = event(&trace, "finite_search");
    assert_eq!(receipt.len(), 1);
    assert_eq!(receipt[0]["payload"]["outcome"], "limit_reached");
    let solve_charges = event(&trace, "tool_reservation")
        .into_iter()
        .filter(|item| item["payload"]["capability"] == "constraints.solve")
        .count();
    assert_eq!(solve_charges, 1);

    let (calls_before, cycles_before, tools_before) = rewind_after_boundary(&cfg, &task);
    assert_eq!((calls_before, cycles_before), (0, 0));
    assert!(tools_before >= 2); // knowledge lookup and the original search charge
    let preview = cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    let pinned_history = preview["finite_search_history"].clone();
    assert_eq!(pinned_history["outcome"], "limit_reached");
    assert_eq!(pinned_history["max_nodes"], 1);
    assert_eq!(pinned_history["explored_nodes"], 1);
    assert_eq!(
        pinned_history["objective_sha256"],
        cogfabric::store::sha256(QUERY.as_bytes())
    );
    assert_eq!(pinned_history["automatic_replay"], false);
    assert_eq!(pinned_history["verified"], false);
    assert_eq!(pinned_history["permissions_granted"], false);
    let db = rusqlite::Connection::open(cfg.db()).unwrap();
    let receipt_id = search_event_id(&db, &task, "finite_search");
    let charge_id: i64 = db
        .query_row(
            "SELECT id FROM events WHERE task_id=?1 AND kind='tool_reservation' AND json_extract(payload,'$.capability')='constraints.solve'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pinned_history["event_id"], receipt_id);
    assert_eq!(pinned_history["charge_event_id"], charge_id);
    drop(db);

    // The mutable current configuration must not replace the task's immutable pins.
    cfg.model = "changed-current-model".into();
    cfg.api_base = "http://127.0.0.1:1/v1".into();
    cfg.max_tokens = 300;
    cfg.max_tool_calls = 3;
    cfg.finite_search = None;
    cfg.save().unwrap();

    let mut resumed_calls = Vec::new();
    for answer in [WITNESS, WITNESS] {
        let server = tokio::spawn(http_once(listener.clone(), answer));
        let report = cogfabric::recovery::continuation::resume(
            store.clone(),
            &task,
            true,
            std::future::pending(),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(report["new_calls"], 1);
        assert_eq!(report["status"], "completed");
        assert_eq!(report["finite_search_history"], pinned_history);
        resumed_calls.push(report["charged_resources"].clone());

        if resumed_calls.len() == 1 {
            // Model the crash point after the resumed call's durable ready-solver
            // boundary and before that boundary's solver dispatch.
            let cutoff = {
                let db = rusqlite::Connection::open(cfg.db()).unwrap();
                ready_boundary_id(&db, &task)
            };
            let db = rusqlite::Connection::open(cfg.db()).unwrap();
            db.execute(
                "DELETE FROM events WHERE task_id=?1 AND id>?2",
                rusqlite::params![task, cutoff],
            )
            .unwrap();
            db.execute(
                "UPDATE tasks SET status='interrupted',answer='' WHERE id=?1",
                [&task],
            )
            .unwrap();
        }
    }

    assert_eq!(resumed_calls[0]["tool_calls"], tools_before + 1);
    assert_eq!(resumed_calls[1]["tool_calls"], tools_before + 1);
    let final_trace = store.task(&task).unwrap();
    assert_eq!(
        event(&final_trace, "finite_search")
            .iter()
            .filter(|item| item["payload"]["outcome"] == "limit_reached")
            .count(),
        1
    );
    assert_eq!(
        event(&final_trace, "tool_reservation")
            .iter()
            .filter(|item| item["payload"]["capability"] == "constraints.solve")
            .count(),
        1
    );
    assert_eq!(event(&final_trace, "reasoning_context").len(), 3);
    assert_eq!(event(&final_trace, "verification").len(), 1);
    let starts = event(&final_trace, "continuation")
        .into_iter()
        .filter(|item| item["payload"]["state"] == "started")
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    assert!(starts
        .iter()
        .all(|item| item["payload"]["finite_search_history"] == pinned_history));
    remove_home(&cfg);
}

#[tokio::test]
async fn finite_search_checkpoint_tampering_is_refused_before_dispatch_or_new_charges() {
    for tamper in [
        "version",
        "criterion",
        "objective_hash",
        "node_limit",
        "explored_nodes",
        "outcome",
        "verified",
        "permissions",
        "source",
        "missing_receipt",
        "receipt_without_charge",
        "missing_policy",
        "duplicate_receipt",
        "duplicate_charge",
        "charge_after_receipt",
        "receipt_after_boundary",
        "extra_charge_between_context_and_receipt",
        "boundary",
    ] {
        let (cfg, store, task) = limit_reached_checkpoint().await;
        let db = rusqlite::Connection::open(cfg.db()).unwrap();
        tamper_checkpoint(&db, &task, tamper);
        drop(db);

        let before = store.task(&task).unwrap();
        assert!(
            cogfabric::recovery::continuation::preview(&store, &task).is_err(),
            "tamper case {tamper} passed preview"
        );
        let result = cogfabric::recovery::continuation::resume(
            store.clone(),
            &task,
            true,
            std::future::ready(()),
        )
        .await;
        assert!(result.is_err(), "tamper case {tamper} passed admission");
        assert_eq!(
            store.task(&task).unwrap(),
            before,
            "tamper case {tamper} wrote events or charges"
        );
        remove_home(&cfg);
    }
}

#[tokio::test]
async fn limit_receipt_resumes_pending_advisory_graph_without_replaying_search() {
    let listener = Arc::new(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let (cfg, store, task) = graph_limit_reached_checkpoint(endpoint).await;

    let preview = cogfabric::recovery::continuation::preview(&store, &task).unwrap();
    assert_eq!(preview["phase"], "pre_solver_graph");
    assert_eq!(preview["finite_search_history"]["outcome"], "limit_reached");
    assert_eq!(preview["subtasks"].as_array().unwrap().len(), 2);
    assert_eq!(preview["subtasks"][0]["state"], "unverified");
    assert_eq!(preview["subtasks"][1]["state"], "pending");

    let provider = tokio::spawn(http_answers(
        listener,
        vec![(192, "Resumed B advice"), (384, WITNESS)],
    ));
    let report = cogfabric::recovery::continuation::resume(
        store.clone(),
        &task,
        true,
        std::future::pending(),
    )
    .await
    .unwrap();
    provider.await.unwrap();

    assert_eq!(report["status"], "completed");
    assert_eq!(report["new_calls"], 2);
    assert_eq!(report["charged_resources"]["calls"], 4);
    assert_eq!(report["charged_resources"]["reserved_output_tokens"], 1024);
    assert_eq!(report["charged_resources"]["tool_calls"], 2);
    assert_eq!(
        report["finite_search_history"],
        preview["finite_search_history"]
    );
    let trace = store.task(&task).unwrap();
    assert_eq!(
        event(&trace, "finite_search")
            .iter()
            .filter(|item| item["payload"]["outcome"] == "limit_reached")
            .count(),
        1
    );
    assert_eq!(
        event(&trace, "tool_reservation")
            .iter()
            .filter(|item| item["payload"]["capability"] == "constraints.solve")
            .count(),
        1
    );
    assert_eq!(event(&trace, "verification").len(), 1);
    remove_home(&cfg);
}
