use super::*;
use crate::{
    config::Config,
    planning::{Plan, Subtask},
    reasoning::Node,
    scheduling::{StableScheduler, SubtaskScheduler},
    store::{sha256, Store},
};
use serde_json::json;

fn fixture() -> (Store, String, Plan, Vec<Subtask>) {
    let store = Store::in_memory().unwrap();
    let task = store
        .create_task("sort:{\"values\":[3,1,2]}", "deep")
        .unwrap();
    let cfg = Config {
        subtask_parallelism: Some(2),
        ..Config::default()
    };
    let call_limit = 10u64.min(cfg.max_calls as u64);
    let cycle_limit = 3u64.min(cfg.max_cycles as u64);
    store
        .event(
            &task,
            "execution_settings",
            "runtime",
            json!({
                "format_version":1,
                "configuration":cfg,
                "mode":"deep",
                "call_limit":call_limit,
                "cycle_limit":cycle_limit
            }),
        )
        .unwrap();

    let plan_text = r#"{"steps":[{"id":"a","objective":"first independent check","depends_on":[]},{"id":"b","objective":"second independent check","depends_on":[]}]}"#;
    let plan = Plan::parse(plan_text).unwrap();
    let planner_text = serde_json::to_string(&plan).unwrap();
    dispatch(&store, &task, 1, "planner", 256, 128);
    response(&store, &task, 1, "planner", 0, &planner_text);
    store
        .event(
            &task,
            "plan",
            "planner-r",
            json!({"graph":plan,"trusted_permissions":false,"scheduler_required":true}),
        )
        .unwrap();
    let scheduler = StableScheduler.descriptor();
    store.event(&task,"subtask_schedule_intent","jev-plus",json!({"scheduler":scheduler,"permissions_granted":false,"implementation_attested":false})).unwrap();
    store.event(&task,"subtask_schedule","jev-plus",json!({"format_version":1,"scheduler":scheduler,"graph_sha256":sha256(&serde_json::to_vec(&plan).unwrap()),"order":[0,1],"permissions_granted":false})).unwrap();

    let mut subtasks = plan
        .steps
        .iter()
        .cloned()
        .map(|step| Subtask {
            step,
            state: "pending".into(),
            result: None,
        })
        .collect::<Vec<_>>();
    for node in &subtasks {
        state_event(&store, &task, node);
    }
    store
        .event(
            &task,
            "subtask_parallel_schedule",
            "jev-plus",
            json!({
                "format_version":1,
                "max_parallelism":2,
                "graph_sha256":sha256(&serde_json::to_vec(&plan).unwrap()),
                "order":[0,1],
                "waves":[[0,1]],
                "permissions_granted":false,
                "results_verified":false
            }),
        )
        .unwrap();

    dispatch(&store, &task, 2, "solver", 192, 240);
    dispatch(&store, &task, 3, "solver", 192, 240);
    for node in &mut subtasks {
        node.state = "running".into();
        state_event(&store, &task, node);
    }
    response(&store, &task, 2, "solver", 1, "first advice");
    response(&store, &task, 3, "solver", 2, "second advice");
    subtasks[0].state = "unverified".into();
    subtasks[0].result = Some("first advice".into());
    store
        .event(&task, "subtask", "jev-plus", json!({"node":subtasks[0]}))
        .unwrap();
    store
        .event(
            &task,
            "subtask_resources",
            "jev-plus",
            json!({
                "format_version":1,
                "node_id":"a",
                "state":"unverified",
                "calls_reserved":1,
                "reserved_output_tokens":192,
                "prompt_bytes":240,
                "input_tokens":20,
                "output_tokens":10,
                "elapsed_ms":1.0,
                "inference_ms":1.0,
                "queue_wait_ms":null,
                "result_verified":false,
                "inference_completed":true
            }),
        )
        .unwrap();
    subtasks[1].state = "running".into();
    subtasks[1].result = None;
    (store, task, plan, subtasks)
}

fn dispatch(store: &Store, task: &str, call: u64, role: &str, allocation: u64, bytes: u64) {
    store
        .event(
            task,
            "model_dispatch",
            "jev-r",
            json!({"call":call,"role":role,"allocation":allocation,"prompt_bytes":bytes}),
        )
        .unwrap();
}

fn response(store: &Store, task: &str, call: u64, role: &str, id: usize, text: &str) {
    let node = Node {
        id,
        parent: id.checked_sub(1),
        role: role.into(),
        cycle: 0,
        model: "fixture/v1".into(),
        content_hash: sha256(text.as_bytes()),
    };
    store
        .event(
            task,
            "reasoning",
            "jev-r",
            json!({
                "call":call,
                "node":node,
                "text":text,
                "input_tokens":20,
                "output_tokens":10,
                "inference_ms":1.0,
                "queue_wait_ms":null
            }),
        )
        .unwrap();
}

fn state_event(store: &Store, task: &str, node: &Subtask) {
    store
        .event(task, "subtask", "jev-plus", json!({"node":node}))
        .unwrap();
}

#[test]
fn reconciles_only_a_complete_charged_wave_suffix_and_preserves_advice_as_unverified() {
    let (store, task, plan, original) = fixture();
    let order = graph::order_from(&store.lock().unwrap(), &task, &plan).unwrap();
    let before: u64 = store
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM events WHERE task_id=?1",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    let checkpoint =
        reconcile_unpublished(&store.lock().unwrap(), &task, &plan, &order, &original).unwrap();
    assert_eq!(checkpoint.reconciled.len(), 1);
    assert_eq!(
        checkpoint.reconciled_indices,
        [1usize].into_iter().collect()
    );
    assert_eq!(checkpoint.subtasks[0].state, "unverified");
    assert_eq!(checkpoint.subtasks[1].state, "unverified");
    assert_eq!(
        checkpoint.subtasks[1].result.as_deref(),
        Some("second advice")
    );
    assert_eq!(checkpoint.subtasks[1].state, "unverified");
    assert_eq!(
        store
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE task_id=?1",
                [&task],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        before,
        "read-only reconciliation helper must not publish its reconstruction"
    );
    assert_eq!(
        inference::audit_from(&store.lock().unwrap(), &task).unwrap()["reserved_calls"],
        3
    );
}

#[test]
fn rejects_missing_duplicate_tampered_and_out_of_order_parallel_evidence() {
    for mutation in 0..8 {
        let (store, task, plan, mut original) = fixture();
        {
            let db = store.lock().unwrap();
            match mutation {
                0 => {
                    db.execute("DELETE FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=3", [&task]).unwrap();
                }
                1 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.prompt_bytes',32001) WHERE task_id=?1 AND kind='model_dispatch' AND json_extract(payload,'$.call')=3", [&task]).unwrap();
                }
                2 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.output_tokens',193) WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=3", [&task]).unwrap();
                }
                3 => {
                    db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=3", [&task]).unwrap();
                }
                4 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.prompt_bytes',241) WHERE task_id=?1 AND kind='subtask_resources'", [&task]).unwrap();
                }
                5 => {
                    db.execute("UPDATE events SET source='foreign' WHERE task_id=?1 AND kind='reasoning' AND json_extract(payload,'$.call')=3", [&task]).unwrap();
                }
                6 => {
                    db.execute("UPDATE events SET source='foreign' WHERE task_id=?1 AND kind='subtask_resources'", [&task]).unwrap();
                }
                _ => {
                    db.execute("INSERT INTO events(task_id,kind,source,payload,created) SELECT task_id,kind,source,payload,created FROM events WHERE task_id=?1 AND kind='subtask' AND json_extract(payload,'$.node.step.id')='b' AND json_extract(payload,'$.node.state')='running'", [&task]).unwrap();
                }
            }
        }
        if mutation == 4 {
            original[0].state = "running".into();
            original[0].result = None;
            original[1].state = "unverified".into();
            original[1].result = Some("second advice".into());
        }
        let order = graph::order_from(&store.lock().unwrap(), &task, &plan).unwrap();
        let err = reconcile_unpublished(&store.lock().unwrap(), &task, &plan, &order, &original);
        assert!(err.is_err(), "mutation {mutation} was accepted");
    }
}
