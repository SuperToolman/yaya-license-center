use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use yaya_ops_protocol::{Capability, GpuMetrics, HostMetrics, MetricsSnapshot};

#[derive(Default)]
pub struct Collector {
    previous_cpu: Mutex<Option<(u64, u64)>>,
    sequence: AtomicU64,
}

pub fn cpu_percent(previous: Option<(u64, u64)>, current: (u64, u64)) -> Option<f64> {
    let (total, idle) = previous?;
    let delta = current.0.checked_sub(total)?;
    let idle_delta = current.1.checked_sub(idle)?;
    if delta == 0 || idle_delta > delta {
        return None;
    }
    Some((delta - idle_delta) as f64 / delta as f64 * 100.0)
}

impl Collector {
    pub fn sample(&self, node_id: &str, boot_id: &str) -> MetricsSnapshot {
        let host = self.host();
        let (gpu_capability, gpus) = nvidia();
        MetricsSnapshot {
            node_id: node_id.into(),
            boot_id: boot_id.into(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
            captured_at_ms: crate::now_ms(),
            host_capability: if host.is_some() {
                Capability::available()
            } else {
                Capability::unavailable("host_metrics_unavailable")
            },
            host,
            gpu_capability,
            gpus,
        }
    }

    fn host(&self) -> Option<HostMetrics> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let counters = stat
            .lines()
            .next()?
            .split_whitespace()
            .skip(1)
            .take(8)
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if counters.len() < 5 {
            return None;
        }
        let current = (
            counters.iter().sum(),
            counters[3].saturating_add(counters[4]),
        );
        let mut previous = self.previous_cpu.lock().ok()?;
        let cpu = cpu_percent(*previous, current);
        *previous = Some(current);
        let mem = std::fs::read_to_string("/proc/meminfo").ok()?;
        let read_mem = |key: &str| {
            mem.lines()
                .find_map(|line| line.strip_prefix(key))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
                .map(|v| v.saturating_mul(1024))
        };
        let total = read_mem("MemTotal:")?;
        let available = read_mem("MemAvailable:")?;
        Some(HostMetrics {
            cpu_percent: cpu,
            logical_cpus: stat
                .lines()
                .filter(|line| {
                    line.starts_with("cpu")
                        && line.as_bytes().get(3).is_some_and(u8::is_ascii_digit)
                })
                .count() as u32,
            memory_total_bytes: total,
            memory_used_bytes: total.saturating_sub(available),
            uptime_seconds: std::fs::read_to_string("/proc/uptime")
                .ok()?
                .split_whitespace()
                .next()?
                .parse()
                .ok()?,
        })
    }
}

#[cfg(target_os = "linux")]
pub fn nvidia() -> (Capability, Vec<GpuMetrics>) {
    use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};
    let nvml = match Nvml::init() {
        Ok(value) => value,
        Err(error) => {
            return (
                Capability::unavailable(match error {
                    nvml_wrapper::error::NvmlError::NoPermission => "permission_denied",
                    nvml_wrapper::error::NvmlError::DriverNotLoaded => "driver_unavailable",
                    _ => "library_or_driver_unavailable",
                }),
                vec![],
            );
        }
    };
    let count = match nvml.device_count() {
        Ok(count) => count,
        Err(_) => return (Capability::unavailable("device_query_failed"), vec![]),
    };
    if count == 0 {
        return (Capability::unavailable("no_device"), vec![]);
    }
    let gpus = (0..count.min(64))
        .filter_map(|i| {
            let device = nvml.device_by_index(i).ok()?;
            let memory = device.memory_info().ok();
            Some(GpuMetrics {
                uuid: device.uuid().ok()?,
                name: device.name().unwrap_or_else(|_| "NVIDIA GPU".into()),
                vendor: "nvidia".into(),
                utilization_percent: device.utilization_rates().ok().map(|v| v.gpu),
                memory_used_bytes: memory.as_ref().map(|v| v.used),
                memory_total_bytes: memory.map(|v| v.total),
                temperature_celsius: device.temperature(TemperatureSensor::Gpu).ok(),
                power_watts: device.power_usage().ok().map(|v| v as f64 / 1000.0),
            })
        })
        .collect::<Vec<_>>();
    if gpus.is_empty() {
        (Capability::unavailable("device_query_failed"), gpus)
    } else {
        (Capability::available(), gpus)
    }
}

#[cfg(not(target_os = "linux"))]
pub fn nvidia() -> (Capability, Vec<GpuMetrics>) {
    (Capability::unavailable("linux_required"), vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_initial_reset_and_zero_are_not_fake_zero() {
        assert_eq!(cpu_percent(None, (100, 50)), None);
        assert_eq!(cpu_percent(Some((100, 50)), (100, 50)), None);
        assert_eq!(cpu_percent(Some((100, 50)), (50, 20)), None);
        assert_eq!(cpu_percent(Some((100, 50)), (200, 75)), Some(75.0));
    }
}
