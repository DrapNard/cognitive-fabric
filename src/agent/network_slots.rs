//! Shared, task-local execution width for mixed typed and model work.
//!
//! A slot bounds an owned host operation, not physical provider activity after
//! an ambiguous timeout. Existing model residency/resource policies still apply.

use anyhow::{bail, Context, Result};
use std::{sync::Arc, time::Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone)]
pub(super) struct Budget {
    slots: Arc<Semaphore>,
    limit: usize,
}

impl Budget {
    pub(super) fn new(limit: usize) -> Result<Self> {
        if !(1..=4).contains(&limit) {
            bail!("Shared network width must be between one and four")
        }
        Ok(Self {
            slots: Arc::new(Semaphore::new(limit)),
            limit,
        })
    }

    pub(super) fn limit(&self) -> usize {
        self.limit
    }

    pub(super) async fn acquire(&self, deadline: Instant) -> Result<OwnedSemaphorePermit> {
        self.acquire_many(1, deadline).await
    }

    /// Reserve paired roles atomically: holding one slot while waiting for
    /// another could deadlock independently admitted pairs at a small width.
    pub(super) async fn acquire_many(
        &self,
        count: u32,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit> {
        if count == 0 || count as usize > self.limit || Instant::now() >= deadline {
            bail!("Shared network slot request is invalid or expired")
        }
        let permit = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.slots.clone().acquire_many_owned(count),
        )
        .await
        .context("Shared network slot deadline expired")?
        .context("Shared network slot budget closed")?;
        if Instant::now() >= deadline {
            bail!("Shared network slot deadline expired before dispatch")
        }
        Ok(permit)
    }
}
