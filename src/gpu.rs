//! Opt-in operator GPU observations, never task-attributed resource accounting.
pub mod nvml;
pub mod window;
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
#[cfg(any(target_os = "linux", test))]
use std::collections::BTreeSet;

/// Explicitly selected read-only GPU observation backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sensor {
    /// Preserve the existing fixed `nvidia-smi` query behavior.
    #[default]
    NvidiaSmi,
    /// Query NVML in a killable same-image worker process.
    Nvml,
}
impl Sensor {
    pub const fn name(self) -> &'static str {
        match self {
            Self::NvidiaSmi => "nvidia-smi",
            Self::Nvml => "nvml",
        }
    }
    pub const fn compiled_support(self) -> bool {
        match self {
            Self::NvidiaSmi => false,
            Self::Nvml => cfg!(feature = "nvml-gpu"),
        }
    }
}

#[derive(Debug, Serialize)]
struct Device {
    uuid: String,
    memory_total_bytes: Option<u64>,
    memory_used_bytes: Option<u64>,
    utilization_percent: Option<f64>,
    power_watts: Option<f64>,
}
#[cfg(any(target_os = "linux", test))]
fn number(raw: &str, maximum: f64) -> Result<Option<f64>> {
    if matches!(raw, "N/A" | "[N/A]" | "[Not Supported]") {
        return Ok(None);
    }
    let value: f64 = raw.parse()?;
    if !value.is_finite() || !(0.0..=maximum).contains(&value) {
        bail!("Invalid GPU metric")
    }
    Ok(Some(value))
}
#[cfg(any(target_os = "linux", test))]
fn memory(raw: &str) -> Result<Option<u64>> {
    let value = number(raw, 16_777_216.0)?;
    if value.is_some_and(|value| value.fract() != 0.0) {
        bail!("Invalid GPU memory units")
    }
    Ok(value.map(|value| value as u64 * 1024 * 1024))
}
#[cfg(any(target_os = "linux", test))]
fn parse(text: &str) -> Result<Vec<Device>> {
    if text.len() > 16384 {
        bail!("GPU response exceeds bounds")
    }
    let mut devices = Vec::new();
    let mut ids = BTreeSet::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 5 || devices.len() >= 16 {
            bail!("Invalid GPU response shape")
        }
        let id = fields[0];
        if !(5..=80).contains(&id.len())
            || !id.starts_with("GPU-")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
            || !ids.insert(id.to_string())
        {
            bail!("Invalid or duplicate GPU identity")
        }
        let total = memory(fields[1])?;
        let used = memory(fields[2])?;
        if total.zip(used).is_some_and(|(total, used)| used > total) {
            bail!("GPU memory exceeds declared capacity")
        }
        devices.push(Device {
            uuid: id.into(),
            memory_total_bytes: total,
            memory_used_bytes: used,
            utilization_percent: number(fields[3], 100.0)?,
            power_watts: number(fields[4], 10000.0)?,
        });
    }
    if devices.is_empty() {
        bail!("No GPU samples")
    }
    Ok(devices)
}

#[cfg(target_os = "linux")]
async fn query_nvidia_smi() -> Result<Vec<Device>> {
    use std::{process::Stdio, time::Duration};
    // Fixed executable and arguments: no PATH search, shell, model input or setters.
    let mut command = tokio::process::Command::new("/usr/bin/nvidia-smi");
    command
        .args([
            "--query-gpu=uuid,memory.total,memory.used,utilization.gpu,power.draw",
            "--format=csv,noheader,nounits",
        ])
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    run_query(command, Duration::from_secs(2)).await
}

#[cfg(target_os = "linux")]
async fn query_nvml() -> Result<Vec<Device>> {
    use std::{process::Stdio, time::Duration};
    if !cfg!(feature = "nvml-gpu") {
        bail!("NVML support is not compiled in; enable the nvml-gpu feature")
    }
    // The child is the already-running program image, never a caller-selected
    // executable. NVML calls may block inside the driver, so they must not run
    // on an async worker thread that cannot be cancelled.
    let mut command =
        tokio::process::Command::new(crate::fabric::unix::process::managed_executable()?);
    command
        .arg("gpu-nvml-worker")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    let bytes = run_bounded_child(command, Duration::from_secs(2), 16384).await?;
    nvml::decode_sample(&bytes)
}

#[cfg(not(target_os = "linux"))]
async fn query_nvml() -> Result<Vec<Device>> {
    bail!("NVML worker requires Linux")
}

#[cfg(target_os = "linux")]
async fn run_query(
    command: tokio::process::Command,
    limit: std::time::Duration,
) -> Result<Vec<Device>> {
    let bytes = run_bounded_child(command, limit, 16384).await?;
    parse(std::str::from_utf8(&bytes)?)
}

#[cfg(target_os = "linux")]
async fn run_bounded_child(
    mut command: tokio::process::Command,
    limit: std::time::Duration,
    maximum: usize,
) -> Result<Vec<u8>> {
    use anyhow::Context;
    use std::process::ExitStatus;
    use tokio::io::AsyncReadExt;
    let mut child = OwnedGpuChild::spawn(&mut command)?;
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .context("Missing GPU output pipe")?;
    let result = tokio::time::timeout(limit, async {
        let mut bytes = Vec::new();
        stdout
            .take((maximum + 1) as u64)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > maximum {
            bail!("GPU response exceeds bounds")
        }
        let status: ExitStatus = child.wait().await?;
        if !status.success() {
            bail!("GPU driver query failed")
        }
        Ok(bytes)
    })
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => {
            child.terminate_and_reap().await?;
            Err(error)
        }
        Err(_) => {
            child.terminate_and_reap().await?;
            bail!("GPU query timeout")
        }
    }
}

#[cfg(target_os = "linux")]
struct OwnedGpuChild {
    child: Option<tokio::process::Child>,
    ownership: Option<crate::fabric::worker_cleanup::Ownership>,
}

#[cfg(target_os = "linux")]
impl OwnedGpuChild {
    fn spawn(command: &mut tokio::process::Command) -> Result<Self> {
        let ownership = crate::fabric::worker_cleanup::Ownership::acquire()?;
        let child = command.spawn()?;
        Ok(Self {
            child: Some(child),
            ownership: Some(ownership),
        })
    }

    fn child_mut(&mut self) -> &mut tokio::process::Child {
        self.child.as_mut().expect("GPU child guard has a child")
    }

    async fn wait(&mut self) -> Result<std::process::ExitStatus> {
        use anyhow::Context;
        let status = self
            .child_mut()
            .wait()
            .await
            .context("Cannot reap GPU worker")?;
        self.child.take();
        self.ownership.take();
        Ok(status)
    }

    async fn terminate_and_reap(&mut self) -> Result<()> {
        use anyhow::Context;
        use std::time::Duration;
        use tokio::time::timeout;
        let Some(child) = self.child.as_mut() else {
            self.ownership.take();
            return Ok(());
        };
        if child.id().is_some() {
            let _ = child.start_kill();
        }
        match timeout(Duration::from_secs(2), child.wait()).await {
            Ok(Ok(_)) => {
                self.child.take();
                self.ownership.take();
                Ok(())
            }
            Ok(Err(error)) => {
                crate::fabric::worker_cleanup::record_failure();
                Err(error).context("GPU worker cleanup failed")
            }
            Err(_) => {
                crate::fabric::worker_cleanup::record_failure();
                bail!("GPU worker reap deadline exhausted")
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for OwnedGpuChild {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            self.ownership.take();
            return;
        };
        let Some(ownership) = self.ownership.take() else {
            crate::fabric::worker_cleanup::record_failure();
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            crate::fabric::worker_cleanup::record_failure();
            return;
        };
        runtime.spawn(async move {
            use std::time::Duration;
            if child.id().is_some() {
                let _ = child.start_kill();
            }
            let reaped = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            if !matches!(reaped, Ok(Ok(_))) {
                crate::fabric::worker_cleanup::record_failure();
            }
            drop(ownership);
        });
    }
}

/// Operator-only, one-shot sampling. Failure preserves unavailable measurements.
pub async fn sample() -> Result<Value> {
    sample_with(Sensor::NvidiaSmi).await
}

/// Operator-only sampling with an explicit backend; unavailable NVML support
/// is reported as unavailable and never silently falls back to another sensor.
pub async fn sample_with(sensor: Sensor) -> Result<Value> {
    crate::security::authorize("human-cli", "telemetry.gpu")?;
    let result = match sensor {
        Sensor::NvidiaSmi => {
            #[cfg(target_os = "linux")]
            {
                query_nvidia_smi().await
            }
            #[cfg(not(target_os = "linux"))]
            {
                Err(anyhow::anyhow!("GPU query requires Linux"))
            }
        }
        Sensor::Nvml => query_nvml().await,
    };
    Ok(match result {
        Ok(devices) => {
            let mut report = json!({"status":"sampled","devices":devices,"scope":"whole_device","task_attributed":false,"energy_joules":null});
            if sensor != Sensor::NvidiaSmi {
                report["sensor"] = json!(sensor.name());
                report["sensor_compiled_support"] = json!(sensor.compiled_support());
            }
            report
        }
        Err(error) => {
            let mut report = json!({"status":"unavailable","devices":null,"reason":error.to_string().chars().take(200).collect::<String>(),"scope":"whole_device","task_attributed":false,"energy_joules":null});
            if sensor != Sensor::NvidiaSmi {
                report["sensor"] = json!(sensor.name());
                report["sensor_compiled_support"] = json!(sensor.compiled_support());
            }
            report
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_samples_preserve_units_missing_metrics_and_device_scope() {
        let devices =
            parse("GPU-ab12, 12288, 512, 25, 40.5\nGPU-cd34, N/A, N/A, [Not Supported], N/A\n")
                .unwrap();
        assert_eq!(devices[0].memory_total_bytes, Some(12884901888));
        assert_eq!(devices[0].memory_used_bytes, Some(536870912));
        assert_eq!(devices[0].power_watts, Some(40.5));
        assert!(devices[1].memory_total_bytes.is_none());
        assert!(devices[1].utilization_percent.is_none());
        assert!(crate::security::authorize("jev-r", "telemetry.gpu").is_err());
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn gpu_process_fixture() {
        match std::env::var("FABRIC_GPU_FIXTURE").as_deref() {
            Ok("overflow") => {
                print!("{}", "x".repeat(20000));
                std::process::exit(0);
            }
            Ok("stall") => {
                std::thread::sleep(std::time::Duration::from_secs(20));
                std::process::exit(0);
            }
            _ => {}
        }
    }
    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn gpu_query_bounds_and_reaps_silent_or_oversized_children() {
        use std::{process::Stdio, time::Duration};
        for mode in ["overflow", "stall"] {
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "gpu::tests::gpu_process_fixture", "--nocapture"])
                .env_clear()
                .env("FABRIC_GPU_FIXTURE", mode)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .stdout(Stdio::piped())
                .kill_on_drop(true);
            let error = run_query(command, Duration::from_millis(500))
                .await
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(if mode == "overflow" {
                    "bounds"
                } else {
                    "timeout"
                }),
                "{error}"
            );
        }
    }

    #[test]
    fn malformed_gpu_metrics_fail_without_partial_results() {
        for raw in [
            "",
            "GPU-aa, 1, 2, 0, 0",
            "GPU-aa, 2, 1, 101, 0",
            "GPU-aa, 2, 1, 0, NaN",
            "GPU-aa, 2.5, 1, 0, 0",
            "GPU-aa, 2, 1, -1, 0",
            "GPU-aa, 2, 1, 0, 0, extra",
            "unknown, 2, 1, 0, 0",
            "GPU-aa, 2, 1, 0, 0\nGPU-aa, 2, 1, 0, 0",
        ] {
            assert!(parse(raw).is_err(), "{raw}");
        }
        assert!(parse(&"GPU-aa, 2, 1, 0, 0\n".repeat(17)).is_err());
        assert!(parse(&"x".repeat(16385)).is_err());
    }
}
