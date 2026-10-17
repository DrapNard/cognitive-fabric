//! Recovery must preserve the cause and perform no writes after failed ownership.
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

async fn errors(store: &Store, shared: std::sync::Arc<Store>, task: &str) -> Vec<anyhow::Error> {
    vec![
        store.recover_tasks().expect_err("Task recovery must deny"),
        store
            .recover_workers()
            .expect_err("Worker recovery must deny"),
        store
            .recover_verified_result(task)
            .expect_err("Result recovery must deny"),
        store
            .recover_advisory_result(task, true)
            .expect_err("Advice recovery must deny"),
        store
            .recover_revision_result(task, true)
            .expect_err("Revision recovery must deny"),
        continuation::preview(store, task).expect_err("Preview must deny"),
        continuation::resume(shared, task, true, std::future::pending())
            .await
            .expect_err("Continuation must deny"),
    ]
}
#[tokio::test]
async fn recovery_distinguishes_contention_unsafe_files_and_os_errors_without_mutation() {
    let root = std::env::temp_dir().join(format!("fabric-lease-errors-{}", uuid::Uuid::new_v4()));
    let store = std::sync::Arc::new(Store::open(&root.join("state.db")).unwrap());
    let task = store
        .create_task("Inspect ownership failure", "fast")
        .unwrap();
    let before = store.task(&task).unwrap();
    let lease = store.execution_lease().unwrap().unwrap();
    for error in errors(&store, store.clone(), &task).await {
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(error.to_string().contains("exclusive runtime lease"));
    }
    assert_eq!(store.task(&task).unwrap(), before);
    drop(lease);
    let path = store.runtime_lock.as_ref().unwrap();
    std::fs::remove_file(path).unwrap();
    let target = root.join("target");
    std::fs::write(&target, "Must remain unchanged").unwrap();
    symlink(&target, path).unwrap();
    for error in errors(&store, store.clone(), &task).await {
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(libc::ELOOP)
        );
        assert!(error.to_string().contains("exclusive runtime lease"));
    }
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "Must remain unchanged"
    );
    assert_eq!(store.task(&task).unwrap(), before);
    std::fs::remove_file(path).unwrap();
    std::fs::write(path, "").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    for error in errors(&store, store.clone(), &task).await {
        assert!(error
            .root_cause()
            .to_string()
            .contains("private owner-controlled regular file"));
        assert!(error.downcast_ref::<std::io::Error>().is_none());
    }
    assert_eq!(store.task(&task).unwrap(), before);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
