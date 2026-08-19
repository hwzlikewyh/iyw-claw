use tokio_util::sync::CancellationToken;

use super::{require_runtime_client, runtime_enforcer, Capability, CapabilityRevocationMonitor};
use crate::acp::types::PromptInputBlock;
use crate::app_error::AppCommandError;

const FILE_UPLOAD_RUNTIME_VERIFIED: bool = true;

pub async fn require_file_upload() -> Result<(), AppCommandError> {
    require_runtime_client(Capability::FileUpload, FILE_UPLOAD_RUNTIME_VERIFIED).await
}

pub async fn monitor_file_upload(
    cancel_target: Option<CancellationToken>,
) -> Result<CapabilityRevocationMonitor, AppCommandError> {
    runtime_enforcer()?
        .monitor_client(
            Capability::FileUpload,
            FILE_UPLOAD_RUNTIME_VERIFIED,
            cancel_target,
        )
        .await
}

pub fn prompt_requires_file_upload(blocks: &[PromptInputBlock]) -> bool {
    blocks.iter().any(|block| match block {
        PromptInputBlock::Image { .. } => true,
        PromptInputBlock::Resource { blob, .. } => {
            blob.as_deref().is_some_and(|value| !value.is_empty())
        }
        PromptInputBlock::Text { .. } | PromptInputBlock::ResourceLink { .. } => false,
    })
}

pub async fn require_prompt_file_upload(
    blocks: &[PromptInputBlock],
) -> Result<(), AppCommandError> {
    if prompt_requires_file_upload(blocks) {
        require_file_upload().await?;
    }
    Ok(())
}

pub async fn monitor_prompt_file_upload(
    blocks: &[PromptInputBlock],
) -> Result<Option<CapabilityRevocationMonitor>, AppCommandError> {
    if !prompt_requires_file_upload(blocks) {
        return Ok(None);
    }
    monitor_file_upload(None).await.map(Some)
}
