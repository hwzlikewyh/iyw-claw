//! 仅保存请求身份，用于宿主按后端账单对账；不写正文或计费估算。
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::responses_metadata::{CodexResponsesMetadata, CodexResponsesRequestKind};

#[derive(Debug, Default)]
pub(crate) struct BillingReceipts {
    path: OnceLock<PathBuf>,
    writer: Mutex<()>,
}

impl BillingReceipts {
    pub(crate) fn set_rollout_path(&self, path: &Path) {
        let _ = self.path.set(path.with_extension("billing.jsonl"));
    }

    pub(crate) async fn record(&self, metadata: &CodexResponsesMetadata, request_id: Option<&str>) {
        let Some(request_id) = request_id.filter(|id| !id.is_empty() && id.len() <= 64) else {
            return;
        };
        let Some(path) = self.path.get() else {
            return;
        };
        let purpose = match metadata.request_kind {
            Some(CodexResponsesRequestKind::Compaction(_)) => "compaction",
            Some(CodexResponsesRequestKind::Turn) | None => "generation",
            _ => return,
        };
        let Some(turn_id) = metadata.turn_id.as_deref() else {
            return;
        };
        let line = serde_json::json!({
            "thread_id": metadata.thread_id, "turn_id": turn_id,
            "request_id": request_id, "purpose": purpose,
        })
        .to_string()
            + "\n";
        let write = self.append(path, &line);
        match tokio::time::timeout(std::time::Duration::from_secs(2), write).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, request_id, turn_id, "billing request association could not be persisted")
            }
            Err(_) => tracing::warn!(
                request_id,
                turn_id,
                "billing request association persistence timed out"
            ),
        }
    }

    async fn append(&self, path: &Path, line: &str) -> std::io::Result<()> {
        let _guard = self.writer.lock().await;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.sync_data().await
    }
}
