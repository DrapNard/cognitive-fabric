//! Direct finite-search witnesses survive a crash without search or model replay.
use anyhow::Result;
use cogfabric::{
    backend::{Generation, InferenceBackend},
    config::Config,
    engine::Engine,
    reasoning::finite_search::Settings,
    store::{sha256, Store},
};
use serde_json::{json, Value};
use std::{future::Future, pin::Pin, process::Command, sync::Arc};

const QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[0,1]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]}"#;
const WITNESS: &str = r#"{"x":1}"#;
const INVALID: &str = r#"{"x":0}"#;

/// Direct deterministic search must not reach an inference backend.
struct NoInference;

impl InferenceBackend for NoInference {
    fn generate<'a>(
        &'a self,
        role: &'a str,
        _: &'a str,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Generation>> + Send + 'a>> {
        panic!("direct finite-search witness unexpectedly called {role}")
    }
}

fn config() -> Config {
    Config {
        home: std::env::temp_dir().join(format!(
            "fabric-finite-search-witness-recovery-{}",
            uuid::Uuid::new_v4()
        )),
        backend: "openai-local".into(),
        model: "pinned-original-model".into(),
        api_base: "http://127.0.0.1:1/v1".into(),
        interpret_goals: false,
        max_calls: 8,
        max_cycles: 4,
        max_tokens: 2400,
        max_tool_calls: 8,
        timeout_seconds: 120,
        finite_search: Some(Settings { max_nodes: 16 }),
        ..Config::default()
    }
}

fn events(trace: &Value, kind: &str) -> Vec<Value> {
    trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == kind)
        .cloned()
        .collect()
}

fn database(cfg: &Config) -> rusqlite::Connection {
    rusqlite::Connection::open(cfg.db()).unwrap()
}

fn home_remove(cfg: &Config) {
    let _ = std::fs::remove_dir_all(&cfg.home);
}

async fn interrupted_at(keep: &str) -> (Config, Arc<Store>, String) {
    let cfg = config();
    cfg.save().unwrap();
    let store = Arc::new(Store::open(&cfg.db()).unwrap());
    let outcome = Engine::with_backend(cfg.clone(), store.clone(), Arc::new(NoInference))
        .run(QUERY, "fast")
        .await
        .unwrap();
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.answer, WITNESS);
    assert_eq!(outcome.calls, 0, "search must finish without an LLM call");

    let before = store.task(&outcome.task_id).unwrap();
    let receipt = events(&before, "finite_search");
    assert_eq!(receipt.len(), 1);
    assert_eq!(receipt[0]["payload"]["format_version"], 2);
    assert_eq!(receipt[0]["payload"]["outcome"], "witness");
    assert_eq!(receipt[0]["payload"]["answer"], WITNESS);
    assert_eq!(
        receipt[0]["payload"]["answer_sha256"],
        sha256(WITNESS.as_bytes())
    );
    assert_eq!(events(&before, "verification").len(), 1);
    let task = outcome.task_id;
    let db = database(&cfg);
    let cutoff: i64 = match keep {
        "receipt" => db
            .query_row(
                "SELECT id FROM events WHERE task_id=?1 AND kind='finite_search'",
                [&task],
                |row| row.get(0),
            )
            .unwrap(),
        "reservation" => db
            .query_row(
                "SELECT id FROM events WHERE task_id=?1 AND kind='tool_reservation' AND json_extract(payload,'$.capability')='constraints.verify' ORDER BY id LIMIT 1",
                [&task],
                |row| row.get(0),
            )
            .unwrap(),
        "verification" => db
            .query_row(
                "SELECT id FROM events WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.origin')='finite_search'",
                [&task],
                |row| row.get(0),
            )
            .unwrap(),
        _ => unreachable!(),
    };
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
    (cfg, store, task)
}

#[tokio::test]
async fn recovers_direct_search_witness_from_each_crash_window_without_replaying_search_or_inference(
) {
    for boundary in ["receipt", "reservation", "verification"] {
        let (cfg, store, task) = interrupted_at(boundary).await;
        let before = store.task(&task).unwrap();
        let finite_search_before = events(&before, "finite_search");
        let original_search_charges = events(&before, "tool_reservation")
            .into_iter()
            .filter(|event| event["payload"]["capability"] == "constraints.solve")
            .count();
        let original_verify_charges = events(&before, "tool_reservation")
            .into_iter()
            .filter(|event| event["payload"]["capability"] == "constraints.verify")
            .count();
        let calls_before = store.inference_audit(&task).unwrap()["reserved_calls"].clone();

        let report = store.recover_verified_result(&task).unwrap();
        assert_eq!(report["status"], "completed", "boundary={boundary}");
        assert_eq!(report["verified"], true, "boundary={boundary}");
        assert_eq!(report["criterion"], "finite_domain/v1");
        assert_eq!(report["automatic_replay"], false);
        assert_eq!(report["inference_repeated"], false);
        assert_eq!(report["answer_hash"], sha256(WITNESS.as_bytes()));
        assert_eq!(report["finite_search_history"]["outcome"], "witness");
        assert_eq!(report["finite_search_history"]["verified"], false);
        assert_eq!(
            report["finite_search_history"]["permissions_granted"],
            false
        );
        assert!(report["finite_search_history"].get("answer").is_none());
        assert!(report["finite_search_history"]["event_id"]
            .as_i64()
            .is_some());
        assert!(report["finite_search_history"]["charge_event_id"]
            .as_i64()
            .is_some());

        let after = store.task(&task).unwrap();
        assert_eq!(after["task"]["answer"], WITNESS);
        assert_eq!(events(&after, "finite_search"), finite_search_before);
        assert_eq!(
            events(&after, "tool_reservation")
                .into_iter()
                .filter(|event| event["payload"]["capability"] == "constraints.solve")
                .count(),
            original_search_charges,
            "search is not charged or replayed during recovery; boundary={boundary}"
        );
        assert_eq!(
            events(&after, "tool_reservation")
                .into_iter()
                .filter(|event| event["payload"]["capability"] == "constraints.verify")
                .count(),
            original_verify_charges + 1,
            "recovery must reserve a fresh verifier charge; boundary={boundary}"
        );
        assert_eq!(
            store.inference_audit(&task).unwrap()["reserved_calls"],
            calls_before
        );
        let proofs = events(&after, "verification");
        assert!(proofs.iter().any(|event| {
            event["payload"]["origin"] == "saved_search_recovery"
                && event["payload"]["verified"] == true
                && event["payload"]["answer_hash"] == sha256(WITNESS.as_bytes())
        }));

        assert!(store.recover_verified_result(&task).is_err());
        home_remove(&cfg);
    }
}

#[tokio::test]
async fn malformed_or_conflicting_direct_witness_receipts_never_create_a_completion() {
    for tamper in [
        "objective",
        "answer",
        "answer_hash",
        "source",
        "duplicate",
        "configuration",
        "order",
        "history",
        "legacy_v1",
    ] {
        let (cfg, store, task) = interrupted_at("receipt").await;
        let db = database(&cfg);
        match tamper {
            "objective" => db.execute(
                "UPDATE events SET payload=json_set(payload,'$.objective_sha256','forged') WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "answer" => db.execute(
                "UPDATE events SET payload=json_set(payload,'$.answer','{\"x\":0}') WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "answer_hash" => db.execute(
                "UPDATE events SET payload=json_set(payload,'$.answer_sha256','forged') WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "source" => db.execute(
                "UPDATE events SET source='attacker' WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "duplicate" => db.execute(
                "INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "configuration" => db.execute(
                "UPDATE events SET payload=json_set(payload,'$.configuration.finite_search.max_nodes',15) WHERE task_id=?1 AND kind='execution_settings'",
                [&task],
            ).unwrap(),
            "order" => db.execute(
                "UPDATE events SET id=id+1000 WHERE task_id=?1 AND kind='tool_reservation' AND json_extract(payload,'$.capability')='constraints.solve'",
                [&task],
            ).unwrap(),
            "history" => db.execute(
                "INSERT INTO events(id,task_id,kind,source,payload,created) SELECT id+1,task_id,'cycle_reservation','jev-r','{}',created FROM events WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            "legacy_v1" => db.execute(
                "UPDATE events SET payload=json_remove(json_set(payload,'$.format_version',1),'$.answer','$.answer_sha256') WHERE task_id=?1 AND kind='finite_search'",
                [&task],
            ).unwrap(),
            _ => unreachable!(),
        };
        drop(db);
        let before = store.task(&task).unwrap();
        assert!(
            store.recover_verified_result(&task).is_err(),
            "tamper={tamper}"
        );
        assert_eq!(store.task(&task).unwrap(), before, "tamper={tamper}");
        home_remove(&cfg);
    }
}

#[tokio::test]
async fn failed_fresh_check_is_charged_but_never_completes_or_replays_search() {
    let (cfg, store, task) = interrupted_at("receipt").await;
    let db = database(&cfg);
    db.execute(
        "UPDATE events SET payload=json_set(payload,'$.answer',?2,'$.answer_sha256',?3) WHERE task_id=?1 AND kind='finite_search'",
        rusqlite::params![task, INVALID, sha256(INVALID.as_bytes())],
    )
    .unwrap();
    drop(db);
    let before = store.task(&task).unwrap();
    let search_before = events(&before, "finite_search");
    let verify_charges_before = events(&before, "tool_reservation")
        .into_iter()
        .filter(|event| event["payload"]["capability"] == "constraints.verify")
        .count();
    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["status"], "interrupted");
    assert_eq!(report["revision_ready"], false);
    assert_eq!(report["inference_repeated"], false);
    assert!(report["finite_search_history"].get("answer").is_none());
    let after = store.task(&task).unwrap();
    assert_eq!(events(&after, "finite_search"), search_before);
    assert_eq!(after["task"]["status"], "interrupted");
    assert_eq!(after["task"]["answer"], "");
    assert_eq!(
        events(&after, "tool_reservation")
            .into_iter()
            .filter(|event| event["payload"]["capability"] == "constraints.verify")
            .count(),
        verify_charges_before + 1
    );
    assert!(events(&after, "verification").iter().any(|event| {
        event["payload"]["origin"] == "saved_search_recovery"
            && event["payload"]["verified"] == false
    }));
    home_remove(&cfg);
}

#[tokio::test]
async fn failed_recovery_audit_rolls_back_the_fresh_verifier_charge() {
    let (cfg, store, task) = interrupted_at("receipt").await;
    database(&cfg)
        .execute_batch(
            "CREATE TRIGGER reject_saved_search_proof BEFORE INSERT ON events WHEN NEW.kind='verification' AND json_extract(NEW.payload,'$.origin')='saved_search_recovery' BEGIN SELECT RAISE(ABORT,'fixture verifier audit failure'); END;",
        )
        .unwrap();
    let before = store.task(&task).unwrap();
    assert!(store.recover_verified_result(&task).is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    database(&cfg)
        .execute_batch("DROP TRIGGER reject_saved_search_proof")
        .unwrap();
    assert_eq!(
        store.recover_verified_result(&task).unwrap()["verified"],
        true
    );
    home_remove(&cfg);
}

#[tokio::test]
async fn exhausted_original_tool_budget_refuses_a_free_verification_charge() {
    let (cfg, store, task) = interrupted_at("receipt").await;
    let db = database(&cfg);
    let existing_charges: u64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE task_id=?1 AND kind='tool_reservation'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    for invocation in (existing_charges + 1)..=u64::from(cfg.max_tool_calls) {
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'policy','security-kernel',?2,unixepoch())",
            rusqlite::params![task, json!({"task_id":task,"actor":"jev-plus","capability":"constraints.verify","allowed":true,"reason":null}).to_string()],
        ).unwrap();
        db.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'tool_reservation','security-kernel',?2,unixepoch())",
            rusqlite::params![task, json!({"format_version":1,"invocation":invocation,"actor":"jev-plus","capability":"constraints.verify"}).to_string()],
        ).unwrap();
    }
    drop(db);
    assert_eq!(
        store.inference_audit(&task).unwrap()["remaining_inference_admission"]["tool_calls"],
        0,
        "fixture must exhaust a valid original task allowance"
    );
    let before = store.task(&task).unwrap();
    assert!(store.recover_verified_result(&task).is_err());
    assert_eq!(store.task(&task).unwrap(), before);
    home_remove(&cfg);
}

#[tokio::test]
async fn recovery_uses_receipt_witness_instead_of_untrusted_task_answer() {
    let (cfg, store, task) = interrupted_at("receipt").await;
    database(&cfg)
        .execute(
            "UPDATE tasks SET answer=?2 WHERE id=?1",
            rusqlite::params![task, INVALID],
        )
        .unwrap();
    let report = store.recover_verified_result(&task).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(store.task(&task).unwrap()["task"]["answer"], WITNESS);
    home_remove(&cfg);
}

#[tokio::test]
async fn cli_recovers_direct_search_witness_from_an_interrupted_task() {
    let (cfg, store, task) = interrupted_at("receipt").await;
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(&cfg.home)
        .args(["--json", "recover-verified-result", &task])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["verified"], true);
    assert_eq!(report["finite_search_history"]["outcome"], "witness");
    assert_eq!(store.task(&task).unwrap()["task"]["answer"], WITNESS);
    home_remove(&cfg);
}

#[tokio::test]
async fn finite_search_history_report_contains_only_bounded_receipt_metadata() {
    let (cfg, store, task) = interrupted_at("receipt").await;
    let report = store.recover_verified_result(&task).unwrap();
    let history = &report["finite_search_history"];
    assert_eq!(history["answer_sha256"], sha256(WITNESS.as_bytes()));
    assert!(history.get("answer").is_none());
    assert_eq!(history["automatic_replay"], false);
    assert_eq!(history["verified"], false);
    assert_eq!(history["permissions_granted"], false);
    home_remove(&cfg);
}
