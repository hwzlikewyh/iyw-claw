use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine};

use super::attachments::{ChannelAttachment, MAX_INBOUND_BYTES, MAX_MESSAGE_ATTACHMENTS, MIB};
use super::manager::ChatChannelManager;
use super::types::IncomingCommand;
use crate::acp::types::PromptInputBlock;

pub type PromptMedia = Arc<[PromptInputBlock]>;
const MAX_PROMPT_IMAGE_BYTES: u64 = 20 * MIB;

#[derive(Default)]
struct StagedFiles(Vec<PathBuf>);

impl Drop for StagedFiles {
    fn drop(&mut self) {
        for file in &self.0 {
            let _ = std::fs::remove_file(file);
            if let Some(parent) = file.parent() {
                let _ = std::fs::remove_dir(parent);
            }
        }
    }
}

#[derive(Clone)]
pub struct ChannelPrompt {
    pub text: String,
    pub media: PromptMedia,
}

impl ChannelPrompt {
    pub fn new(text: impl Into<String>, media: &PromptMedia) -> Self {
        Self {
            text: text.into(),
            media: Arc::clone(media),
        }
    }

    pub fn blocks(&self) -> Vec<PromptInputBlock> {
        let mut blocks = vec![PromptInputBlock::Text {
            text: self.text.clone(),
        }];
        blocks.extend(self.media.iter().cloned());
        blocks
    }
}

impl From<String> for ChannelPrompt {
    fn from(text: String) -> Self {
        Self {
            text,
            media: PromptMedia::default(),
        }
    }
}

pub async fn prepare(
    manager: &ChatChannelManager,
    command: &IncomingCommand,
    data_dir: &Path,
) -> Result<PromptMedia, String> {
    if command.attachments.is_empty() {
        return Ok(PromptMedia::default());
    }
    if command.attachments.len() > MAX_MESSAGE_ATTACHMENTS {
        return Err(format!(
            "每条消息最多接收 {MAX_MESSAGE_ATTACHMENTS} 个附件。"
        ));
    }
    let monitor = crate::acp::capability_policy::monitor_file_upload(None)
        .await
        .map_err(|error| error.to_string())?;
    let mut blocks = Vec::new();
    let mut staged_files = StagedFiles::default();
    let mut budget = MediaBudget::default();
    let staging = StageContext {
        data_dir,
        trace_id: &command.message_trace_id,
        monitor: &monitor,
    };
    for source in &command.attachments {
        let file = manager
            .download_attachment(command.channel_id, source)
            .await
            .map_err(|error| format!("附件下载失败：{error}"))?;
        budget.reserve(&file)?;
        let staged = staging.write(&file).await?;
        staged_files.0.push(PathBuf::from(&staged.path));
        blocks.push(input_block(&file, &staged.path)?);
        tracing::info!(channel_id = command.channel_id, trace_id = %command.message_trace_id,
            bytes = file.byte_len(), mime_type = %file.mime_type, "[ChatChannel] inbound attachment staged");
    }
    staged_files.0.clear();
    Ok(Arc::from(blocks))
}

struct StageContext<'a> {
    data_dir: &'a Path,
    trace_id: &'a str,
    monitor: &'a crate::acp::capability_policy::CapabilityRevocationMonitor,
}

impl StageContext<'_> {
    async fn write(
        &self,
        file: &ChannelAttachment,
    ) -> Result<crate::commands::chat_attachments::StagedChatAttachment, String> {
        crate::commands::chat_attachments::stage_chat_attachment_bytes_core(
            self.data_dir,
            None,
            Some(self.trace_id),
            &file.name,
            &file.bytes,
            self.monitor,
        )
        .await
        .map_err(|error| format!("附件保存失败：{error}"))
    }
}

#[derive(Default)]
struct MediaBudget {
    total: u64,
    images: u64,
}

impl MediaBudget {
    fn reserve(&mut self, file: &ChannelAttachment) -> Result<(), String> {
        self.total = self.total.saturating_add(file.byte_len());
        if file.mime_type.starts_with("image/") {
            self.images = self.images.saturating_add(file.byte_len());
        }
        if self.total > MAX_INBOUND_BYTES as u64 || self.images > MAX_PROMPT_IMAGE_BYTES {
            return Err("附件总大小不能超过 100 MiB，图像输入合计不能超过 20 MiB。".into());
        }
        Ok(())
    }
}

fn input_block(file: &ChannelAttachment, path: &str) -> Result<PromptInputBlock, String> {
    let uri = reqwest::Url::from_file_path(path)
        .map_err(|_| "附件本地路径无效".to_string())?
        .to_string();
    if matches!(
        file.mime_type.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        return Ok(PromptInputBlock::Image {
            data: STANDARD.encode(&file.bytes),
            mime_type: file.mime_type.clone(),
            uri: Some(uri),
            local_path: Some(path.to_string()),
        });
    }
    Ok(PromptInputBlock::ResourceLink {
        uri,
        name: file.name.clone(),
        mime_type: Some(file.mime_type.clone()),
        description: Some("用户通过消息渠道发送的附件".into()),
    })
}
