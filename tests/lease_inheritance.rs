//! A separate process image keeps this controlled fork out of parallel unit tests.
use cogfabric::store::Store;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, net::UnixStream},
    },
    time::Duration,
};
struct Child(libc::pid_t);
impl Drop for Child {
    fn drop(&mut self) {
        if self.0 > 0 {
            unsafe {
                libc::kill(self.0, libc::SIGKILL);
                libc::waitpid(self.0, std::ptr::null_mut(), 0);
            }
        }
    }
}
#[test]
fn fork_inheritance_retains_close_on_exec_lease_until_child_exits() {
    let root = std::env::temp_dir().join(format!("fabric-fork-lease-{}", uuid::Uuid::new_v4()));
    let path = root.join("state.db");
    let store = Store::open(&path).unwrap();
    let task = store
        .create_task("Inherited lease regression", "fast")
        .unwrap();
    let before = store.task(&task).unwrap();
    let lease = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(root.join("state.db.runtime.lock"))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) },
        0
    );
    assert_ne!(
        unsafe { libc::fcntl(lease.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    let (mut parent, child) = UnixStream::pair().unwrap();
    parent
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    child
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        // Only async-signal-safe syscalls after fork; no Rust/SQLite destructors.
        unsafe {
            libc::close(parent.as_raw_fd());
            let mut byte = 1u8;
            if libc::write(child.as_raw_fd(), (&byte as *const u8).cast(), 1) != 1 {
                libc::_exit(2);
            }
            if libc::read(child.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) != 1 {
                libc::_exit(3);
            }
            libc::_exit(0);
        }
    }
    let mut guard = Child(pid);
    drop(child);
    let mut ready = [0u8; 1];
    parent.read_exact(&mut ready).unwrap();
    assert_eq!(ready, [1]);
    drop(lease); // Only the forked child retains this open file description.
    let error = store.recover_tasks().unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(store.task(&task).unwrap(), before);
    parent.write_all(&[1]).unwrap();
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
    guard.0 = 0;
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
    assert_eq!(store.recover_tasks().unwrap(), vec![task]);
    assert!(store.recover_tasks().unwrap().is_empty());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
