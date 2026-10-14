//! Bounded metadata acknowledgements; the trusted embedding supplies durability.
use crate::artifact::Identity;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Allocated,
    DispatchReserved,
    Returned,
    NotReturned,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub task: String,
    pub artifact: Identity,
    pub slot: usize,
    pub kind: Kind,
}
/// Return Ok only after the required durable acknowledgement. The library does
/// not attest the sink, preempt synchronous callbacks or grant them guest access.
pub trait Audit: Send + Sync {
    fn record(&self, event: &Event) -> Result<(), String>;
}
#[cfg(test)]
#[derive(Default)]
pub(crate) struct MemoryAudit(pub std::sync::Mutex<Vec<Event>>);
#[cfg(test)]
impl Audit for MemoryAudit {
    fn record(&self, event: &Event) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "audit poisoned")?
            .push(event.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        artifact::{Artifact, Revoker},
        authority::TaskAuthority,
    };
    use sha2::{Digest, Sha256};
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    const TASK: &str = "11111111-1111-4111-8111-111111111111";
    fn artifact(body: &str) -> Artifact {
        let bytes = wat::parse_str(format!(
            "(module (func (export \"evaluate\") (param i64) (result i64) {body}))"
        ))
        .unwrap();
        Artifact::import(
            "math.audit.s1",
            "1.0.0",
            &format!("{:x}", Sha256::digest(&bytes)),
            &bytes,
        )
        .unwrap()
    }
    struct FailingAudit {
        kind: Kind,
        persist_first: bool,
        events: MemoryAudit,
    }
    impl Audit for FailingAudit {
        fn record(&self, event: &Event) -> Result<(), String> {
            if event.kind == self.kind {
                if self.persist_first {
                    self.events.record(event)?;
                }
                return Err("private_audit_backend_diagnostic".into());
            }
            self.events.record(event)
        }
    }
    #[test]
    fn audit_failures_never_refund_or_expose_guest_results() {
        for kind in [Kind::Allocated, Kind::DispatchReserved, Kind::Returned] {
            for persist_first in [false, true] {
                let artifact = artifact("local.get 0");
                let audit = Arc::new(FailingAudit {
                    kind,
                    persist_first,
                    events: MemoryAudit::default(),
                });
                let mut authority = TaskAuthority::operator(
                    TASK,
                    Instant::now() + Duration::from_secs(10),
                    1,
                    &[&artifact],
                    audit.clone(),
                )
                .unwrap();
                let grant = authority.grant(&artifact, 987654321);
                if kind == Kind::Allocated {
                    assert_eq!(grant.err().unwrap(), "audit acknowledgement failed");
                } else {
                    let mut grant = grant.unwrap();
                    assert_eq!(
                        artifact
                            .invoke(&mut grant, TASK, "human-cli", 987654321)
                            .unwrap_err(),
                        "audit acknowledgement failed"
                    );
                    assert_eq!(
                        artifact
                            .invoke(&mut grant, TASK, "human-cli", 987654321)
                            .unwrap_err(),
                        "invocation grant already consumed"
                    );
                }
                assert_eq!(authority.remaining(), 0);
                assert!(authority.grant(&artifact, 0).is_err());
                let events = audit.events.0.lock().unwrap();
                assert_eq!(
                    events.iter().filter(|event| event.kind == kind).count(),
                    usize::from(persist_first)
                );
                let debug = format!("{events:?}");
                assert!(!debug.contains("private_audit_backend_diagnostic"));
                assert!(!debug.contains("input:") && !debug.contains("result:"));
            }
        }
    }
    #[test]
    fn ordered_metadata_distinguishes_returned_and_trapped_execution() {
        for (body, terminal) in [
            ("local.get 0", Kind::Returned),
            ("unreachable", Kind::NotReturned),
        ] {
            let artifact = artifact(body);
            let audit = Arc::new(MemoryAudit::default());
            let mut authority = TaskAuthority::operator(
                TASK,
                Instant::now() + Duration::from_secs(10),
                1,
                &[&artifact],
                audit.clone(),
            )
            .unwrap();
            let mut grant = authority.grant(&artifact, 42).unwrap();
            let _ = artifact.invoke(&mut grant, TASK, "human-cli", 42);
            let events = audit.0.lock().unwrap();
            assert_eq!(
                events.iter().map(|e| e.kind).collect::<Vec<_>>(),
                vec![Kind::Allocated, Kind::DispatchReserved, terminal]
            );
            assert!(events
                .iter()
                .all(|e| e.task == TASK && e.artifact == *artifact.identity() && e.slot == 1));
        }
    }
    struct RevokingAudit {
        events: MemoryAudit,
        revoker: Mutex<Option<Revoker>>,
    }
    impl Audit for RevokingAudit {
        fn record(&self, event: &Event) -> Result<(), String> {
            self.events.record(event)?;
            if event.kind == Kind::DispatchReserved {
                self.revoker.lock().unwrap().as_ref().unwrap().revoke();
            }
            Ok(())
        }
    }
    #[test]
    fn revocation_during_dispatch_ack_suppresses_result() {
        let artifact = artifact("(loop $l br $l) i64.const 0");
        let audit = Arc::new(RevokingAudit {
            events: MemoryAudit::default(),
            revoker: Mutex::new(None),
        });
        let mut authority = TaskAuthority::operator(
            TASK,
            Instant::now() + Duration::from_secs(10),
            1,
            &[&artifact],
            audit.clone(),
        )
        .unwrap();
        let mut grant = authority.grant(&artifact, 0).unwrap();
        *audit.revoker.lock().unwrap() = Some(grant.revoker());
        assert_eq!(
            artifact
                .invoke(&mut grant, TASK, "human-cli", 0)
                .unwrap_err(),
            "invocation grant revoked"
        );
        assert_eq!(
            audit.events.0.lock().unwrap().last().unwrap().kind,
            Kind::NotReturned
        );
        assert_eq!(authority.remaining(), 0);
    }
}
