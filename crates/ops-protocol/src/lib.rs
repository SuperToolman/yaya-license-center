//! Versioned, read-only contracts. No business credentials or shell commands cross this boundary.
use serde::{Deserialize, Serialize};

#[cfg(feature = "unix-http")]
pub mod local;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const CONTAINER_LIMIT: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub available: bool,
    pub reason: Option<String>,
}

impl Capability {
    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostMonitorHello {
    pub node_id: String,
    pub name: String,
    pub boot_id: String,
    // Keep the v1 JSON field compatible with stored node snapshots and the UI.
    #[serde(rename = "agentVersion")]
    pub monitor_version: String,
    pub protocol_version: u32,
    pub docker: Capability,
    pub host_metrics: Capability,
    pub nvidia: Capability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostMetrics {
    pub cpu_percent: Option<f64>,
    pub logical_cpus: u32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub uptime_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuMetrics {
    pub uuid: String,
    pub name: String,
    pub vendor: String,
    pub utilization_percent: Option<u32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub temperature_celsius: Option<u32>,
    pub power_watts: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    pub node_id: String,
    pub boot_id: String,
    pub sequence: u64,
    pub captured_at_ms: i64,
    pub host: Option<HostMetrics>,
    pub host_capability: Capability,
    pub gpu_capability: Capability,
    pub gpus: Vec<GpuMetrics>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerSummary {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerList {
    pub items: Vec<ContainerSummary>,
    pub limit: usize,
    pub truncated: bool,
    pub captured_at_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostMonitorError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostMonitorReply<T> {
    pub data: Option<T>,
    pub error: Option<HostMonitorError>,
}

impl<T> HostMonitorReply<T> {
    pub fn success(data: T) -> Self {
        Self {
            data: Some(data),
            error: None,
        }
    }
    pub fn failure(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            data: None,
            error: Some(HostMonitorError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullable_metrics_and_error_contract_roundtrip() {
        let gpu = GpuMetrics {
            uuid: "GPU-test".into(),
            name: "test".into(),
            vendor: "nvidia".into(),
            utilization_percent: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: None,
            power_watts: None,
        };
        let value = serde_json::to_value(gpu).unwrap();
        assert!(value["powerWatts"].is_null());
        let error = HostMonitorReply::<HostMonitorHello>::failure("DOCKER_UNAVAILABLE", "unavailable");
        let decoded: HostMonitorReply<HostMonitorHello> =
            serde_json::from_value(serde_json::to_value(error).unwrap()).unwrap();
        assert!(decoded.data.is_none());
        assert_eq!(decoded.error.unwrap().code, "DOCKER_UNAVAILABLE");
    }
}
