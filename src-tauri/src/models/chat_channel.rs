use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChannelInfo {
    pub id: i32,
    pub name: String,
    pub channel_type: String,
    /// Desired state: whether the user wants this channel active. This is
    /// NOT the same as "connected" — see `runtime_status`.
    pub enabled: bool,
    pub config_json: String,
    pub event_filter_json: Option<String>,
    pub daily_report_enabled: bool,
    pub daily_report_time: Option<String>,
    /// Last observed runtime state: saved | connecting | connected | error |
    /// disconnected. Never conflated with `enabled`.
    pub runtime_status: String,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatusInfo {
    pub channel_id: i32,
    pub name: String,
    pub channel_type: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChannelMessageLogInfo {
    pub id: i32,
    pub channel_id: i32,
    pub direction: String,
    pub message_type: String,
    pub content_preview: String,
    pub status: String,
    pub error_detail: Option<String>,
    pub trace_id: Option<String>,
    pub provider_message_id: Option<String>,
    pub created_at: String,
}

impl From<crate::db::entities::chat_channel::Model> for ChatChannelInfo {
    fn from(m: crate::db::entities::chat_channel::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            channel_type: m.channel_type,
            enabled: m.enabled,
            config_json: m.config_json,
            event_filter_json: m.event_filter_json,
            daily_report_enabled: m.daily_report_enabled,
            daily_report_time: m.daily_report_time,
            runtime_status: m.runtime_status,
            last_error: m.last_error,
            last_error_at: m.last_error_at.map(|v| v.to_rfc3339()),
            last_connected_at: m.last_connected_at.map(|v| v.to_rfc3339()),
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

impl From<crate::db::entities::chat_channel_message_log::Model> for ChatChannelMessageLogInfo {
    fn from(m: crate::db::entities::chat_channel_message_log::Model) -> Self {
        Self {
            id: m.id,
            channel_id: m.channel_id,
            direction: m.direction,
            message_type: m.message_type,
            content_preview: m.content_preview,
            status: m.status,
            error_detail: m.error_detail,
            trace_id: m.trace_id,
            provider_message_id: m.provider_message_id,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}
