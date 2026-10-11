//! Same-boot elapsed-time evidence. Never restore a budget from civil timestamps.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    format_version: u32,
    clock: String,
    boot_id: String,
    started_ns: u64,
    deadline_ns: u64,
}
impl Snapshot {
    fn capture(seconds: u64) -> Option<Self> {
        let (boot_id, started_ns) = observation()?;
        let deadline_ns = started_ns.checked_add(seconds.checked_mul(1_000_000_000)?)?;
        Some(Self {
            format_version: 1,
            clock: "linux_boottime".into(),
            boot_id,
            started_ns,
            deadline_ns,
        })
    }
    pub(crate) fn validate(&self, seconds: u64) -> Result<()> {
        if self.format_version != 1
            || self.clock != "linux_boottime"
            || uuid::Uuid::parse_str(&self.boot_id)?.to_string() != self.boot_id
            || self.deadline_ns.checked_sub(self.started_ns) != seconds.checked_mul(1_000_000_000)
        {
            bail!("Invalid saved execution clock")
        }
        Ok(())
    }
    fn remaining_at(&self, boot: &str, now: u64) -> Result<Value> {
        if boot != self.boot_id {
            return Ok(
                json!({"clock":"linux_boottime","status":"different_boot","remaining_ns":null}),
            );
        }
        if now < self.started_ns {
            bail!("Execution clock moved backwards or saved clock is invalid")
        }
        Ok(
            json!({"clock":"linux_boottime","status":"same_boot","remaining_ns":self.deadline_ns.saturating_sub(now),"expired":now>=self.deadline_ns}),
        )
    }
    pub(crate) fn remaining(&self) -> Result<Value> {
        match observation() {
            Some((boot, now)) => self.remaining_at(&boot, now),
            None => {
                Ok(json!({"clock":"linux_boottime","status":"unavailable","remaining_ns":null}))
            }
        }
    }
}

pub(crate) struct ExecutionStart {
    pub(crate) instant: Instant,
    pub(crate) deadline: Instant,
    pub(crate) clock: Option<Snapshot>,
}
impl ExecutionStart {
    pub(crate) fn new(seconds: u64) -> Self {
        // Read boot time before the process-local start: recovery is conservative
        // about this capture overhead, rather than granting extra elapsed time.
        let clock = Snapshot::capture(seconds);
        let instant = Instant::now();
        Self {
            instant,
            deadline: instant + Duration::from_secs(seconds),
            clock,
        }
    }
}

#[cfg(target_os = "linux")]
fn observation() -> Option<(String, u64)> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut time) } != 0
        || time.tv_sec < 0
        || !(0..1_000_000_000).contains(&time.tv_nsec)
    {
        return None;
    }
    let ns = u64::try_from(time.tv_sec)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(time.tv_nsec as u64)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/proc/sys/kernel/random/boot_id")
        .ok()?;
    let mut raw = String::new();
    file.take(65).read_to_string(&mut raw).ok()?;
    let boot_id = uuid::Uuid::parse_str(raw.trim()).ok()?.to_string();
    Some((boot_id, ns))
}
#[cfg(not(target_os = "linux"))]
fn observation() -> Option<(String, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_boot_deadlines_expire_without_restoring_time_after_reboot() {
        let boot = "00000000-0000-4000-8000-000000000001";
        let snapshot = Snapshot {
            format_version: 1,
            clock: "linux_boottime".into(),
            boot_id: boot.into(),
            started_ns: 100,
            deadline_ns: 1_000_000_100,
        };
        snapshot.validate(1).unwrap();
        assert_eq!(
            snapshot.remaining_at(boot, 500_000_100).unwrap()["remaining_ns"],
            500_000_000u64
        );
        assert_eq!(
            snapshot.remaining_at(boot, 1_000_000_100).unwrap()["remaining_ns"],
            0
        );
        assert_eq!(
            snapshot.remaining_at(boot, 2_000_000_100).unwrap()["expired"],
            true
        );
        assert!(snapshot.remaining_at(boot, 99).is_err());
        let other = snapshot
            .remaining_at("00000000-0000-4000-8000-000000000002", 500)
            .unwrap();
        assert!(other["remaining_ns"].is_null());
        assert_eq!(other["status"], "different_boot");
        assert!(snapshot.validate(2).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn actual_kernel_observation_is_bounded_by_original_allowance() {
        let start = ExecutionStart::new(1);
        {
            let clock = start
                .clock
                .expect("Linux boot clock observation unavailable");
            clock.validate(1).unwrap();
            let remaining = clock.remaining().unwrap();
            assert_eq!(remaining["status"], "same_boot");
            assert!(remaining["remaining_ns"].as_u64().unwrap() <= 1_000_000_000);
        }
    }
}
