//! 仅保存请求身份，用于宿主按后端账单对账；不写正文或计费估算。
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;

use crate::responses_metadata::{CodexResponsesMetadata, CodexResponsesRequestKind};

const WRITE_WAIT_BUDGET: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub(crate) struct BillingReceipts {
    path: OnceLock<PathBuf>,
    writer: Arc<Mutex<()>>,
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
            "task_id": metadata.billing_goal_id,
            "goal_identity_known": metadata.billing_goal_known,
            "root_turn_id": metadata.root_turn_id,
            "parent_thread_id": metadata.parent_thread_id,
        })
        .to_string()
            + "\n";
        self.persist(
            path.clone(),
            (line, request_id.to_owned(), turn_id.to_owned()),
        )
        .await;
    }

    async fn persist(&self, path: PathBuf, receipt: (String, String, String)) {
        let writer = Arc::clone(&self.writer);
        let (line, request_id, turn_id) = receipt;
        let mut write = tokio::spawn(async move {
            let _guard = writer.lock().await;
            // 单个阻塞任务完成追加和同步，取消等待不会释放串行锁或丢弃待写内容。
            let result = tokio::task::spawn_blocking(move || append(&path, &line)).await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(%error, request_id, turn_id, "billing request association could not be persisted")
                }
                Err(error) => {
                    tracing::error!(%error, request_id, turn_id, "billing request association writer failed")
                }
            }
        });
        if tokio::time::timeout(WRITE_WAIT_BUDGET, &mut write)
            .await
            .is_err()
        {
            tracing::debug!("billing request association write continues in background");
        }
    }
}

fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    file.sync_data()
}
