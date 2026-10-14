//! Optional trusted embedding bridge. Core owns the task, clock, lease and ledger.
use crate::{
    artifact::Artifact,
    audit::{Audit, Event, Kind},
    authority::TaskAuthority,
};
use cogfabric::{
    config::Config,
    module_tasks::{Context, Identity, Phase},
    store::Store,
};
use std::sync::Arc;
pub struct CoreAudit(pub Arc<Context>);
impl Audit for CoreAudit {
    fn record(&self, event: &Event) -> Result<(), String> {
        let artifact = Identity {
            id: event.artifact.id.clone(),
            version: event.artifact.version.clone(),
            checksum: event.artifact.checksum.clone(),
        };
        let phase = match event.kind {
            Kind::Allocated => Phase::Allocated,
            Kind::DispatchReserved => Phase::DispatchReserved,
            Kind::Returned => Phase::Returned,
            Kind::NotReturned => Phase::NotReturned,
        };
        self.0
            .record(&event.task, &artifact, event.slot, phase)
            .map_err(|_| "Core module audit failed".into())
    }
}
/// The caller authenticates operator intent. No model or IPC entry point is added.
/// The returned context retains the original lease; grants also retain it via Audit.
pub fn operator(
    store: Arc<Store>,
    cfg: &Config,
    actor: &str,
    maximum: usize,
    seconds: u64,
    allowed: &[&Artifact],
) -> Result<(Arc<Context>, TaskAuthority), String> {
    let identities = allowed
        .iter()
        .map(|a| {
            let i = a.identity();
            Identity {
                id: i.id.clone(),
                version: i.version.clone(),
                checksum: i.checksum.clone(),
            }
        })
        .collect();
    let context = Arc::new(
        Context::operator(store, cfg, actor, maximum, seconds, identities)
            .map_err(|e| e.to_string())?,
    );
    let authority = TaskAuthority::operator(
        context.task(),
        context.deadline(),
        maximum,
        allowed,
        Arc::new(CoreAudit(context.clone())),
    )?;
    Ok((context, authority))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn core_acknowledges_execution_and_rejects_a_second_authority_reset() {
        let bytes = wat::parse_str(
            "(module (func (export \"evaluate\") (param i64) (result i64) local.get 0))",
        )
        .unwrap();
        let artifact = Artifact::import(
            "math.demo.s1",
            "1.0.0",
            &format!("{:x}", Sha256::digest(&bytes)),
            &bytes,
        )
        .unwrap();
        let store = Arc::new(Store::in_memory().unwrap());
        let (ctx, mut authority) = operator(
            store.clone(),
            &Config::default(),
            "human-cli",
            1,
            10,
            &[&artifact],
        )
        .unwrap();
        let mut grant = authority.grant(&artifact, 42).unwrap();
        assert_eq!(
            artifact
                .invoke(&mut grant, ctx.task(), "human-cli", 42)
                .unwrap(),
            42
        );
        let mut duplicate = TaskAuthority::operator(
            ctx.task(),
            ctx.deadline(),
            1,
            &[&artifact],
            Arc::new(CoreAudit(ctx.clone())),
        )
        .unwrap();
        assert!(duplicate.grant(&artifact, 99).is_err());
        store.finish_task(ctx.task(), "unverified", "").unwrap();
    }
}
