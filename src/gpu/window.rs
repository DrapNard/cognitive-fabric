//! Optional bounded device observations during an execution future.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, future::Future, time::Duration};
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub interval_ms: u64,
    pub max_samples: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            max_samples: 600,
        }
    }
}
impl Settings {
    fn validate(self) -> Result<()> {
        crate::security::authorize("human-cli", "telemetry.gpu")?;
        if !(250..=5000).contains(&self.interval_ms) || !(1..=600).contains(&self.max_samples) {
            bail!("GPU observation cadence or run-wide sample bound is invalid")
        }
        Ok(())
    }
}
/// One irreversible query budget shared by every window in a comparison run.
pub struct Meter {
    settings: Settings,
    remaining: u32,
}
impl Meter {
    pub fn new(settings: Settings) -> Result<Self> {
        settings.validate()?;
        Ok(Self {
            settings,
            remaining: settings.max_samples,
        })
    }
    pub fn remaining_samples(&self) -> u32 {
        self.remaining
    }
    pub async fn observe<T>(&mut self, execution: impl Future<Output = T>) -> (T, Value) {
        self.observe_with(execution, crate::gpu::sample).await
    }
    async fn observe_with<T, F, S>(
        &mut self,
        execution: impl Future<Output = T>,
        mut sample: F,
    ) -> (T, Value)
    where
        F: FnMut() -> S,
        S: Future<Output = Result<Value>>,
    {
        let started = Instant::now();
        let mut next = started;
        let mut trace = Trace::default();
        tokio::pin!(execution);
        loop {
            if self.remaining == 0 {
                let result = execution.await;
                return (
                    result,
                    trace.report(started.elapsed(), true, Duration::ZERO),
                );
            }
            tokio::select! {
                biased;
                result=&mut execution=>return (result,trace.report(started.elapsed(),false,Duration::ZERO)),
                _=tokio::time::sleep_until(next)=>{}
            }
            self.remaining -= 1;
            trace.queries += 1;
            let query_started = Instant::now();
            let query = sample();
            tokio::pin!(query);
            tokio::select! {
                biased;
                result=&mut execution=>{
                    let duration=started.elapsed();
                    // The fixed GPU sampler owns and reaps its child. Finish its
                    // bounded cleanup instead of dropping a running query.
                    let _=query.await;
                    return (result,trace.report(duration,false,started.elapsed().saturating_sub(duration)));
                }
                observation=&mut query=>{
                    let ended=Instant::now();
                    let midpoint=query_started.duration_since(started)+(ended-query_started)/2;
                    trace.record(midpoint,observation.ok());
                    next=ended+Duration::from_millis(self.settings.interval_ms);
                }
            }
        }
    }
}

#[derive(Clone)]
struct Point {
    power: Option<f64>,
}
#[derive(Default)]
struct Device {
    samples: u32,
    max_memory: Option<u64>,
    max_utilization: Option<f64>,
    observed_joules: f64,
    covered_seconds: f64,
    segments: u32,
}
type SampledDevices = BTreeMap<String, (Point, Option<u64>, Option<f64>)>;
#[derive(Default)]
struct Trace {
    queries: u32,
    accepted: u32,
    unavailable: u32,
    identity_changed: bool,
    devices: BTreeMap<String, Device>,
    previous: Option<(Duration, BTreeMap<String, Point>)>,
}
impl Trace {
    fn record(&mut self, time: Duration, value: Option<Value>) {
        let parsed = (|| -> Option<SampledDevices> {
            let value = value?;
            if value["status"] != "sampled"
                || value["scope"] != "whole_device"
                || value["task_attributed"] != false
            {
                return None;
            }
            let devices = value["devices"].as_array()?;
            if devices.is_empty() || devices.len() > 16 {
                return None;
            }
            let mut points = BTreeMap::new();
            for device in devices {
                let id = device["uuid"].as_str()?.to_owned();
                if id.len() > 80 || !id.starts_with("GPU-") {
                    return None;
                }
                let power = if device["power_watts"].is_null() {
                    None
                } else {
                    Some(
                        device["power_watts"]
                            .as_f64()
                            .filter(|n| n.is_finite() && (0.0..=10000.0).contains(n))?,
                    )
                };
                let memory = if device["memory_used_bytes"].is_null() {
                    None
                } else {
                    Some(device["memory_used_bytes"].as_u64()?)
                };
                let utilization = if device["utilization_percent"].is_null() {
                    None
                } else {
                    Some(
                        device["utilization_percent"]
                            .as_f64()
                            .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))?,
                    )
                };
                if points
                    .insert(id, (Point { power }, memory, utilization))
                    .is_some()
                {
                    return None;
                }
            }
            Some(points)
        })();
        let Some(points) = parsed else {
            self.unavailable += 1;
            self.previous = None;
            return;
        };
        if self.devices.keys().any(|id| !points.contains_key(id))
            || points
                .keys()
                .any(|id| !self.devices.is_empty() && !self.devices.contains_key(id))
        {
            self.identity_changed = true;
            self.previous = None;
        }
        if points.keys().any(|id| !self.devices.contains_key(id))
            && self.devices.len()
                + points
                    .keys()
                    .filter(|id| !self.devices.contains_key(*id))
                    .count()
                > 16
        {
            self.unavailable += 1;
            self.previous = None;
            return;
        }
        self.accepted += 1;
        for (id, (point, memory, utilization)) in &points {
            let device = self.devices.entry(id.clone()).or_default();
            device.samples += 1;
            device.max_memory = device.max_memory.max(*memory);
            if let Some(value) = utilization {
                device.max_utilization = Some(
                    device
                        .max_utilization
                        .map_or(*value, |previous| previous.max(*value)),
                );
            }
            if let Some((prior, previous)) = &self.previous {
                if let Some((old, new)) = previous.get(id).and_then(|p| p.power).zip(point.power) {
                    let seconds = time.saturating_sub(*prior).as_secs_f64();
                    if time > *prior && seconds <= 10.0 {
                        device.observed_joules += (old + new) * 0.5 * seconds;
                        device.covered_seconds += seconds;
                        device.segments += 1;
                    }
                }
            }
        }
        self.previous = Some((
            time,
            points
                .into_iter()
                .map(|(id, (point, _, _))| (id, point))
                .collect(),
        ));
    }
    fn report(self, duration: Duration, exhausted: bool, tail: Duration) -> Value {
        json!({"format_version":1,"scope":"whole_device_during_execution_window","task_attributed":false,
            "task_energy_joules":null,"true_peak_memory_known":false,"full_window_energy_known":false,
            "window_ms":duration.as_secs_f64()*1000.0,"query_cleanup_tail_ms":tail.as_secs_f64()*1000.0,
            "queries_charged":self.queries,"samples_within_window":self.accepted,"unavailable_samples":self.unavailable,
            "queries_not_sampled_in_window":self.queries.saturating_sub(self.accepted+self.unavailable),
            "run_sample_budget_exhausted":exhausted,"device_identity_changed":self.identity_changed,
            "estimate_method":"trapezoid-between-consecutive-observed-query-midpoints/v1",
            "devices":self.devices.into_iter().map(|(id,d)|json!({"uuid":id,"samples":d.samples,
                "max_sampled_memory_bytes":d.max_memory,"max_sampled_utilization_percent":d.max_utilization,
                "observed_interval_energy_estimate_joules":if d.segments>0 && !self.identity_changed {Some(d.observed_joules)}else{None},
                "covered_interval_ms":d.covered_seconds*1000.0,"integrated_intervals":d.segments})).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(id: &str, power: Value, memory: u64) -> Value {
        json!({"status":"sampled","scope":"whole_device","task_attributed":false,
            "devices":[{"uuid":id,"power_watts":power,"memory_used_bytes":memory,"utilization_percent":25.0}]})
    }
    #[test]
    fn integrates_only_observed_intervals_and_never_claims_task_energy_or_true_peak() {
        let mut trace = Trace {
            queries: 2,
            ..Default::default()
        };
        trace.record(Duration::ZERO, Some(sample("GPU-aa", json!(10.0), 100)));
        trace.record(
            Duration::from_secs(2),
            Some(sample("GPU-aa", json!(30.0), 200)),
        );
        let report = trace.report(Duration::from_secs(3), false, Duration::ZERO);
        assert_eq!(
            report["devices"][0]["observed_interval_energy_estimate_joules"],
            40.0
        );
        assert_eq!(report["devices"][0]["covered_interval_ms"], 2000.0);
        assert_eq!(report["devices"][0]["max_sampled_memory_bytes"], 200);
        assert_eq!(report["task_attributed"], false);
        assert!(report["task_energy_joules"].is_null());
        assert_eq!(report["full_window_energy_known"], false);
        assert_eq!(report["true_peak_memory_known"], false);
    }
    #[test]
    fn missing_power_errors_large_gaps_and_identity_changes_cannot_fabricate_energy() {
        for scenario in 0..4 {
            let mut trace = Trace::default();
            trace.record(Duration::ZERO, Some(sample("GPU-aa", json!(10.0), 100)));
            match scenario {
                0 => trace.record(Duration::from_secs(1), None),
                1 => trace.record(
                    Duration::from_secs(1),
                    Some(sample("GPU-aa", Value::Null, 100)),
                ),
                2 => {}
                3 => trace.record(
                    Duration::from_secs(1),
                    Some(sample("GPU-bb", json!(10.0), 100)),
                ),
                _ => unreachable!(),
            }
            trace.record(
                Duration::from_secs(if scenario == 2 { 20 } else { 2 }),
                Some(sample("GPU-aa", json!(30.0), 200)),
            );
            let report = trace.report(Duration::from_secs(21), false, Duration::ZERO);
            assert!(
                report["devices"][0]["observed_interval_energy_estimate_joules"].is_null(),
                "scenario {scenario}"
            );
            assert!(report["task_energy_joules"].is_null());
        }
    }
    #[tokio::test(start_paused = true)]
    async fn query_budget_is_shared_irreversibly_across_windows() {
        let mut meter = Meter::new(Settings {
            interval_ms: 250,
            max_samples: 3,
        })
        .unwrap();
        let (result, report) = meter
            .observe_with(
                async {
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    42
                },
                || async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(sample("GPU-aa", json!(20.0), 100))
                },
            )
            .await;
        assert_eq!(result, 42);
        assert_eq!(report["queries_charged"], 3);
        assert_eq!(report["samples_within_window"], 3);
        assert_eq!(report["run_sample_budget_exhausted"], true);
        assert_eq!(meter.remaining_samples(), 0);
        let (_, report) = meter
            .observe_with(async { 0 }, || async {
                panic!("Exhausted budget must not query again")
            })
            .await;
        assert_eq!(report["queries_charged"], 0);
        assert!(report["devices"].as_array().unwrap().is_empty());
    }
    #[tokio::test(start_paused = true)]
    async fn completing_execution_drains_inflight_query_without_counting_a_late_sample() {
        let mut meter = Meter::new(Settings {
            interval_ms: 250,
            max_samples: 3,
        })
        .unwrap();
        let (result, report) = meter
            .observe_with(
                async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    "done"
                },
                || async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok(sample("GPU-aa", json!(20.0), 100))
                },
            )
            .await;
        assert_eq!(result, "done");
        assert_eq!(report["window_ms"], 10.0);
        assert_eq!(report["query_cleanup_tail_ms"], 40.0);
        assert_eq!(report["samples_within_window"], 0);
        assert_eq!(report["queries_not_sampled_in_window"], 1);
        assert_eq!(meter.remaining_samples(), 2);
    }
    #[test]
    fn invalid_query_limits_are_refused() {
        for settings in [
            Settings {
                interval_ms: 249,
                max_samples: 1,
            },
            Settings {
                interval_ms: 5001,
                max_samples: 1,
            },
            Settings {
                interval_ms: 1000,
                max_samples: 0,
            },
            Settings {
                interval_ms: 1000,
                max_samples: 601,
            },
        ] {
            assert!(Meter::new(settings).is_err());
        }
    }
}
