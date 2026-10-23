//! Opt-in operator GPU observations, never task-attributed resource accounting.
pub mod window;
#[cfg(any(target_os = "linux", test))]
use anyhow::bail;
use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};
#[cfg(any(target_os = "linux", test))]
use std::collections::BTreeSet;

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
async fn query() -> Result<Vec<Device>> {
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
async fn run_query(
    mut command: tokio::process::Command,
    limit: std::time::Duration,
) -> Result<Vec<Device>> {
    use anyhow::Context;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().context("Missing GPU output pipe")?;
    let result = tokio::time::timeout(limit, async {
        let mut bytes = Vec::new();
        stdout.take(16385).read_to_end(&mut bytes).await?;
        if bytes.len() > 16384 {
            bail!("GPU response exceeds bounds")
        }
        if !child.wait().await?.success() {
            bail!("GPU driver query failed")
        }
        parse(std::str::from_utf8(&bytes)?)
    })
    .await;
    match result {
        Ok(Ok(devices)) => Ok(devices),
        failure => {
            let _ = child.start_kill();
            let reaped = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            if !matches!(reaped, Ok(Ok(_))) {
                bail!("GPU query cleanup failed")
            }
            match failure {
                Ok(Err(error)) => Err(error),
                _ => bail!("GPU query timeout"),
            }
        }
    }
}

/// Operator-only, one-shot sampling. Failure preserves unavailable measurements.
pub async fn sample() -> Result<Value> {
    crate::security::authorize("human-cli", "telemetry.gpu")?;
    #[cfg(target_os = "linux")]
    let result = query().await;
    #[cfg(not(target_os = "linux"))]
    let result: Result<Vec<Device>> = Err(anyhow::anyhow!("GPU query requires Linux"));
    Ok(match result {
        Ok(devices) => {
            json!({"status":"sampled","devices":devices,"scope":"whole_device","task_attributed":false,"energy_joules":null})
        }
        Err(error) => {
            json!({"status":"unavailable","devices":null,"reason":error.to_string().chars().take(200).collect::<String>(),"scope":"whole_device","task_attributed":false,"energy_joules":null})
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
