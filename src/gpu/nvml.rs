//! Same-image, read-only NVML child and its bounded FlatBuffers result.
use super::Device;
use anyhow::{bail, Result};

#[allow(dead_code, unused_imports, clippy::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/gpu_generated.rs"));
}
use generated::telemetry::gpu as fb;

const MAX_FRAME: usize = 16 * 1024;
const MAX_DEVICES: usize = 16;
const MAX_MEMORY_BYTES: u64 = 16_777_216 * 1024 * 1024;

/// Hidden same-image worker entry point. The server and CLI delegate this
/// before loading configuration or opening runtime state.
pub fn run_worker() -> Result<()> {
    #[cfg(feature = "nvml-gpu")]
    {
        let devices = query_devices()?;
        let sample = fb::SampleT {
            version: 1,
            devices: devices
                .into_iter()
                .map(|device| fb::DeviceT {
                    uuid: device.uuid,
                    memory_total_bytes: device.memory_total_bytes.unwrap_or_default(),
                    has_memory_total: device.memory_total_bytes.is_some(),
                    memory_used_bytes: device.memory_used_bytes.unwrap_or_default(),
                    has_memory_used: device.memory_used_bytes.is_some(),
                    utilization_percent: device.utilization_percent.unwrap_or_default(),
                    has_utilization: device.utilization_percent.is_some(),
                    power_watts: device.power_watts.unwrap_or_default(),
                    has_power: device.power_watts.is_some(),
                })
                .collect(),
        };
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let root = sample.pack(&mut builder);
        fb::finish_sample_buffer(&mut builder, root);
        let bytes = builder.finished_data();
        if bytes.len() > MAX_FRAME {
            bail!("NVML sample exceeds the protocol frame bound")
        }
        use std::io::Write;
        std::io::stdout().lock().write_all(bytes)?;
        return Ok(());
    }
    #[cfg(not(feature = "nvml-gpu"))]
    bail!("NVML support is not compiled in")
}

#[cfg(feature = "nvml-gpu")]
fn query_devices() -> Result<Vec<Device>> {
    use nvml_wrapper::Nvml;
    let nvml = Nvml::init().map_err(|_| anyhow::anyhow!("NVML initialization failed"))?;
    let count = nvml
        .device_count()
        .map_err(|_| anyhow::anyhow!("NVML device enumeration failed"))?;
    if count == 0 || count as usize > MAX_DEVICES {
        bail!("NVML device count is outside the supported bound")
    }
    let mut devices = Vec::with_capacity(count as usize);
    for index in 0..count {
        let device = nvml
            .device_by_index(index)
            .map_err(|_| anyhow::anyhow!("NVML device query failed"))?;
        let uuid = device
            .uuid()
            .map_err(|_| anyhow::anyhow!("NVML device identity query failed"))?;
        if !valid_uuid(&uuid) || devices.iter().any(|seen: &Device| seen.uuid == uuid) {
            bail!("NVML returned an invalid or duplicate device identity")
        }
        // Unsupported or unavailable individual sensors remain unknown. The
        // response never substitutes zero for a missing measurement.
        let memory = device.memory_info().ok();
        let utilization = device.utilization_rates().ok();
        let power = device.power_usage().ok();
        devices.push(Device {
            uuid,
            memory_total_bytes: memory.as_ref().map(|info| info.total),
            memory_used_bytes: memory.as_ref().map(|info| info.used),
            utilization_percent: utilization.map(|rates| f64::from(rates.gpu)),
            power_watts: power.map(|milliwatts| f64::from(milliwatts) / 1000.0),
        });
    }
    Ok(devices)
}

fn valid_uuid(uuid: &str) -> bool {
    (5..=80).contains(&uuid.len())
        && uuid.starts_with("GPU-")
        && uuid[4..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

pub(super) fn decode_sample(bytes: &[u8]) -> Result<Vec<Device>> {
    if !(8..=MAX_FRAME).contains(&bytes.len()) || !fb::sample_buffer_has_identifier(bytes) {
        bail!("Invalid NVML FlatBuffers frame")
    }
    let options = flatbuffers::VerifierOptions {
        max_depth: 8,
        max_tables: 64,
        max_apparent_size: MAX_FRAME,
        ..Default::default()
    };
    let root = flatbuffers::root_with_opts::<fb::Sample>(&options, bytes)?;
    let raw_devices = root.devices();
    if raw_devices.is_empty() || raw_devices.len() > MAX_DEVICES {
        bail!("NVML device list exceeds the supported bound")
    }
    let sample = root.unpack();
    if sample.version != 1 || sample.devices.len() != raw_devices.len() {
        bail!("Unsupported NVML sample version or shape")
    }
    let mut devices = Vec::with_capacity(sample.devices.len());
    for raw in sample.devices {
        let uuid = raw.uuid;
        if !valid_uuid(&uuid) || devices.iter().any(|seen: &Device| seen.uuid == uuid) {
            bail!("Invalid or duplicate NVML device identity")
        }
        let total = raw.has_memory_total.then_some(raw.memory_total_bytes);
        let used = raw.has_memory_used.then_some(raw.memory_used_bytes);
        if total.is_some_and(|bytes| bytes > MAX_MEMORY_BYTES)
            || used.is_some_and(|bytes| bytes > MAX_MEMORY_BYTES)
        {
            bail!("NVML memory value exceeds the supported bound")
        }
        if total.zip(used).is_some_and(|(total, used)| used > total) {
            bail!("NVML memory use exceeds total memory")
        }
        let utilization = raw.has_utilization.then_some(raw.utilization_percent);
        if utilization.is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value)) {
            bail!("NVML utilization is outside the supported range")
        }
        let power = raw.has_power.then_some(raw.power_watts);
        if power.is_some_and(|value| !value.is_finite() || !(0.0..=10000.0).contains(&value)) {
            bail!("NVML power is outside the supported range")
        }
        devices.push(Device {
            uuid,
            memory_total_bytes: total,
            memory_used_bytes: used,
            utilization_percent: utilization,
            power_watts: power,
        });
    }
    Ok(devices)
}
