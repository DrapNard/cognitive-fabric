//! Pinned lifecycle policy loading must not block on special filesystem inputs.
use cogfabric::residency::lifecycle::NativeLifecyclePolicy;
use serde_json::json;
use std::{os::unix::ffi::OsStrExt, path::PathBuf, time::Duration};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("fabric-native-policy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn fifo_policy_refuses_without_a_writer_or_runtime_initialization() {
    let root = Root::new();
    let policy = root.0.join("policy.fifo");
    let path = std::ffi::CString::new(policy.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let state = root.0.join("state");
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_cogfabric"));
    command
        .arg("--home")
        .arg(&state)
        .arg("--native-lifecycle-policy")
        .arg(&policy)
        .args(["--approve-native-lifecycle", "ask", "calc:1+1"])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .expect("A special policy input must not block waiting for a writer")
        .unwrap();
    assert!(!output.status.success());
    assert!(!state.exists());
}

#[test]
fn policy_loading_preserves_strict_pins_and_bounded_regular_files() {
    let root = Root::new();
    let file = root.0.join("policy.json");
    let value = json!({"format_version":1,"api_base":"http://127.0.0.1:1234/api/v1",
        "models":[{"model":"fixture","context_length":4096}],"max_lifecycle_seconds":30});
    std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    let loaded = NativeLifecyclePolicy::load(&file).unwrap();
    assert_eq!(loaded.models[0].model, "fixture");
    let link = root.0.join("substitution.json");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    assert!(NativeLifecyclePolicy::load(link).is_err());
    assert!(NativeLifecyclePolicy::load(&root.0).is_err());
    std::fs::write(&file, b"{\"format_version\":1,\"format_version\":1}").unwrap();
    assert!(NativeLifecyclePolicy::load(&file).is_err());
    std::fs::write(&file, vec![b' '; 65_537]).unwrap();
    assert!(NativeLifecyclePolicy::load(&file).is_err());
}
