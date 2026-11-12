use std::sync::OnceLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sysinfo::{MacAddr, Networks};

use crate::app_error::AppCommandError;

mod probe;

#[derive(Debug, Serialize)]
pub struct NetworkInterfaceTraffic {
    pub name: String,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct NetworkTrafficSnapshot {
    pub sampled_at_ms: u64,
    pub interfaces: Vec<NetworkInterfaceTraffic>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProbeKind {
    Latency,
    Download,
    Upload,
}

#[derive(Debug, Serialize)]
pub struct NetworkProbeResult {
    pub kind: NetworkProbeKind,
    pub target: &'static str,
    pub elapsed_ms: f64,
    pub bytes: usize,
    pub mbps: Option<f64>,
}

pub async fn get_network_traffic_core() -> Result<NetworkTrafficSnapshot, AppCommandError> {
    tokio::task::spawn_blocking(collect_traffic)
        .await
        .map_err(|error| {
            tracing::error!(%error, "network traffic sampling task failed");
            AppCommandError::task_execution_failed("Network sampling failed")
                .with_detail(error.to_string())
        })
}

fn collect_traffic() -> NetworkTrafficSnapshot {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    let started = STARTED.get_or_init(Instant::now);
    let networks = Networks::new_with_refreshed_list();
    // 排除 loopback 和无硬件地址的隧道，避免把同一份流量重复计数。
    let mut interfaces = networks
        .iter()
        .filter(|(_, data)| data.mac_address() != MacAddr::UNSPECIFIED)
        .map(|(name, data)| NetworkInterfaceTraffic {
            name: name.clone(),
            received_bytes: data.total_received(),
            transmitted_bytes: data.total_transmitted(),
        })
        .collect::<Vec<_>>();
    interfaces.sort_by(|left, right| left.name.cmp(&right.name));
    NetworkTrafficSnapshot {
        sampled_at_ms: started.elapsed().as_millis() as u64,
        interfaces,
    }
}

pub async fn measure_network_speed_core(
    kind: NetworkProbeKind,
) -> Result<NetworkProbeResult, AppCommandError> {
    probe::measure(kind).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_network_traffic() -> Result<NetworkTrafficSnapshot, AppCommandError> {
    get_network_traffic_core().await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn measure_network_speed(
    kind: NetworkProbeKind,
) -> Result<NetworkProbeResult, AppCommandError> {
    measure_network_speed_core(kind).await
}
