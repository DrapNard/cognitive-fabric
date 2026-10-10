//! Optional process-wide measurements; these do not measure external model servers.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProcessMemory {
    pub rss_bytes: Option<u64>,
    pub lifetime_peak_rss_bytes: Option<u64>,
}

/// Sample the calling Rust process. Missing procfs data remains unavailable.
pub fn process_memory() -> ProcessMemory {
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let sample = (|| {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open("/proc/self/status")
                .ok()?;
            let mut bytes = Vec::new();
            file.take(16385).read_to_end(&mut bytes).ok()?;
            if bytes.len() > 16384 {
                return None;
            }
            parse_status(std::str::from_utf8(&bytes).ok()?)
        })();
        sample.unwrap_or_default()
    }
    #[cfg(not(target_os = "linux"))]
    ProcessMemory::default()
}

#[cfg(any(target_os = "linux", test))]
fn parse_status(text: &str) -> Option<ProcessMemory> {
    let mut memory = ProcessMemory::default();
    for line in text.lines() {
        let (key, value) = match line.split_once(':') {
            Some(pair) => pair,
            None => continue,
        };
        let target = match key {
            "VmRSS" => &mut memory.rss_bytes,
            "VmHWM" => &mut memory.lifetime_peak_rss_bytes,
            _ => continue,
        };
        let mut fields = value.split_whitespace();
        let bytes = fields.next()?.parse::<u64>().ok()?.checked_mul(1024)?;
        if fields.next()? != "kB" || fields.next().is_some() || target.is_some() {
            return None;
        }
        *target = Some(bytes);
    }
    Some(memory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proc_metrics_preserve_units_missing_values_and_fail_closed() {
        let sample = parse_status("Name:\truntime\nVmRSS: 12 kB\nVmHWM: 20 kB\n").unwrap();
        assert_eq!(sample.rss_bytes, Some(12288));
        assert_eq!(sample.lifetime_peak_rss_bytes, Some(20480));
        assert!(parse_status("Name: runtime").unwrap().rss_bytes.is_none());
        for invalid in [
            "VmRSS: 1 MB",
            "VmRSS: -1 kB",
            "VmHWM: 18446744073709551615 kB",
            "VmRSS: 1 kB\nVmRSS: 2 kB",
            "VmHWM: 1 kB extra",
        ] {
            assert!(parse_status(invalid).is_none());
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn live_process_measurements_are_optional_and_serializable() {
        let sample = process_memory();
        if let Some(rss) = sample.rss_bytes {
            assert!(rss > 0);
        }
        let restored: ProcessMemory =
            serde_json::from_value(serde_json::to_value(&sample).unwrap()).unwrap();
        assert_eq!(restored.rss_bytes, sample.rss_bytes);
    }
}
