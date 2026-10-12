use super::*;
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
};
struct Fixture {
    store: Store,
    task: String,
    home: PathBuf,
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let home =
            std::env::temp_dir().join(format!("fabric-cleanup-test-{}", uuid::Uuid::new_v4()));
        let store = Store::open(&home.join("state.db")).unwrap();
        let task = store.create_task("cleanup fixture", "fast").unwrap();
        let (guard, listener, socket) =
            crate::fabric::unix::process::SocketDirectory::bind_tool().unwrap();
        guard.record(&store, &task).unwrap();
        let directory = socket.parent().unwrap().to_path_buf();
        drop(listener);
        // Simulate a missing destructor after a crash; no live process is created.
        std::mem::forget(guard);
        Self {
            store,
            task,
            home,
            directory,
        }
    }
    fn plan(&self) -> CleanupPlan {
        self.store.preview_worker_files(0, 128).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}
#[tokio::test]
async fn reviewed_cleanup_is_audited_and_idempotent() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    assert_eq!(plan.entries.len(), 1);
    assert!(fixture.store.clean_worker_files(&plan, false).is_err());
    assert!(fixture.directory.exists());
    assert_eq!(fixture.store.clean_worker_files(&plan, true).unwrap(), 1);
    assert!(!fixture.directory.exists());
    assert!(fixture.store.clean_worker_files(&plan, true).is_err());
    let empty = fixture.plan();
    assert!(empty.entries.is_empty());
    assert_eq!(fixture.store.clean_worker_files(&empty, true).unwrap(), 0);
    let trace = fixture.store.task(&fixture.task).unwrap();
    assert_eq!(trace["task"]["status"], "running");
    let states: Vec<_> = trace["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "worker_socket_cleanup")
        .map(|e| e["payload"]["state"].as_str().unwrap())
        .collect();
    assert_eq!(states, ["intent", "removed"]);
    assert!(security::authorize("jev-plus", "worker.cleanup").is_err());
    assert!(security::authorize("jev-r", "worker.cleanup").is_err());
}
#[tokio::test]
async fn leases_cross_workspace_and_tampered_plans_prevent_deletion() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    let lease = fixture.store.execution_lease().unwrap();
    assert!(fixture.store.preview_worker_files(0, 128).is_err());
    assert!(fixture.store.clean_worker_files(&plan, true).is_err());
    drop(lease);
    let other = Fixture::new();
    assert!(other.store.clean_worker_files(&plan, true).is_err());
    let mut forged = plan.clone();
    forged.entries[0].resource.name = "cogfabric-worker-../../sensitive".into();
    assert!(fixture.store.clean_worker_files(&forged, true).is_err());
    forged.checksum = forged.digest().unwrap();
    assert!(fixture.store.clean_worker_files(&forged, true).is_err());
    assert!(fixture.directory.exists());
    assert!(other.directory.exists());
}
#[tokio::test]
async fn symlinks_regular_files_extra_entries_and_permissions_are_denied() {
    for case in 0..4 {
        let fixture = Fixture::new();
        let plan = fixture.plan();
        let socket = fixture.directory.join("worker.sock");
        match case {
            0 => {
                std::fs::remove_file(&socket).unwrap();
                symlink(fixture.home.join("state.db"), &socket).unwrap();
            }
            1 => {
                std::fs::remove_file(&socket).unwrap();
                std::fs::write(&socket, "preserve me").unwrap();
            }
            2 => std::fs::write(fixture.directory.join("important.txt"), "preserve me").unwrap(),
            _ => {
                std::fs::set_permissions(&fixture.directory, std::fs::Permissions::from_mode(0o755))
                    .unwrap()
            }
        }
        assert!(fixture.store.clean_worker_files(&plan, true).is_err());
        assert!(fixture.directory.exists());
        assert!(socket.symlink_metadata().is_ok());
    }
}
#[tokio::test]
async fn replacement_directory_and_missing_socket_require_fresh_review() {
    let fixture = Fixture::new();
    let plan = fixture.plan();
    std::fs::remove_file(fixture.directory.join("worker.sock")).unwrap();
    assert!(fixture.store.clean_worker_files(&plan, true).is_err());
    let fresh = fixture.plan();
    assert!(!fresh.entries[0].socket_present);
    assert_eq!(fixture.store.clean_worker_files(&fresh, true).unwrap(), 1);
    let fixture = Fixture::new();
    let plan = fixture.plan();
    let moved = fixture.home.join("original-directory");
    std::fs::rename(&fixture.directory, &moved).unwrap();
    std::fs::create_dir(&fixture.directory).unwrap();
    std::fs::set_permissions(&fixture.directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(fixture.store.clean_worker_files(&plan, true).is_err());
    assert!(moved.join("worker.sock").exists());
}
#[tokio::test]
async fn audit_failure_prevents_removal_and_post_removal_failure_is_explicit() {
    for state in ["intent", "removed"] {
        let fixture = Fixture::new();
        let plan = fixture.plan();
        fixture.store.lock().unwrap().execute_batch(&format!("CREATE TRIGGER fail_cleanup BEFORE INSERT ON events WHEN NEW.kind='worker_socket_cleanup' AND json_extract(NEW.payload,'$.state')='{state}' BEGIN SELECT RAISE(ABORT,'audit failure'); END;")).unwrap();
        assert!(fixture.store.clean_worker_files(&plan, true).is_err());
        assert_eq!(fixture.directory.exists(), state == "intent");
        if state == "removed" {
            assert!(fixture.plan().entries.is_empty());
        }
    }
}
#[tokio::test]
async fn pagination_advances_over_normally_removed_resources() {
    let fixture = Fixture::new();
    let (guard, listener, _) = crate::fabric::unix::process::SocketDirectory::bind_tool().unwrap();
    guard.record(&fixture.store, &fixture.task).unwrap();
    drop(listener);
    drop(guard);
    let first = fixture.store.preview_worker_files(0, 1).unwrap();
    assert!(first.more);
    assert_eq!(first.entries.len(), 1);
    fixture.store.clean_worker_files(&first, true).unwrap();
    let second = fixture.store.preview_worker_files(first.cursor, 1).unwrap();
    assert!(!second.more);
    assert!(second.entries.is_empty());
    assert!(second.cursor > first.cursor);
    let terminal = fixture
        .store
        .preview_worker_files(second.cursor, 1)
        .unwrap();
    assert_eq!(terminal.cursor, second.cursor);
    assert!(fixture.store.preview_worker_files(0, 129).is_err());
}

#[test]
fn crashed_host_fixture() {
    let Some(home) = std::env::var_os("COGFABRIC_TEST_CLEANUP_HOME") else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let home = PathBuf::from(home);
        let store = Store::open(&home.join("state.db")).unwrap();
        let _lease = store.execution_lease().unwrap();
        let task = store
            .create_task("crashed host cleanup fixture", "fast")
            .unwrap();
        let (_guard, _listener, _socket) =
            crate::fabric::unix::process::SocketDirectory::bind_tool().unwrap();
        _guard.record(&store, &task).unwrap();
        std::fs::write(home.join("ready"), "ready").unwrap();
        std::future::pending::<()>().await;
    });
}
#[tokio::test]
async fn actual_host_death_releases_lease_and_allows_reviewed_cleanup() {
    let home = std::env::temp_dir().join(format!("fabric-cleanup-crash-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&home).unwrap();
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "recovery::worker_files::tests::crashed_host_fixture",
        ])
        .env("COGFABRIC_TEST_CLEANUP_HOME", &home)
        .stdout(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !home.join("ready").exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "Fixture exited before creating its socket"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let store = Store::open(&home.join("state.db")).unwrap();
    assert!(store.preview_worker_files(0, 128).is_err());
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    let plan = store.preview_worker_files(0, 128).unwrap();
    assert_eq!(plan.entries.len(), 1);
    let directory = PathBuf::from("/tmp").join(&plan.entries[0].resource.name);
    assert!(directory.join("worker.sock").exists());
    assert_eq!(store.clean_worker_files(&plan, true).unwrap(), 1);
    assert!(!directory.exists());
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn invalid_oversized_and_duplicate_journals_fail_before_deletion() {
    for case in 0..6 {
        let fixture = Fixture::new();
        let plan = fixture.plan();
        let mut payload = serde_json::to_value(&plan.entries[0].resource).unwrap();
        match case {
            0 => payload["name"] = serde_json::json!("cogfabric-worker-../../outside"),
            1 => payload["uid"] = serde_json::json!(unsafe { libc::geteuid() }.wrapping_add(1)),
            2 => payload["format_version"] = serde_json::json!(2),
            3 => payload["padding"] = serde_json::json!("x".repeat(4096)),
            4 => payload["socket_inode"] = serde_json::json!(0),
            _ => {
                fixture
                    .store
                    .event(&fixture.task, "worker_socket", "fabric", payload.clone())
                    .unwrap();
            }
        }
        if case != 5 {
            fixture
                .store
                .lock()
                .unwrap()
                .execute(
                    "UPDATE events SET payload=?1 WHERE kind='worker_socket'",
                    [payload.to_string()],
                )
                .unwrap();
        }
        assert!(fixture.store.preview_worker_files(0, 128).is_err());
        assert!(fixture.store.clean_worker_files(&plan, true).is_err());
        assert!(fixture.directory.join("worker.sock").exists());
    }
}
