use serde::{Deserialize, Serialize};

use super::types_cdp::{
    BrowserDialogSnapshot, BrowserDownloadSnapshot, BrowserFileChooserSnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeStatus {
    Unsupported,
    Missing,
    Verifying,
    Ready,
    Starting,
    Running,
    Recovering,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTabStatus {
    Creating,
    Live,
    Navigating,
    Crashed,
    Gone,
    Closing,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserViewStatus {
    Unclaimed,
    Attaching,
    Docked,
    Detaching,
    Detached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserControlStatus {
    Idle,
    AgentRunning,
    UserActive,
    UserHeld,
    AgentWaiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostKind {
    Docked,
    Detached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEngineKind {
    Chrome,
    Edge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAgentIdentity {
    pub connection_id: String,
    pub conversation_id: Option<i32>,
    pub turn_generation: i64,
}

#[derive(Debug)]
pub struct BrowserAgentToolCall {
    pub identity: BrowserAgentIdentity,
    pub tool: String,
    pub input: serde_json::Value,
    pub cancellation: tokio_util::sync::CancellationToken,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserGenerations {
    pub runtime_generation: u64,
    pub tab_generation: u64,
    pub view_generation: u64,
    pub control_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCapability {
    pub supported: bool,
    pub status: BrowserRuntimeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub platform: String,
    pub architecture: String,
    pub sidecar_version: String,
    pub sidecar_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<BrowserEngineSummary>,
}

impl BrowserCapability {
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            supported: false,
            status: BrowserRuntimeStatus::Unsupported,
            reason: Some(reason.into()),
            platform: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            sidecar_version: "0.34.0".to_string(),
            sidecar_verified: false,
            engine: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserEngineSummary {
    pub kind: BrowserEngineKind,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeSnapshot {
    pub status: BrowserRuntimeStatus,
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabSnapshot {
    pub browser_tab_id: String,
    pub title: String,
    pub url: String,
    pub status: BrowserTabStatus,
    pub view_status: BrowserViewStatus,
    pub control_status: BrowserControlStatus,
    pub document_epoch: u64,
    pub generations: BrowserGenerations,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostSnapshot {
    pub host_id: String,
    pub window_label: String,
    pub kind: BrowserHostKind,
    pub generation: u64,
    pub visible: bool,
    pub tab_order: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tab_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStateSnapshot {
    pub state_revision: u64,
    pub capability: BrowserCapability,
    pub runtime: BrowserRuntimeSnapshot,
    pub tabs: Vec<BrowserTabSnapshot>,
    pub hosts: Vec<BrowserHostSnapshot>,
    pub dialogs: Vec<BrowserDialogSnapshot>,
    pub file_choosers: Vec<BrowserFileChooserSnapshot>,
    pub downloads: Vec<BrowserDownloadSnapshot>,
    pub view_claims: Vec<BrowserViewClaimSnapshot>,
    pub user_action_requests: Vec<BrowserUserActionRequestSnapshot>,
    pub window_open_requests: Vec<BrowserWindowOpenRequestSnapshot>,
    pub window_close_requests: Vec<BrowserWindowCloseRequestSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserUserActionRequestSnapshot {
    pub request_id: String,
    pub browser_tab_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWindowCloseRequestSnapshot {
    pub request_id: String,
    pub browser_tab_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWindowOpenRequestSnapshot {
    pub request_id: String,
    pub browser_tab_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserOperationSource {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFrameSubscriptionStatus {
    Connecting,
    Streaming,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFrameSubscriptionSnapshot {
    pub subscription_id: String,
    pub browser_tab_id: String,
    pub generations: BrowserGenerations,
    pub status: BrowserFrameSubscriptionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserViewClaimSnapshot {
    pub claim_id: String,
    pub browser_tab_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_host_id: Option<String>,
    pub target_host_id: String,
    pub target_index: usize,
    pub target_status: BrowserViewStatus,
    pub generations: BrowserGenerations,
    pub first_frame_seq: Option<u64>,
    pub expires_in_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostRegistration {
    pub host_id: String,
    pub generation: u64,
    pub state: BrowserStateSnapshot,
}
