use super::*;
use crate::{
    config::Config,
    scheduling::{StableScheduler, SubtaskScheduler},
};

fn fixture() -> (Store, String, Plan) {
    let store = Store::in_memory().unwrap();
    let task = store.create_task("private goal", "deep").unwrap();
    let cfg = Config {
        subtask_parallelism: Some(2),
        ..Config::default()
    };
    store
        .event(
            &task,
            "execution_settings",
            "runtime",
            json!({"format_version":1,"configuration":cfg}),
        )
        .unwrap();
    let plan = Plan::parse(r#"{"steps":[{"id":"a","objective":"private first","depends_on":[]},{"id":"b","objective":"private second","depends_on":[]},{"id":"c","objective":"private dependent","depends_on":["a"]},{"id":"d","objective":"private dependent","depends_on":["b"]}]}"#).unwrap();
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
    store.event(&task,"subtask_schedule","jev-plus",json!({"format_version":1,"scheduler":scheduler,"graph_sha256":sha256(&serde_json::to_vec(&plan).unwrap()),"order":[0,1,2,3],"permissions_granted":false})).unwrap();
    for index in 0..4 {
        node(&store, &task, &plan, index, "pending");
    }
    marker(&store, &task, &plan);
    (store, task, plan)
}
fn node(store: &Store, task: &str, plan: &Plan, index: usize, state: &str) {
    store.event(task,"subtask","jev-plus",json!({"node":Subtask{step:plan.steps[index].clone(),state:state.into(),result:(state=="unverified").then(||"private result".into())}})).unwrap();
}
fn marker(store: &Store, task: &str, plan: &Plan) {
    store.event(task,"subtask_parallel_schedule","jev-plus",json!({"format_version":1,"max_parallelism":2,"graph_sha256":sha256(&serde_json::to_vec(plan).unwrap()),"order":[0,1,2,3],"waves":[[0,1],[2,3]],"permissions_granted":false,"results_verified":false})).unwrap();
}

#[test]
fn observes_bounded_parallel_waves_without_exposing_bodies_or_authorizing_replay() {
    let (store, task, plan) = fixture();
    for wave in [[0, 1], [2, 3]] {
        for index in wave {
            node(&store, &task, &plan, index, "running");
        }
        let report = store.graph_audit(&task).unwrap();
        assert!(!report.to_string().contains("private"));
        assert_eq!(report["resume_authorized"], false);
        for index in wave.into_iter().rev() {
            node(&store, &task, &plan, index, "unverified");
        }
    }
    let report = store.graph_audit(&task).unwrap();
    assert!(report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|node| node["state"] == "unverified" && node["result_verified"] == false));
}

#[test]
fn refuses_changed_ambiguous_unbounded_or_unpinned_parallel_schedule() {
    for variant in 0..14 {
        let (store, task, plan) = fixture();
        {
            let db = store.lock().unwrap();
            match variant {
                0 => {
                    db.execute("UPDATE events SET source='model' WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                1 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.format_version',2) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                2 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.permissions_granted',json('true')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                3 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.results_verified',json('true')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                4 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.max_parallelism',5) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                5 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.waves',json('[[0,2],[1,3]]')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                6 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.order',json('[1,0,2,3]')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                7 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.graph_sha256','changed') WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                8 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.extra',0) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                9 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.max_parallelism',3,'$.waves',json('[[0,1],[2,3]]')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                10 => {
                    db.execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='execution_settings'",
                        [&task],
                    )
                    .unwrap();
                }
                11 => {
                    db.execute("UPDATE events SET payload=json_set(payload,'$.waves',json('[[0,1],[2,2,3]]')) WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                12 => {
                    db.execute("UPDATE events SET payload='{}' WHERE task_id=?1 AND kind='subtask_parallel_schedule'",[&task]).unwrap();
                }
                _ => {}
            }
        }
        if variant == 13 {
            marker(&store, &task, &plan);
        }
        assert!(store.graph_audit(&task).is_err(), "variant {variant}");
    }
}

#[test]
fn refuses_unadmitted_overlap_premature_results_and_unfinished_wave_crossing() {
    for variant in 0..4 {
        let (store, task, plan) = fixture();
        node(&store, &task, &plan, 0, "running");
        match variant {
            0 => {
                node(&store, &task, &plan, 0, "unverified");
                node(&store, &task, &plan, 1, "running");
            }
            1 => {
                node(&store, &task, &plan, 1, "running");
                node(&store, &task, &plan, 0, "unverified");
                node(&store, &task, &plan, 2, "running");
            }
            2 => {
                store
                    .lock()
                    .unwrap()
                    .execute(
                        "DELETE FROM events WHERE task_id=?1 AND kind='subtask_parallel_schedule'",
                        [&task],
                    )
                    .unwrap();
                node(&store, &task, &plan, 1, "running");
            }
            _ => {
                node(&store, &task, &plan, 2, "running");
            }
        }
        assert!(store.graph_audit(&task).is_err(), "variant {variant}");
    }
}

#[test]
fn refusal_to_continue_parallel_history_is_read_only_even_when_marker_is_malformed() {
    let cfg = Config {
        home: std::env::temp_dir()
            .join(format!("fabric-parallel-recovery-{}", uuid::Uuid::new_v4())),
        ..Config::default()
    };
    cfg.save().unwrap();
    let store = Store::open(&cfg.db()).unwrap();
    let task = store.create_task("private original", "deep").unwrap();
    store
        .event(
            &task,
            "subtask_parallel_schedule",
            "foreign",
            json!({"malformed":true}),
        )
        .unwrap();
    store
        .lock()
        .unwrap()
        .execute("UPDATE tasks SET status='interrupted' WHERE id=?1", [&task])
        .unwrap();
    let before = store.task(&task).unwrap();
    let error = crate::recovery::continuation::preview(&store, &task).unwrap_err();
    assert!(error.to_string().contains("Parallel advisory"), "{error:#}");
    assert_eq!(store.task(&task).unwrap(), before);
    std::fs::remove_dir_all(&cfg.home).unwrap();
}
