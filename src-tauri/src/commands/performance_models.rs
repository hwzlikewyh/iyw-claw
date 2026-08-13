use serde::Serialize;

use crate::acp::types::ConnectionStatus;
use crate::models::agent::AgentType;

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppPerformanceStats {
    pub cpu_usage: f32,
    pub memory_used_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_memory_used_bytes: Option<u64>,
    pub os_info: OsInfo,
    pub processes: Vec<AppProcessInfo>,
    pub agent_sessions: Vec<AppAgentSessionInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_memory: Option<AppSystemMemoryInfo>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSystemMemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub pressure: String,
    pub shrinking_reserve_bytes: u64,
    pub emergency_reserve_bytes: u64,
    pub idle_agent_budget_bytes: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppAgentSessionInfo {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub conversation_title: Option<String>,
    pub agent_type: AgentType,
    pub status: ConnectionStatus,
    pub launcher_pid: Option<u32>,
    pub last_activity_at: chrono::DateTime<chrono::Utc>,
    pub private_memory_bytes: Option<u64>,
    pub memory_bytes: u64,
    pub process_count: usize,
    pub recoverable: bool,
    pub protection_reason: Option<String>,
    pub can_end: bool,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    pub os_name: String,
    pub arch: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppProcessInfo {
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<u32>,
    pub display_name: String,
    pub agent_type: Option<String>,
    pub is_main_process: bool,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_memory_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_role: Option<String>,
    pub status: String,
}
