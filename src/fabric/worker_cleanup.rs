//! Collective process ownership survives dropped task futures until bounded reaping.
use crate::{recovery::ExecutionLease, store::Store};
use anyhow::{bail, Context, Result};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::{process::Child, sync::Notify};

static OWNERS: AtomicUsize = AtomicUsize::new(0);
static CHANGED: Notify = Notify::const_new();
static FAILED: AtomicBool = AtomicBool::new(false);

pub(crate) fn record_failure() {
    FAILED.store(true, Ordering::Release);
}

pub(crate) struct Ownership;
impl Ownership {
    pub(crate) fn acquire() -> Result<Self> {
        if FAILED.load(Ordering::Acquire) {
            bail!("Collective worker admission disabled after cleanup failure")
        }
        OWNERS.fetch_add(1, Ordering::AcqRel);
        Ok(Self)
    }
}
impl Drop for Ownership {
    fn drop(&mut self) {
        OWNERS.fetch_sub(1, Ordering::AcqRel);
        CHANGED.notify_waiters();
    }
}

/// Call after stopping task admission and dropping completed request handlers.
/// Includes unlaunched task owners and reapers detached by task cancellation.
pub async fn drain_workers() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let changed = CHANGED.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if OWNERS.load(Ordering::Acquire) == 0 {
                break;
            }
            changed.await;
        }
    })
    .await
    .context("Collective worker shutdown drain deadline exhausted")?;
    if FAILED.load(Ordering::Acquire) {
        bail!("Collective worker cleanup encountered errors; inspect its audit")
    }
    Ok(())
}

pub(crate) struct Pending {
    pub child: Child,
    pub socket: super::unix::process::SocketDirectory,
    pub lease: Option<ExecutionLease>,
}

pub(crate) fn detached(
    mut pending: Vec<Pending>,
    ownership: Ownership,
    store: Arc<Store>,
    task: String,
    kind: &'static str,
) {
    pending.retain(|owned| owned.child.id().is_some());
    if pending.is_empty() {
        return;
    }
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            let _ownership = ownership;
            for Pending {
                mut child,
                socket,
                lease,
            } in pending
            {
                // Child::id becomes None only after Tokio observed its exit.
                if let Some(pid) = child.id() {
                    let result: Result<()> =
                        async {
                            let mut forced = false;
                            let status =
                                match tokio::time::timeout(Duration::from_secs(1), child.wait())
                                    .await
                                {
                                    Ok(status) => status?,
                                    Err(_) => {
                                        forced = true;
                                        child.start_kill()?;
                                        tokio::time::timeout(Duration::from_secs(2), child.wait())
                                            .await
                                            .context(
                                                "Dropped collective worker reap deadline exhausted",
                                            )??
                                    }
                                };
                            store.event(
                                &task,
                                kind,
                                "fabric",
                                serde_json::json!({
                                    "pid":pid,"forced":forced,"exit_code":status.code(),
                                    "reaped":true,"dropped_owner":true
                                }),
                            )
                        }
                        .await;
                    if result.is_err() {
                        FAILED.store(true, Ordering::Release);
                    }
                }
                drop(child);
                drop(socket);
                drop(lease);
            }
        });
    } else {
        // No runtime means no bounded wait can be guaranteed. kill_on_drop is
        // only best effort; expose failure and never fabricate a reaping audit.
        FAILED.store(true, Ordering::Release);
    }
}
