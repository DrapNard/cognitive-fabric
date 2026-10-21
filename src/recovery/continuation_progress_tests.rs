use super::progress_history;
use crate::{
    config::Config,
    reasoning::progress::{History, Settings},
    store::{sha256, Store},
};
use serde_json::{json, Value};

const QUERY: &str = r#"csp:{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":2}]}"#;

fn fixture(scores: &[i64]) -> (Store, String, Config, History) {
    let store = Store::in_memory().unwrap();
    let task = store.create_task(QUERY, "research").unwrap();
    let cfg = Config {
        progress_stopping: Some(Settings { plateau_cycles: 2 }),
        ..Config::default()
    };
    let mut history = History::default();
    for (offset, value) in scores.iter().enumerate() {
        let cycle = offset as u32 + 1;
        let answer = json!({"x":value}).to_string();
        let hash = sha256(answer.as_bytes());
        store.event(&task, "reasoning", "jev-r", json!({
            "call":cycle,"node":{"id":offset,"parent":offset.checked_sub(1),"role":"solver","cycle":cycle,"model":"fixture","content_hash":hash},"text":answer
        })).unwrap();
        store.event(&task, "verification", "verifier-r", json!({
            "criterion":"finite_domain/v1","verified":false,"objective_hash":sha256(QUERY.as_bytes()),"answer_hash":hash,"cycle":cycle,
            "reason":crate::verification::Criterion::from_query(QUERY).unwrap().unwrap().check(&answer).unwrap_err().to_string()
        })).unwrap();
        let assessment = history
            .observe(
                QUERY,
                cycle,
                &answer,
                cfg.progress_stopping.as_ref().unwrap(),
            )
            .unwrap();
        store
            .event(&task, "reasoning_progress", "verifier-r", assessment)
            .unwrap();
    }
    (store, task, cfg, history)
}

fn boundary(history: &History) -> Value {
    json!({"progress_state":serde_json::to_value(history).unwrap()})
}

#[test]
fn continuation_recomputes_scoped_progress_and_rejects_a_tampered_audit_or_snapshot() {
    let (store, task, cfg, history) = fixture(&[1, 1]);
    let db = store.lock().unwrap();
    assert_eq!(
        progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).unwrap(),
        history
    );
    drop(db);

    for mutation in [
        "score",
        "snapshot",
        "missing",
        "source",
        "hash",
        "criterion",
        "objective",
        "duplicate",
        "future",
        "gap",
        "order",
    ] {
        let (store, task, cfg, history) = fixture(&[1, 1]);
        {
            let db = store.lock().unwrap();
            match mutation {
                "score" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.score',0) WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                "missing" => {
                    db.execute("DELETE FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                "source" => {
                    db.execute("UPDATE events SET source='foreign' WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                "hash" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.answer_hash',?2) WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", rusqlite::params![task, "0".repeat(64)]).unwrap();
                }
                "criterion" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.criterion','integer_sort/v1') WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                "objective" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.objective_hash',?2) WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", rusqlite::params![task, "0".repeat(64)]).unwrap();
                }
                "duplicate" => {
                    db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                "future" => {
                    let raw: String = db.query_row("SELECT payload FROM events WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task], |row| row.get(0)).unwrap();
                    let mut value: Value = serde_json::from_str(&raw).unwrap();
                    value["cycle"] = json!(3);
                    db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES(?1,'reasoning_progress','verifier-r',?2,unixepoch())", rusqlite::params![task, value.to_string()]).unwrap();
                }
                "gap" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.cycle',3) WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                "order" => {
                    db.execute("UPDATE events SET id=id+100000 WHERE task_id=?1", [&task])
                        .unwrap();
                    db.execute("UPDATE events SET id=-1 WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                    db.execute("UPDATE events SET id=-2 WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                    db.execute("UPDATE events SET id=100001 WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.cycle')=1", [&task]).unwrap();
                    db.execute("UPDATE events SET id=100002 WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=1", [&task]).unwrap();
                    db.execute("UPDATE events SET id=100003 WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=1", [&task]).unwrap();
                    db.execute("UPDATE events SET id=100004 WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.node.cycle')=2", [&task]).unwrap();
                    db.execute("UPDATE events SET id=100005 WHERE task_id=?1 AND kind='reasoning_progress' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                    db.execute("UPDATE events SET id=100006 WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=2", [&task]).unwrap();
                }
                _ => {}
            }
            let mut saved = boundary(&history);
            if mutation == "snapshot" {
                saved["progress_state"]["non_improving_cycles"] = json!(0);
            }
            assert!(
                progress_history::validate(&db, &task, QUERY, &cfg, Some(&saved)).is_err(),
                "accepted {mutation}"
            );
            let other_query = r#"csp:{"variables":[{"id":"x","domain":[1,2]}],"constraints":[{"kind":"linear","terms":[{"variable":"x","coefficient":1}],"relation":"eq","rhs":1}]}"#;
            assert!(
                progress_history::validate(&db, &task, other_query, &cfg, Some(&saved)).is_err(),
                "accepted objective isolation"
            );
        }
    }
}

#[test]
fn recovery_refuses_plateaued_boundary_and_orphan_or_future_verification() {
    let (store, task, cfg, history) = fixture(&[1, 1, 1]);
    assert_eq!(history.observed_cycles(), 3);
    {
        let db = store.lock().unwrap();
        assert!(
            progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).is_err()
        );
    }

    let (store, task, cfg, history) = fixture(&[1]);
    store.event(&task, "verification", "verifier-r", json!({
        "criterion":"finite_domain/v1","verified":false,"objective_hash":sha256(QUERY.as_bytes()),"answer_hash":"0".repeat(64),"cycle":2
    })).unwrap();
    let db = store.lock().unwrap();
    assert!(
        progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).is_err()
    );
}

#[test]
fn accepts_one_exact_operator_recheck_after_a_durable_failed_assessment() {
    let (store, task, cfg, history) = fixture(&[1, 1]);
    let answer = r#"{"x":1}"#;
    let criterion = crate::verification::Criterion::from_query(QUERY)
        .unwrap()
        .unwrap();
    let reason = criterion.check(answer).unwrap_err().to_string();
    store
        .event(
            &task,
            "verification",
            "verifier-r",
            json!({
                "criterion":"finite_domain/v1",
                "verified":false,
                "origin":"saved_solver_recovery",
                "objective_hash":sha256(QUERY.as_bytes()),
                "answer_hash":sha256(answer.as_bytes()),
                "cycle":2,
                "reason":reason
            }),
        )
        .unwrap();
    let db = store.lock().unwrap();
    assert_eq!(
        progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).unwrap(),
        history
    );
}

fn cycle_zero_solver_and_recheck(store: &Store, task: &str, answer: &str) {
    let criterion = crate::verification::Criterion::from_query(QUERY)
        .unwrap()
        .unwrap();
    let hash = sha256(answer.as_bytes());
    let call = 20;
    store
        .event(
            task,
            "reasoning",
            "jev-r",
            json!({
                "call":call,
                "node":{"id":20,"parent":null,"role":"solver","cycle":0,"model":"fixture","content_hash":hash},
                "text":answer
            }),
        )
        .unwrap();
    store
        .event(
            task,
            "verification",
            "verifier-r",
            json!({
                "criterion":"finite_domain/v1",
                "verified":false,
                "origin":"saved_solver_recovery",
                "objective_hash":sha256(QUERY.as_bytes()),
                "answer_hash":hash,
                "reason":criterion.check(answer).unwrap_err().to_string(),
                "cycle":0,
                "call":call
            }),
        )
        .unwrap();
}

#[test]
fn cycle_zero_saved_solver_rechecks_are_bound_but_never_advance_progress() {
    let (store, task, cfg, history) = fixture(&[1, 1]);
    cycle_zero_solver_and_recheck(&store, &task, r#"{"x":1}"#);
    let db = store.lock().unwrap();
    assert_eq!(
        progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).unwrap(),
        history
    );
    drop(db);

    for mutation in [
        "call",
        "hash",
        "reason",
        "objective",
        "origin",
        "verified",
        "extra",
        "source",
        "cycle",
        "duplicate",
    ] {
        let (store, task, cfg, history) = fixture(&[1]);
        cycle_zero_solver_and_recheck(&store, &task, r#"{"x":1}"#);
        {
            let db = store.lock().unwrap();
            match mutation {
                "call" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.call',21) WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "hash" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.answer_hash',?2) WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", rusqlite::params![task, "0".repeat(64)]).unwrap();
                }
                "reason" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.reason','fabricated') WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "objective" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.objective_hash',?2) WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", rusqlite::params![task, "0".repeat(64)]).unwrap();
                }
                "origin" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.origin','model_claim') WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "verified" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.verified',true) WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "extra" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.permissions_granted',true) WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "source" => {
                    db.execute("UPDATE events SET source='model' WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "cycle" => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.cycle',1) WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                "duplicate" => {
                    db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.cycle')=0", [&task]).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history)))
                    .is_err(),
                "accepted {mutation}"
            );
        }
    }
}

#[test]
fn continuation_rejects_assessments_written_after_the_selected_boundary() {
    let (store, task, cfg, history) = fixture(&[1]);
    let saved_boundary = boundary(&history);
    store
        .event(&task, "reasoning_boundary", "jev-r", saved_boundary.clone())
        .unwrap();
    let boundary_id: i64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT max(id) FROM events WHERE task_id=?1 AND kind='reasoning_boundary'",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let mut future = store.task(&task).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "reasoning_progress")
        .unwrap()["payload"]
        .clone();
    future["cycle"] = json!(2);
    store
        .event(&task, "reasoning_progress", "verifier-r", future)
        .unwrap();
    let db = store.lock().unwrap();
    assert!(progress_history::validate_before(
        &db,
        &task,
        QUERY,
        &cfg,
        Some(&saved_boundary),
        None,
        Some(boundary_id)
    )
    .is_err());
}

#[test]
fn valid_pre_context_published_memory_failures_do_not_enter_solver_progress_history() {
    let (store, task, cfg, history) = fixture(&[1]);
    let memory_id = uuid::Uuid::new_v4().to_string();
    store
        .event(
            &task,
            "reasoning_context",
            "jev-r",
            json!({"format_version":1}),
        )
        .unwrap();
    let mut memory_check = json!({
        "criterion":"finite_domain/v1",
        "verified":false,
        "origin":"published_memory",
        "memory_id":memory_id,
        "expert":null,
        "objective_hash":sha256(QUERY.as_bytes()),
        "answer_hash":"a".repeat(64),
        "reason":"Assignment violates declared constraint 0"
    });
    store
        .event(&task, "verification", "verifier-r", memory_check.clone())
        .unwrap();
    {
        let db = store.lock().unwrap();
        db.execute("UPDATE events SET id=id+100000 WHERE task_id=?1", [&task])
            .unwrap();
        db.execute(
            "UPDATE events SET id=1 WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.origin')='published_memory'",
            [&task],
        )
        .unwrap();
        db.execute(
            "UPDATE events SET id=2 WHERE task_id=?1 AND kind='reasoning_context'",
            [&task],
        )
        .unwrap();
        db.execute(
            "UPDATE events SET id=3 WHERE task_id=?1 AND kind='reasoning'",
            [&task],
        )
        .unwrap();
        db.execute(
            "UPDATE events SET id=4 WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.origin') IS NULL",
            [&task],
        )
        .unwrap();
        db.execute(
            "UPDATE events SET id=5 WHERE task_id=?1 AND kind='reasoning_progress'",
            [&task],
        )
        .unwrap();
        assert_eq!(
            progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).unwrap(),
            history
        );
        memory_check["objective_hash"] = json!("0".repeat(64));
        db.execute(
            "UPDATE events SET payload=?2 WHERE task_id=?1 AND kind='verification' AND json_extract(payload,'$.origin')='published_memory'",
            rusqlite::params![task, memory_check.to_string()],
        )
        .unwrap();
        assert!(
            progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).is_err()
        );
    }
}

#[test]
fn recovery_can_fill_the_missing_failed_check_and_assessment_crash_window() {
    let (store, task, cfg, _) = fixture(&[1]);
    let mut history = History::default();
    {
        let db = store.lock().unwrap();
        db.execute(
            "DELETE FROM events WHERE task_id=?1 AND kind IN ('verification','reasoning_progress')",
            [&task],
        )
        .unwrap();
    }
    let answer = r#"{"x":1}"#;
    let criterion = crate::verification::Criterion::from_query(QUERY)
        .unwrap()
        .unwrap();
    let reason = criterion.check(answer).unwrap_err().to_string();
    store
        .event(
            &task,
            "verification",
            "verifier-r",
            json!({
                "criterion":"finite_domain/v1",
                "verified":false,
                "origin":"saved_solver_recovery",
                "objective_hash":sha256(QUERY.as_bytes()),
                "answer_hash":sha256(answer.as_bytes()),
                "cycle":1,
                "reason":reason
            }),
        )
        .unwrap();
    let assessment = history
        .observe(QUERY, 1, answer, cfg.progress_stopping.as_ref().unwrap())
        .unwrap();
    store
        .event(&task, "reasoning_progress", "verifier-r", assessment)
        .unwrap();
    let db = store.lock().unwrap();
    assert_eq!(
        progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).unwrap(),
        history
    );
}

#[test]
fn operator_recovery_config_without_opt_in_rejects_unexpected_progress_snapshot() {
    let (store, task, _cfg, history) = fixture(&[1]);
    let cfg = Config::default();
    let db = store.lock().unwrap();
    assert!(
        progress_history::validate(&db, &task, QUERY, &cfg, Some(&boundary(&history))).is_err()
    );
}
