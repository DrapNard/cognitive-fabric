//! Immutable compiled artifact and non-serializable single-invocation scope.
//! The embedding Rust caller must authenticate/approve grant construction.
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const READY: u8 = 0;
const RUNNING: u8 = 1;
const REVOKED: u8 = 2;
const CANCELLED: u8 = 3;
const COMPLETE: u8 = 4;
const MAX_GRANT_LIFETIME: Duration = Duration::from_secs(30);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub version: String,
    pub checksum: String,
}
#[derive(Clone)]
pub struct Artifact {
    identity: Identity,
    engine: wasmi::Engine,
    module: Arc<wasmi::Module>,
}
/// No serde implementation, public fields or Clone: untrusted payloads cannot
/// construct or reset this object. This is not remote-user authentication.
pub struct TaskGrant {
    task: String,
    identity: Identity,
    input: i64,
    state: Arc<AtomicU8>,
    deadline: Instant,
}
/// Cloneable cancellation authority can only reduce an existing grant.
#[derive(Clone)]
pub struct Revoker(Arc<AtomicU8>);
impl Revoker {
    pub fn revoke(&self) -> bool {
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| match state {
                READY => Some(REVOKED),
                RUNNING => Some(CANCELLED),
                _ => None,
            })
            .is_ok()
    }
}
impl TaskGrant {
    pub fn revoker(&self) -> Revoker {
        Revoker(self.state.clone())
    }
    fn finish(&self, result: Result<i64, String>) -> Result<i64, String> {
        if self
            .state
            .compare_exchange(RUNNING, COMPLETE, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            self.state.store(COMPLETE, Ordering::SeqCst);
            return Err("invocation grant revoked".into());
        }
        if Instant::now() >= self.deadline {
            return Err("invocation grant expired".into());
        }
        result
    }
}
impl Artifact {
    pub fn import(id: &str, version: &str, checksum: &str, bytes: &[u8]) -> Result<Self, String> {
        if !(3..=101).contains(&id.len())
            || !id.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        {
            return Err("invalid artifact identifier".into());
        }
        if version.len() > 64
            || semver::Version::parse(version)
                .map_err(|_| "invalid version")?
                .to_string()
                != version
        {
            return Err("noncanonical version".into());
        }
        if bytes.len() > 65536 {
            return Err("module byte limit".into());
        }
        if checksum.len() != 64
            || !checksum
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || format!("{:x}", Sha256::digest(bytes)) != checksum
        {
            return Err("artifact checksum mismatch".into());
        }
        let (engine, module) = crate::compile_checked(bytes)?;
        Ok(Self {
            identity: Identity {
                id: id.into(),
                version: version.into(),
                checksum: checksum.into(),
            },
            engine,
            module: Arc::new(module),
        })
    }
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    /// Only an authenticated/approved embedding may call this constructor.
    /// It grants one exact input, not cumulative task or registry authority.
    pub fn operator_grant(&self, task: &str, input: i64) -> Result<TaskGrant, String> {
        let deadline = Instant::now()
            .checked_add(MAX_GRANT_LIFETIME)
            .ok_or("invalid grant lifetime")?;
        self.operator_grant_until(task, input, deadline)
    }
    /// The embedding must narrow this to its original task deadline.
    pub fn operator_grant_until(
        &self,
        task: &str,
        input: i64,
        deadline: Instant,
    ) -> Result<TaskGrant, String> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("expired grant deadline")?;
        if remaining.is_zero() || remaining > MAX_GRANT_LIFETIME {
            return Err("invalid grant lifetime".into());
        }
        if uuid::Uuid::parse_str(task)
            .map_err(|_| "invalid task identity")?
            .to_string()
            != task
        {
            return Err("noncanonical task identity".into());
        }
        Ok(TaskGrant {
            task: task.into(),
            identity: self.identity.clone(),
            input,
            state: Arc::new(AtomicU8::new(READY)),
            deadline,
        })
    }
    pub fn invoke(
        &self,
        grant: &mut TaskGrant,
        task: &str,
        actor: &str,
        input: i64,
    ) -> Result<i64, String> {
        if task != grant.task
            || actor != "human-cli"
            || self.identity != grant.identity
            || input != grant.input
        {
            return Err("invocation outside grant scope".into());
        }
        if Instant::now() >= grant.deadline {
            return Err("invocation grant expired".into());
        }
        // Atomic admission orders cancellation against dispatch. Admitted work
        // remains consumed even if it traps or cancellation suppresses its result.
        grant
            .state
            .compare_exchange(READY, RUNNING, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|state| {
                if state == REVOKED {
                    "invocation grant revoked"
                } else {
                    "invocation grant already consumed"
                }
            })?;
        let result = crate::call_checked(&self.engine, &self.module, input);
        grant.finish(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const TASK: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER: &str = "22222222-2222-4222-8222-222222222222";
    fn bytes(body: &str) -> Vec<u8> {
        wat::parse_str(format!(
            "(module (func (export \"evaluate\") (param i64) (result i64) {body}))"
        ))
        .unwrap()
    }
    fn import(version: &str, bytes: &[u8]) -> Artifact {
        Artifact::import(
            "math.demo.s1",
            version,
            &format!("{:x}", Sha256::digest(bytes)),
            bytes,
        )
        .unwrap()
    }
    #[test]
    fn scope_denials_replay_and_traps_preserve_one_invocation() {
        let artifact = import("1.0.0", &bytes("local.get 0 i64.const 2 i64.mul"));
        let mut grant = artifact.operator_grant(TASK, 21).unwrap();
        assert!(artifact.invoke(&mut grant, OTHER, "human-cli", 21).is_err());
        assert!(artifact.invoke(&mut grant, TASK, "jev-r", 21).is_err());
        assert!(artifact.invoke(&mut grant, TASK, "human-cli", 22).is_err());
        assert_eq!(
            artifact.invoke(&mut grant, TASK, "human-cli", 21).unwrap(),
            42
        );
        assert!(artifact.invoke(&mut grant, TASK, "human-cli", 21).is_err());
        let trap = import("2.0.0", &bytes("unreachable"));
        let mut grant = trap.operator_grant(TASK, 0).unwrap();
        assert!(trap.invoke(&mut grant, TASK, "human-cli", 0).is_err());
        assert_eq!(
            trap.invoke(&mut grant, TASK, "human-cli", 0).unwrap_err(),
            "invocation grant already consumed"
        );
    }
    #[test]
    fn version_and_content_are_pinned_after_source_changes() {
        let mut original = bytes("local.get 0 i64.const 1 i64.add");
        let old = import("1.0.0", &original);
        let mut grant = old.operator_grant(TASK, 21).unwrap();
        original.fill(0);
        let new = import("2.0.0", &bytes("local.get 0 i64.const 2 i64.add"));
        assert!(new.invoke(&mut grant, TASK, "human-cli", 21).is_err());
        let same_version = import("1.0.0", &bytes("local.get 0 i64.const 3 i64.add"));
        assert!(same_version
            .invoke(&mut grant, TASK, "human-cli", 21)
            .is_err());
        assert_eq!(
            old.clone()
                .invoke(&mut grant, TASK, "human-cli", 21)
                .unwrap(),
            22
        );
    }
    #[test]
    fn fresh_state_and_fuel_traps_do_not_reuse_authority() {
        let state=wat::parse_str("(module (global $counter (mut i64) (i64.const 0)) (func (export \"evaluate\") (param i64) (result i64) global.get $counter i64.const 1 i64.add global.set $counter global.get $counter))").unwrap();
        let artifact = import("1.0.0", &state);
        for task in [TASK, OTHER] {
            let mut grant = artifact.operator_grant(task, 0).unwrap();
            assert_eq!(
                artifact.invoke(&mut grant, task, "human-cli", 0).unwrap(),
                1
            );
        }
        let looping = import("2.0.0", &bytes("(loop $l br $l) i64.const 0"));
        let mut grant = looping.operator_grant(TASK, 0).unwrap();
        assert!(looping
            .invoke(&mut grant, TASK, "human-cli", 0)
            .unwrap_err()
            .contains("fuel"));
        assert_eq!(
            looping
                .invoke(&mut grant, TASK, "human-cli", 0)
                .unwrap_err(),
            "invocation grant already consumed"
        );
    }

    #[test]
    fn revocation_and_expiry_prevent_admission() {
        let artifact = import("1.0.0", &bytes("(loop $l br $l) i64.const 0"));
        let mut grant = artifact.operator_grant(TASK, 0).unwrap();
        let revoker = grant.revoker();
        assert!(revoker.clone().revoke());
        assert!(!revoker.revoke());
        assert_eq!(
            artifact
                .invoke(&mut grant, TASK, "human-cli", 0)
                .unwrap_err(),
            "invocation grant revoked"
        );
        assert_eq!(grant.state.load(Ordering::SeqCst), REVOKED);
        let mut expired = artifact.operator_grant(TASK, 0).unwrap();
        // Simulate time passing without sleeps; public callers cannot alter it.
        expired.deadline = Instant::now();
        assert_eq!(
            artifact
                .invoke(&mut expired, TASK, "human-cli", 0)
                .unwrap_err(),
            "invocation grant expired"
        );
        assert_eq!(expired.state.load(Ordering::SeqCst), READY);
        assert!(artifact
            .operator_grant_until(TASK, 0, Instant::now())
            .is_err());
        assert!(artifact
            .operator_grant_until(TASK, 0, Instant::now() + Duration::from_secs(60))
            .is_err());
    }
    #[test]
    fn cloned_revoker_cancels_queued_work_on_another_thread() {
        let artifact = import("1.0.0", &bytes("(loop $l br $l) i64.const 0"));
        let mut grant = artifact.operator_grant(TASK, 0).unwrap();
        let revoker = grant.revoker();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = barrier.clone();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            let result = artifact.invoke(&mut grant, TASK, "human-cli", 0);
            (result, grant.state.load(Ordering::SeqCst))
        });
        assert!(revoker.revoke());
        barrier.wait();
        let (result, state) = worker.join().unwrap();
        assert_eq!(result.unwrap_err(), "invocation grant revoked");
        assert_eq!(state, REVOKED, "Queued cancellation must prevent admission");
    }

    #[test]
    fn cancellation_after_admission_suppresses_result_and_remains_consumed() {
        let artifact = import("1.0.0", &bytes("local.get 0"));
        let mut grant = artifact.operator_grant(TASK, 42).unwrap();
        // Exercise the terminal state transition independently of scheduler timing.
        grant.state.store(RUNNING, Ordering::SeqCst);
        assert!(grant.revoker().revoke());
        assert_eq!(grant.state.load(Ordering::SeqCst), CANCELLED);
        assert_eq!(
            grant.finish(Ok(42)).unwrap_err(),
            "invocation grant revoked"
        );
        assert_eq!(grant.state.load(Ordering::SeqCst), COMPLETE);
        assert!(!grant.revoker().revoke());
        assert_eq!(
            artifact
                .invoke(&mut grant, TASK, "human-cli", 42)
                .unwrap_err(),
            "invocation grant already consumed"
        );
        let mut expired = artifact.operator_grant(TASK, 42).unwrap();
        expired.state.store(RUNNING, Ordering::SeqCst);
        expired.deadline = Instant::now();
        assert_eq!(
            expired.finish(Ok(42)).unwrap_err(),
            "invocation grant expired"
        );
        assert_eq!(expired.state.load(Ordering::SeqCst), COMPLETE);
    }

    #[test]
    fn invalid_identity_hash_and_semantics_never_create_artifacts() {
        let body = bytes("local.get 0");
        let hash = format!("{:x}", Sha256::digest(&body));
        for (id, version, checksum) in [
            ("../escape", "1.0.0", hash.as_str()),
            ("math.demo.s1", "1", hash.as_str()),
            ("math.demo.s1", "1.0.0", "bad"),
        ] {
            assert!(Artifact::import(id, version, checksum, &body).is_err());
        }
        assert!(Artifact::import("math.demo.s1", "1.0.0", &"0".repeat(64), &body).is_err());
        let invalid = bytes("i32.const 1");
        assert!(Artifact::import(
            "math.demo.s1",
            "1.0.0",
            &format!("{:x}", Sha256::digest(&invalid)),
            &invalid
        )
        .is_err());
        assert!(import("1.0.0", &body)
            .operator_grant("not-a-task", 0)
            .is_err());
    }
}
