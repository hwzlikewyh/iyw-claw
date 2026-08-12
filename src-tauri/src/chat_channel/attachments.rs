use std::sync::Arc;

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
}
