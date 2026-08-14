use serde_json::Value;

use super::manager::BrowserSessionManager;
use super::types_cdp::{BrowserDialogKind, BrowserDownloadStatus, BrowserFileChooserMode};

impl BrowserSessionManager {
    pub(super) async fn handle_cdp_event(
        &self,
        generation: u64,
        method: &str,
        params: Value,
        session_id: Option<&str>,
        target_id: Option<String>,
        frame_target: Option<String>,
    ) {
        if self.state.read().await.runtime.generation != generation {
            return;
        }
        match method {
            "Target.targetCreated" => self.handle_target_created(generation, params),
            "Target.targetDestroyed" => self.handle_target_failure(generation, params, false).await,
            "Target.targetCrashed" => self.handle_target_failure(generation, params, true).await,
            "Target.targetInfoChanged" => self.handle_target_info(params).await,
            "Page.javascriptDialogOpening" => {
                self.handle_dialog_open(params, session_id, target_id).await
            }
            "Page.javascriptDialogClosed" => self.handle_dialog_closed(target_id).await,
            "Page.fileChooserOpened" => {
                self.handle_file_chooser(params, session_id, target_id)
                    .await
            }
            "Page.lifecycleEvent" => self.handle_lifecycle(params, target_id).await,
            "Browser.downloadWillBegin" => self.handle_download_begin(params, frame_target).await,
            "Browser.downloadProgress" => self.handle_download_progress(params).await,
            _ => {}
        }
    }

    fn handle_target_created(&self, generation: u64, params: Value) {
        let Some(info) = params.get("targetInfo") else {
            return;
        };
        if info.get("type").and_then(Value::as_str) != Some("page") {
            return;
        }
        let (Some(target_id), Some(opener_id)) = (
            info.get("targetId").and_then(Value::as_str),
            info.get("openerId").and_then(Value::as_str),
        ) else {
            return;
        };
        let url = info
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("about:blank");
        let url = if url.is_empty() { "about:blank" } else { url };
        let manager = self.clone();
        let target_id = target_id.to_string();
        let opener_id = opener_id.to_string();
        let url = url.to_string();
        tokio::spawn(async move {
            manager
                .adopt_popup(generation, target_id, opener_id, url)
                .await;
        });
    }

    async fn handle_target_info(&self, params: Value) {
        let Some(info) = params.get("targetInfo") else {
            return;
        };
        let (Some(target_id), Some(url)) = (
            info.get("targetId").and_then(Value::as_str),
            info.get("url").and_then(Value::as_str),
        ) else {
            return;
        };
        let title = info
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.state
            .write()
            .await
            .update_target_info(target_id, bounded(title), url.to_string());
    }

    async fn handle_dialog_open(
        &self,
        params: Value,
        session_id: Option<&str>,
        target_id: Option<String>,
    ) {
        let (Some(session), Some(target)) = (session_id, target_id) else {
            return;
        };
        let kind = match params.get("type").and_then(Value::as_str) {
            Some("alert") => BrowserDialogKind::Alert,
            Some("confirm") => BrowserDialogKind::Confirm,
            Some("prompt") => BrowserDialogKind::Prompt,
            Some("beforeunload") => BrowserDialogKind::BeforeUnload,
            _ => return,
        };
        self.state.write().await.open_dialog(
            &target,
            session.to_string(),
            kind,
            bounded(
                params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            bounded(
                params
                    .get("defaultPrompt")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
        );
    }

    async fn handle_dialog_closed(&self, target_id: Option<String>) {
        if let Some(target_id) = target_id {
            self.state.write().await.close_dialog_for_target(&target_id);
        }
    }

    async fn handle_file_chooser(
        &self,
        params: Value,
        session_id: Option<&str>,
        target_id: Option<String>,
    ) {
        let (Some(session), Some(target)) = (session_id, target_id) else {
            return;
        };
        let mode = if params.get("mode").and_then(Value::as_str) == Some("selectMultiple") {
            BrowserFileChooserMode::SelectMultiple
        } else {
            BrowserFileChooserMode::SelectSingle
        };
        self.state
            .write()
            .await
            .open_file_chooser(&target, session.to_string(), mode);
    }

    async fn handle_lifecycle(&self, params: Value, target_id: Option<String>) {
        if params.get("name").and_then(Value::as_str) == Some("init") {
            if let Some(target_id) = target_id {
                self.state.write().await.record_document_init(&target_id);
            }
        }
    }

    async fn handle_download_begin(&self, params: Value, target_id: Option<String>) {
        let (Some(id), Some(filename)) = (
            params.get("guid").and_then(Value::as_str),
            params.get("suggestedFilename").and_then(Value::as_str),
        ) else {
            return;
        };
        self.state.write().await.begin_download(
            id.to_string(),
            target_id.as_deref(),
            bounded(filename),
        );
    }

    async fn handle_download_progress(&self, params: Value) {
        let Some(id) = params.get("guid").and_then(Value::as_str) else {
            return;
        };
        let status = match params.get("state").and_then(Value::as_str) {
            Some("completed") => BrowserDownloadStatus::Completed,
            Some("canceled") => BrowserDownloadStatus::Cancelled,
            Some("inProgress") => BrowserDownloadStatus::InProgress,
            _ => BrowserDownloadStatus::Failed,
        };
        self.state.write().await.update_download(
            id,
            status,
            number_u64(&params, "receivedBytes"),
            Some(number_u64(&params, "totalBytes")).filter(|value| *value > 0),
            params
                .get("filePath")
                .and_then(Value::as_str)
                .map(str::to_string),
        );
    }
}

fn bounded(value: &str) -> String {
    value.chars().take(4096).collect()
}

fn number_u64(params: &Value, key: &str) -> u64 {
    params
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .max(0.0) as u64
}
