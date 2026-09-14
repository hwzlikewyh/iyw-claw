use std::sync::Arc;

use super::error::ChatChannelError;

pub const MIB: u64 = 1024 * 1024;
pub const MAX_INBOUND_BYTES: usize = 100 * MIB as usize;
pub const MAX_MESSAGE_ATTACHMENTS: usize = 10;

#[derive(Debug, Clone, Copy)]
pub struct AttachmentCapability {
    pub supported: bool,
    pub max_file_bytes: Option<u64>,
}

impl AttachmentCapability {
    pub const UNSUPPORTED: Self = Self {
        supported: false,
        max_file_bytes: None,
    };

    pub fn for_channel(channel_type: &str) -> Self {
        let limit = match channel_type {
            "lark" => 30 * MIB,
            "wecom" | "wecom_ai_bot" | "wecom_agent" | "dingtalk" => 20 * MIB,
            "weixin" => 100 * MIB,
            _ => return Self::UNSUPPORTED,
        };
        Self {
            supported: true,
            max_file_bytes: Some(limit),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelAttachment {
    pub name: String,
    pub mime_type: String,
    pub bytes: Arc<[u8]>,
}

impl ChannelAttachment {
    pub fn byte_len(&self) -> u64 {
        self.bytes.len() as u64
    }

    pub fn validate(&self, limit: u64) -> Result<(), ChatChannelError> {
        if self.bytes.is_empty() || self.byte_len() > limit {
            return Err(ChatChannelError::SendFailed(format!(
                "Attachment must contain 1 to {limit} bytes"
            )));
        }
        Ok(())
    }
}

// 平台资源凭据只驻留内存，不派生 Debug/Serialize，避免泄露下载密钥。
pub struct IncomingAttachment {
    pub name: String,
    pub mime_type: String,
    pub source: AttachmentSource,
}

pub enum AttachmentSource {
    Lark {
        message_id: String,
        key: String,
        kind: String,
    },
    WecomAiBot {
        url: String,
        aes_key: String,
    },
    WecomAgent {
        media_id: String,
    },
    WecomCli {
        media_id: String,
    },
    Dingtalk {
        download_code: String,
    },
    Weixin {
        url: String,
        aes_key: Option<String>,
    },
}
