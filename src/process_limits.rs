//! Optional Linux process limits for trusted, operator-configured workers.
//!
//! These limits are applied in the child between `fork` and `exec` with
//! `setrlimit`. They constrain virtual address space and CPU time for that
//! process; they are not RSS/GPU quotas, a filesystem or network sandbox, or a
//! complete isolation boundary. They must only be constructed from trusted
//! operator configuration, never from model output or an external provider.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

const MIN_ADDRESS_SPACE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ADDRESS_SPACE_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const MAX_CPU_SECONDS: u64 = 3_600;

/// Kernel-enforced limits for a managed worker process on Linux.
///
/// Address space measures virtual memory (`RLIMIT_AS`), not resident memory.
/// CPU time measures cumulative process CPU seconds (`RLIMIT_CPU`), not wall
/// clock time. The inherited limit is never raised: each resulting soft and
/// hard limit is clamped to the requested value and both inherited limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Maximum virtual address space in bytes (64 MiB through 1 TiB).
    pub address_space_bytes: u64,
    /// Maximum cumulative CPU time in seconds (1 through 3600).
    pub cpu_seconds: u64,
}

impl Limits {
    /// Validate field bounds and ensure this platform supports these limits.
    pub fn validate(self) -> Result<()> {
        if !(MIN_ADDRESS_SPACE_BYTES..=MAX_ADDRESS_SPACE_BYTES).contains(&self.address_space_bytes)
        {
            bail!(
                "worker address-space limit must be between {MIN_ADDRESS_SPACE_BYTES} and {MAX_ADDRESS_SPACE_BYTES} bytes"
            )
        }
        if !(1..=MAX_CPU_SECONDS).contains(&self.cpu_seconds) {
            bail!("worker CPU limit must be between 1 and {MAX_CPU_SECONDS} seconds")
        }

        #[cfg(target_os = "linux")]
        {
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            bail!("managed worker kernel limits are supported only on Linux")
        }
    }
}

/// Add fail-closed child-side kernel limits to a Tokio process command.
///
/// `None` leaves the command untouched. On Linux, the `pre_exec` hook only
/// performs `getrlimit`, `setrlimit`, and construction of OS errors. Any
/// failure prevents the child from reaching `exec`; the parent process limits
/// are never modified.
pub fn apply(command: &mut Command, limits: Option<Limits>) -> Result<()> {
    let Some(limits) = limits else {
        return Ok(());
    };
    limits.validate()?;

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;

        let address_space = libc::rlim_t::try_from(limits.address_space_bytes)
            .map_err(|_| anyhow::anyhow!("worker address-space limit does not fit rlim_t"))?;
        let cpu_seconds = libc::rlim_t::try_from(limits.cpu_seconds)
            .map_err(|_| anyhow::anyhow!("worker CPU limit does not fit rlim_t"))?;

        // SAFETY: The closure only invokes async-signal-safe libc resource-limit
        // calls and reads errno through `last_os_error`; it takes no locks,
        // allocates no memory, and does not access parent-owned mutable state.
        unsafe {
            command.as_std_mut().pre_exec(move || {
                apply_one_limit(libc::RLIMIT_AS as libc::c_int, address_space)?;
                apply_one_limit(libc::RLIMIT_CPU as libc::c_int, cpu_seconds)?;
                apply_one_limit(libc::RLIMIT_CORE as libc::c_int, 0)
            });
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = command;
        bail!("managed worker kernel limits are supported only on Linux")
    }
}

#[cfg(target_os = "linux")]
fn apply_one_limit(resource: libc::c_int, requested: libc::rlim_t) -> std::io::Result<()> {
    let mut inherited = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    // SAFETY: `inherited` points to a valid writable `rlimit`, and the resource
    // is one of the fixed Linux constants selected by this module.
    if unsafe { libc::getrlimit(resource as _, &mut inherited) } != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let ceiling = inherited.rlim_cur.min(inherited.rlim_max);
    let limit = requested.min(ceiling);
    let restricted = libc::rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };

    // SAFETY: `restricted` points to a valid initialized `rlimit`. Both values
    // are no greater than the inherited soft or hard limit.
    if unsafe { libc::setrlimit(resource as _, &restricted) } != 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::time::Duration;

    fn current(resource: libc::c_int) -> libc::rlimit {
        let mut value = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: value is a writable rlimit and the resource is a fixed constant.
        assert_eq!(unsafe { libc::getrlimit(resource as _, &mut value) }, 0);
        value
    }

    fn child(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process_limits::tests::child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env_clear()
            .env("COGFABRIC_LIMIT_FIXTURE", mode)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        command
    }

    #[test]
    #[ignore = "subprocess fixture; invoked by process-limit regression tests"]
    fn child_fixture() {
        match std::env::var("COGFABRIC_LIMIT_FIXTURE").unwrap().as_str() {
            "readback" | "stricter" => {
                let address = current(libc::RLIMIT_AS as _);
                let cpu = current(libc::RLIMIT_CPU as _);
                assert_eq!(current(libc::RLIMIT_CORE as _).rlim_max, 0);
                if std::env::var("COGFABRIC_LIMIT_FIXTURE").unwrap() == "stricter" {
                    assert!(address.rlim_max <= 256 * 1024 * 1024);
                    assert!(cpu.rlim_max <= 5);
                }
                assert!(address.rlim_cur <= 512 * 1024 * 1024);
                assert_eq!(address.rlim_cur, address.rlim_max);
                assert!(cpu.rlim_cur <= 10);
                assert_eq!(cpu.rlim_cur, cpu.rlim_max);
                // An allocation larger than the entire allowed address space must fail.
                // SAFETY: anonymous mapping has no backing file; no returned pointer is read.
                let mapped = unsafe {
                    libc::mmap(
                        std::ptr::null_mut(),
                        1024 * 1024 * 1024,
                        libc::PROT_READ | libc::PROT_WRITE,
                        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                        -1,
                        0,
                    )
                };
                assert_eq!(mapped, libc::MAP_FAILED);
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ENOMEM)
                );
            }
            "cpu" => {
                let mut accumulator = 0u64;
                loop {
                    accumulator = std::hint::black_box(accumulator.wrapping_add(1));
                }
            }
            _ => panic!("Unknown child fixture"),
        }
    }

    #[tokio::test]
    async fn child_kernel_limits_restrict_mapping_without_changing_parent() {
        let address = current(libc::RLIMIT_AS as _);
        let cpu = current(libc::RLIMIT_CPU as _);
        let mut command = child("readback");
        apply(
            &mut command,
            Some(Limits {
                address_space_bytes: 512 * 1024 * 1024,
                cpu_seconds: 10,
            }),
        )
        .unwrap();
        let mut child = command.spawn().unwrap();
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());
        let after_address = current(libc::RLIMIT_AS as _);
        let after_cpu = current(libc::RLIMIT_CPU as _);
        assert_eq!(
            (address.rlim_cur, address.rlim_max),
            (after_address.rlim_cur, after_address.rlim_max)
        );
        assert_eq!(
            (cpu.rlim_cur, cpu.rlim_max),
            (after_cpu.rlim_cur, after_cpu.rlim_max)
        );
    }

    #[tokio::test]
    async fn stricter_inherited_limits_are_never_raised() {
        use std::os::unix::process::CommandExt;
        let mut command = child("stricter");
        // SAFETY: the child hook makes only the same fixed resource-limit calls.
        unsafe {
            command.as_std_mut().pre_exec(|| {
                apply_one_limit(libc::RLIMIT_AS as _, 256 * 1024 * 1024)?;
                apply_one_limit(libc::RLIMIT_CPU as _, 5)
            });
        }
        apply(
            &mut command,
            Some(Limits {
                address_space_bytes: 512 * 1024 * 1024,
                cpu_seconds: 10,
            }),
        )
        .unwrap();
        let mut child = command.spawn().unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success());
    }

    #[tokio::test]
    async fn cpu_budget_terminates_only_the_busy_child() {
        let mut command = child("cpu");
        apply(
            &mut command,
            Some(Limits {
                address_space_bytes: 512 * 1024 * 1024,
                cpu_seconds: 1,
            }),
        )
        .unwrap();
        let mut child = command.spawn().unwrap();
        let status = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
            Ok(status) => status.unwrap(),
            Err(_) => {
                child.kill().await.unwrap();
                let _ = child.wait().await;
                panic!("CPU limit failed to terminate child");
            }
        };
        assert!(matches!(
            status.signal(),
            Some(libc::SIGKILL | libc::SIGXCPU)
        ));
    }

    #[test]
    fn malformed_or_unbounded_limits_fail_before_spawn() {
        for limits in [
            Limits {
                address_space_bytes: 0,
                cpu_seconds: 1,
            },
            Limits {
                address_space_bytes: 512 * 1024 * 1024,
                cpu_seconds: 0,
            },
        ] {
            assert!(apply(&mut Command::new("/not/executed"), Some(limits)).is_err());
        }
        assert!(serde_json::from_str::<Limits>(
            r#"{"address_space_bytes":536870912,"cpu_seconds":10,"shell":true}"#
        )
        .is_err());
    }
}
