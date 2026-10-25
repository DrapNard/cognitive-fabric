use super::*;

fn database_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "cogfabric-agent-session-migration-{}.db",
        Uuid::new_v4()
    ))
}

fn create_schema_nine(path: &Path) -> String {
    let store = Store::open(path).unwrap();
    let task_id = store
        .create_task("preserve pre-session task", "fast")
        .unwrap();
    store
        .finish_task(&task_id, "completed", "preserved")
        .unwrap();
    {
        let db = store.lock().unwrap();
        db.execute_batch(
            "DROP INDEX agent_session_calls_generation;
             DROP TABLE agent_session_calls;
             DROP TABLE agent_sessions;
             PRAGMA user_version=9;",
        )
        .unwrap();
    }
    drop(store);
    task_id
}

#[test]
fn schema_ten_migration_preserves_existing_tasks_and_cascades_session_rows() {
    let path = database_path();
    let task_id = create_schema_nine(&path);

    let store = Store::open(&path).unwrap();
    let version: i64 = store
        .lock()
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 10);
    assert_eq!(store.task(&task_id).unwrap()["task"]["answer"], "preserved");

    let session_id = Uuid::new_v4().to_string();
    let call_id = format!("cgf_{}", "a".repeat(32));
    {
        let db = store.lock().unwrap();
        let tx = db
            .unchecked_transaction()
            .expect("migration regression fixture transaction");
        tx.execute(
            "INSERT INTO agent_sessions(id,task_id,state,generation,claim,snapshot,snapshot_sha256,updated)
             VALUES (?1,?2,'waiting',1,?3,'{}',?4,1.0)",
            params![session_id, task_id, Uuid::new_v4().to_string(), "b".repeat(64)],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO agent_session_calls(id,session_id,generation,call_sha256)
             VALUES (?1,?2,1,?3)",
            params![call_id, session_id, "c".repeat(64)],
        )
        .unwrap();
        tx.commit().unwrap();
        db.execute("DELETE FROM tasks WHERE id=?1", [&task_id])
            .unwrap();
        let sessions: u64 = db
            .query_row("SELECT count(*) FROM agent_sessions", [], |row| row.get(0))
            .unwrap();
        let calls: u64 = db
            .query_row("SELECT count(*) FROM agent_session_calls", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(sessions, 0);
        assert_eq!(calls, 0);
    }

    drop(store);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn schema_ten_migration_rolls_back_fully_on_conflicting_index() {
    let path = database_path();
    let task_id = create_schema_nine(&path);
    {
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE INDEX agent_session_calls_generation ON events(task_id);
             PRAGMA user_version=9;",
        )
        .unwrap();
    }

    assert!(Store::open(&path).is_err());
    let db = Connection::open(&path).unwrap();
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 9);
    let sessions_exist: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='agent_sessions')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!sessions_exist);
    assert_eq!(
        db.query_row("SELECT answer FROM tasks WHERE id=?1", [&task_id], |row| {
            row.get::<_, String>(0)
        })
        .unwrap(),
        "preserved"
    );

    drop(db);
    std::fs::remove_file(&path).unwrap();
}
