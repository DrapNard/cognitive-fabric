//! Operator maintenance must work even when normal initialization cannot write.
use rusqlite::Connection;
use serde_json::Value;
use std::{path::Path, process::Command};

fn run(home: &Path, command: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_cogfabric"))
        .arg("--home")
        .arg(home)
        .args(["--json", command])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn operator_can_inspect_and_checkpoint_an_oversized_wal_without_bootstrap_writes() {
    let home = std::env::temp_dir().join(format!("fabric-wal-cli-{}", uuid::Uuid::new_v4()));
    run(&home, "init");
    let cfg = cogfabric::config::Config::load(Some(home.clone()), None).unwrap();
    let writer = Connection::open(cfg.db()).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE external_wal_fixture(body BLOB); PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    let reader = Connection::open(cfg.db()).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: u64 = reader
        .query_row("SELECT count(*) FROM external_wal_fixture", [], |r| {
            r.get(0)
        })
        .unwrap();
    for _ in 0..36 {
        writer
            .execute(
                "INSERT INTO external_wal_fixture VALUES(zeroblob(500000))",
                [],
            )
            .unwrap();
    }
    let status = run(&home, "storage-status");
    assert_eq!(status["wal_hard_limit"], true);
    assert!(status["wal_bytes"].as_u64().unwrap() > status["wal_limit_bytes"].as_u64().unwrap());
    assert!(status["maximum_bytes"].as_u64().unwrap() <= cfg.max_database_bytes);
    let blocked = run(&home, "storage-checkpoint");
    assert_eq!(blocked["busy"], true);
    assert_eq!(blocked["completed"], false);
    let count: u64 = reader
        .query_row("SELECT count(*) FROM external_wal_fixture", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    let recovered = run(&home, "storage-checkpoint");
    assert_eq!(recovered["completed"], true);
    assert_eq!(recovered["readers_evicted"], false);
    assert_eq!(recovered["tasks_replayed"], false);
    assert_eq!(run(&home, "storage-status")["wal_bytes"], 0);
    assert_eq!(
        writer
            .query_row("SELECT count(*) FROM external_wal_fixture", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        36
    );
    drop(writer);
    std::fs::remove_dir_all(home).unwrap();
}
