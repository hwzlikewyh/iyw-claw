//! 仅在已授权的模型回合中准备旧图片，恢复历史本身不执行上传。
use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use codex_attachment_store::UploadRequest;
use codex_attachment_store::UploadResult;
use codex_protocol::models::{
    ContentItem, FunctionCallOutputContentItem, ImageReference, ResponseItem,
};
use sha2::{Digest, Sha256};

use super::session::Session;
use super::turn_context::TurnContext;

impl Session {
    pub(crate) async fn promote_legacy_images(&self, turn: &TurnContext) {
        if !self.services.model_client.uses_managed_image_provider() {
            return;
        }
        let history = self.clone_history().await;
        if !history
            .raw_items()
            .flat_map(item_images)
            .any(|image| matches!(image, ImageReference::Inline { .. }))
        {
            return;
        }
        let store = self
            .services
            .model_client
            .managed_image_store(
                &turn.model_info().slug,
                Arc::clone(&self.services.image_store),
            )
            .await;
        let references = upload_legacy_images(
            (&self.thread_id.to_string(), turn.config.ephemeral),
            &history,
            store.as_ref(),
        ).await;
        if references.is_empty() {
            return;
        }
        let mut state = self.state.lock().await;
        if state.history.reset_version == history.reset_version {
            let changed = state.history.promote_image_references(&references);
            tracing::info!(thread_id = %self.thread_id, images = changed,
                "legacy image references prepared without rewriting original rollout");
        }
    }
}

async fn upload_legacy_images(
    (thread_id, ephemeral): (&str, bool),
    history: &crate::context_manager::ContextManager,
    store: &dyn codex_attachment_store::AttachmentStore,
) -> HashMap<[u8; 32], ImageReference> {
    let mut references = HashMap::new();
    for image in history.raw_items().flat_map(item_images) {
        let ImageReference::Inline { image_url } = image else {
            continue;
        };
        let Some(data) = inline_bytes(image_url) else {
            continue;
        };
        let key: [u8; 32] = Sha256::digest(image_url.as_bytes()).into();
        if references.contains_key(&key) {
            continue;
        }
        match store
            .upload(UploadRequest {
                thread_id: thread_id.to_owned(),
                ephemeral,
                file_name: None,
                tags: HashMap::new(),
                data,
            })
            .await
        {
            Ok(UploadResult::File { file_id })
                if file_id.starts_with(crate::client::managed_images::IMAGE_ID_PREFIX) =>
            {
                references.insert(key, ImageReference::File { file_id });
            }
            // 未开启或不可用时保留全部原图片，不逐张重复探测失败的服务。
            _ => break,
        }
    }
    references
}

fn inline_bytes(url: &str) -> Option<Vec<u8>> {
    let value = url.strip_prefix("data:")?;
    let (metadata, payload) = value.split_once(',')?;
    if !metadata
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        return None;
    }
    STANDARD.decode(payload).ok()
}

fn item_images(item: &ResponseItem) -> Vec<&ImageReference> {
    match item {
        ResponseItem::Message { content, .. } => content
            .iter()
            .filter_map(|part| match part {
                ContentItem::InputImage { image, .. } => Some(image),
                _ => None,
            })
            .collect(),
        ResponseItem::FunctionCallOutput { output, .. }
        | ResponseItem::CustomToolCallOutput { output, .. } => output
            .content_items()
            .unwrap_or_default()
            .iter()
            .filter_map(|part| match part {
                FunctionCallOutputContentItem::InputImage { image, .. } => Some(image),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
