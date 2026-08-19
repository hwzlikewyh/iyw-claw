use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessStage {
    pub key: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReadinessReport {
    pub channel_id: i32,
    pub name: String,
    pub channel_type: String,
    pub enabled: bool,
    pub runtime_status: String,
    pub transport_connected: bool,
    pub callback_verified: bool,
    pub saved: bool,
    pub credential_ready: bool,
    pub inbound_verified: bool,
    pub workspace_ready: bool,
    pub agent_ready: bool,
    pub gateway_ready: bool,
    pub roundtrip_ready: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    pub last_connected_at: Option<String>,
    pub last_inbound_at: Option<String>,
    pub inbound_count: u64,
    pub stages: Vec<ReadinessStage>,
}
