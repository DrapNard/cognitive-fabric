//! One trusted embedding authority per task; no serialization or duplication API.
use crate::artifact::{validate_scope, Artifact, Identity, Revoker, TaskGrant};
use crate::audit::{Audit, Kind};
use std::{sync::Arc, time::Instant};
pub struct TaskAuthority {
    task: String,
    deadline: Instant,
    allowed: Vec<Identity>,
    maximum: usize,
    issued: Vec<Revoker>,
    revoked: bool,
    audit: Arc<dyn Audit>,
}
impl TaskAuthority {
    /// The embedding authenticates/approves this constructor and must prevent
    /// issuing multiple independent authorities to reset a task's allowance.
    pub fn operator(
        task: &str,
        deadline: Instant,
        maximum: usize,
        allowed: &[&Artifact],
        audit: Arc<dyn Audit>,
    ) -> Result<Self, String> {
        validate_scope(task, deadline)?;
        if !(1..=8).contains(&maximum) || allowed.is_empty() || allowed.len() > 8 {
            return Err("invalid task authority bounds".into());
        }
        let mut identities = Vec::new();
        for artifact in allowed {
            if identities
                .iter()
                .any(|identity: &Identity| identity.id == artifact.identity().id)
            {
                return Err("ambiguous artifact set".into());
            }
            identities.push(artifact.identity().clone());
        }
        Ok(Self {
            task: task.into(),
            deadline,
            allowed: identities,
            maximum,
            issued: Vec::new(),
            revoked: false,
            audit,
        })
    }
    pub fn remaining(&self) -> usize {
        self.maximum - self.issued.len()
    }
    pub fn grant(&mut self, artifact: &Artifact, input: i64) -> Result<TaskGrant, String> {
        if self.revoked {
            return Err("task authority revoked".into());
        }
        if Instant::now() >= self.deadline {
            return Err("task authority expired".into());
        }
        if !self.allowed.contains(artifact.identity()) {
            return Err("artifact outside task authority".into());
        }
        if self.remaining() == 0 {
            return Err("task invocation allowance exhausted".into());
        }
        let grant = artifact.issue_grant(
            &self.task,
            input,
            self.deadline,
            self.audit.clone(),
            self.issued.len() + 1,
        )?;
        // Allocation is conservative: unused, revoked and trapped grants are not
        // refunded. Issuance is serialized by the mutable authority reference.
        self.issued.push(grant.revoker());
        if let Err(error) = grant.record(Kind::Allocated) {
            grant.revoker().revoke();
            return Err(error);
        }
        Ok(grant)
    }
    pub fn revoke(&mut self) {
        self.revoked = true;
        for grant in &self.issued {
            grant.revoke();
        }
    }
}
#[cfg(test)]
impl TaskAuthority {
    fn operator_for_test(
        task: &str,
        deadline: Instant,
        maximum: usize,
        allowed: &[&Artifact],
    ) -> Result<Self, String> {
        Self::operator(
            task,
            deadline,
            maximum,
            allowed,
            Arc::new(crate::audit::MemoryAudit::default()),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::time::Duration;
    const TASK: &str = "11111111-1111-4111-8111-111111111111";
    fn artifact(version: &str, body: &str) -> Artifact {
        let bytes = wat::parse_str(format!(
            "(module (func (export \"evaluate\") (param i64) (result i64) {body}))"
        ))
        .unwrap();
        Artifact::import(
            "math.demo.s1",
            version,
            &format!("{:x}", Sha256::digest(&bytes)),
            &bytes,
        )
        .unwrap()
    }
    #[test]
    fn allowance_is_shared_across_inputs_and_not_refunded() {
        let artifact = artifact("1.0.0", "local.get 0");
        let mut authority = TaskAuthority::operator_for_test(
            TASK,
            Instant::now() + Duration::from_secs(10),
            2,
            &[&artifact],
        )
        .unwrap();
        let mut first = authority.grant(&artifact, 7).unwrap();
        let second = authority.grant(&artifact, 8).unwrap();
        assert_eq!(authority.remaining(), 0);
        assert!(authority.grant(&artifact, 9).is_err());
        assert_eq!(
            artifact.invoke(&mut first, TASK, "human-cli", 7).unwrap(),
            7
        );
        assert!(second.revoker().revoke());
        drop(second);
        assert_eq!(authority.remaining(), 0);
        assert!(authority.grant(&artifact, 9).is_err());
    }
    #[test]
    fn pinning_denial_expiry_and_group_revoke() {
        let old = artifact("1.0.0", "local.get 0");
        let new = artifact("2.0.0", "local.get 0");
        let altered = artifact("1.0.0", "i64.const 99");
        let mut authority = TaskAuthority::operator_for_test(
            TASK,
            Instant::now() + Duration::from_secs(10),
            2,
            &[&old],
        )
        .unwrap();
        assert!(authority.grant(&new, 0).is_err());
        assert!(authority.grant(&altered, 0).is_err());
        assert_eq!(authority.remaining(), 2);
        let mut first = authority.grant(&old, 1).unwrap();
        let mut second = authority.grant(&old, 2).unwrap();
        authority.revoke();
        assert!(old.invoke(&mut first, TASK, "human-cli", 1).is_err());
        assert!(old.invoke(&mut second, TASK, "human-cli", 2).is_err());
        assert!(authority.grant(&old, 3).is_err());
        let mut expired = TaskAuthority::operator_for_test(
            TASK,
            Instant::now() + Duration::from_secs(10),
            1,
            &[&old],
        )
        .unwrap();
        expired.deadline = Instant::now();
        assert!(expired.grant(&old, 0).is_err());
        assert_eq!(expired.remaining(), 1);
    }
    #[test]
    fn different_approved_artifacts_share_slots_even_when_guest_traps() {
        let first = artifact("1.0.0", "unreachable");
        let bytes=wat::parse_str("(module (func (export \"evaluate\") (param i64) (result i64) local.get 0 i64.const 1 i64.add))").unwrap();
        let second = Artifact::import(
            "math.other.s1",
            "1.0.0",
            &format!("{:x}", Sha256::digest(&bytes)),
            &bytes,
        )
        .unwrap();
        let mut authority = TaskAuthority::operator_for_test(
            TASK,
            Instant::now() + Duration::from_secs(10),
            2,
            &[&first, &second],
        )
        .unwrap();
        let mut one = authority.grant(&first, 0).unwrap();
        let mut two = authority.grant(&second, 4).unwrap();
        assert!(first.invoke(&mut one, TASK, "human-cli", 0).is_err());
        assert_eq!(second.invoke(&mut two, TASK, "human-cli", 4).unwrap(), 5);
        assert_eq!(authority.remaining(), 0);
        assert!(authority.grant(&first, 0).is_err());
        assert!(authority.grant(&second, 4).is_err());
    }

    #[test]
    fn invalid_and_ambiguous_authorities_are_denied() {
        let old = artifact("1.0.0", "local.get 0");
        let new = artifact("2.0.0", "local.get 0");
        let deadline = Instant::now() + Duration::from_secs(10);
        for count in [0, 9] {
            assert!(TaskAuthority::operator_for_test(TASK, deadline, count, &[&old]).is_err());
        }
        assert!(TaskAuthority::operator_for_test(TASK, deadline, 1, &[]).is_err());
        assert!(TaskAuthority::operator_for_test(TASK, deadline, 1, &[&old, &new]).is_err());
        assert!(TaskAuthority::operator_for_test("bad-task", deadline, 1, &[&old]).is_err());
        assert!(TaskAuthority::operator_for_test(TASK, Instant::now(), 1, &[&old]).is_err());
    }
}
