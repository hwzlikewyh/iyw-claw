use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::service::ChannelToolService;
use super::types::{ChannelCaller, SendItemInput};
use crate::db::service::{
    chat_channel_target_service,
    chat_channel_tool_audit_service::{self, AuditRecord},
};

#[derive(Serialize)]
struct AuditFile {
    name: String,
    bytes: Option<u64>,
}

impl ChannelToolService {
    pub(super) async fn audit_send_item(
        &self,
        caller: &ChannelCaller,
        request_id: &str,
        item: &SendItemInput,
        working_dir: &Path,
        result: &Value,
    ) -> Result<(), String> {
        let target_label = self.target_label(item).await;
        let file_summary_json = safe_file_summary(&item.files, working_dir);
        let error_code = result
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| result.get("message_error").and_then(Value::as_str))
            .or_else(|| result.get("file_error").and_then(Value::as_str))
            .or_else(|| result.get("log_error").and_then(Value::as_str))
            .or_else(|| result.get("target_error").and_then(Value::as_str));
        let record = AuditRecord {
            agent_type: &caller.agent_type,
            session_ref: &caller.session_ref,
            operation: "send_channel_messages",
            channel_id: Some(item.channel_id),
            target_id: item.target_id.as_deref(),
            target_label: target_label.as_deref(),
            file_summary_json,
            status: result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("failed"),
            error_code,
            request_id,
        };
        chat_channel_tool_audit_service::create(&self.db.conn, record)
            .await
            .map_err(|_| "AUDIT_UNAVAILABLE".to_string())
    }

    async fn target_label(&self, item: &SendItemInput) -> Option<String> {
        let target_id = item.target_id.as_deref()?;
        chat_channel_target_service::find_by_target_id(&self.db.conn, item.channel_id, target_id)
            .await
            .ok()
            .flatten()
            .map(|target| target.display_name)
    }
}

fn safe_file_summary(paths: &[String], working_dir: &Path) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let files = paths
        .iter()
        .map(|value| audit_file(value, working_dir))
        .collect::<Vec<_>>();
    serde_json::to_string(&files).ok()
}

fn audit_file(value: &str, working_dir: &Path) -> AuditFile {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        working_dir.join(path)
    };
    AuditFile {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_string(),
        bytes: std::fs::metadata(path).ok().map(|metadata| metadata.len()),
    }
}
