use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::chat_channel::types::ChannelType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QrStatus {
    Waiting,
    Scanned,
    Connecting,
    Connected,
    Expired,
    Denied,
    Cancelled,
    Error,
}

impl QrStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Connected | Self::Expired | Self::Denied | Self::Cancelled | Self::Error
        )
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrStartResponse {
    pub session_id: String,
    pub channel_id: i32,
    pub channel_type: ChannelType,
    pub qr_content: String,
    pub expires_at: DateTime<Utc>,
    pub status: QrStatus,
    pub retry_after_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrPollResponse {
    pub session_id: String,
    pub channel_id: i32,
    pub status: QrStatus,
    pub retry_after_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ProviderSession {
    Weixin {
        qrcode: String,
    },
    WecomAiBot {
        scode: String,
    },
    Dingtalk {
        device_code: String,
    },
    Lark {
        device_code: String,
        region: LarkRegion,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LarkRegion {
    Feishu,
    Lark,
}

impl LarkRegion {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("lark") | Some("larksuite") => Self::Lark,
            _ => Self::Feishu,
        }
    }

    pub fn config_value(self) -> &'static str {
        match self {
            Self::Feishu => "feishu",
            Self::Lark => "lark",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderStart {
    pub session: ProviderSession,
    pub qr_content: String,
    pub expires_in_secs: u64,
    pub retry_after_ms: u64,
}

#[derive(Debug, Clone)]
pub enum ProviderPoll {
    Waiting,
    Scanned,
    VerificationRequired,
    Approved(ProviderCredentials),
    Expired,
    Denied(&'static str),
}

#[derive(Debug, Clone)]
pub struct ProviderCredentials {
    pub token: String,
    pub config_patch: serde_json::Value,
}
