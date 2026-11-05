use crate::{app_error::AppCommandError, native_share};
use iyw_share_core::{Inbox, PreparedShare, ShareBatch, ShareSummary};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

static INBOX_WORK: Mutex<()> = Mutex::new(());

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePreview {
    #[serde(flatten)]
    prepared: PreparedShare,
    paths: Vec<String>,
}

async fn with_inbox<T, F>(app: AppHandle, operation: F) -> Result<T, AppCommandError>
where
    T: Send + 'static,
    F: FnOnce(Inbox) -> iyw_share_core::Result<T> + Send + 'static,
{
    let inbox = native_share::inbox(&app).map_err(command_error)?;
    tokio::task::spawn_blocking(move || {
        let _guard = INBOX_WORK
            .lock()
            .map_err(|_| iyw_share_core::invalid("分享处理状态不可用，请重启应用"))?;
        operation(inbox)
    })
    .await
    .map_err(|_| AppCommandError::task_execution_failed("分享处理任务未能完成"))?
    .map_err(command_error)
}

fn command_error(error: iyw_share_core::ShareError) -> AppCommandError {
    tracing::warn!(target: "iyw_native_share", error = %error, "native share operation failed");
    AppCommandError::task_execution_failed(error.to_string())
}

#[tauri::command]
pub async fn native_share_list(app: AppHandle) -> Result<Vec<ShareSummary>, AppCommandError> {
    with_inbox(app, |inbox| inbox.list()).await
}

#[tauri::command]
pub async fn native_share_prepare(
    app: AppHandle,
    id: String,
) -> Result<SharePreview, AppCommandError> {
    uuid::Uuid::parse_str(&id).map_err(|_| AppCommandError::invalid_input("无效的分享批次编号"))?;
    tracing::info!(target: "iyw_native_share", batch_id = %id, "preparing shared chat archive");
    with_inbox(app, move |inbox| {
        let prepared = inbox.prepare(&id)?;
        let paths = inbox.prepared_paths(&id)?;
        Ok(SharePreview { prepared, paths })
    })
    .await
}

#[tauri::command]
pub async fn native_share_complete(app: AppHandle, id: String) -> Result<(), AppCommandError> {
    uuid::Uuid::parse_str(&id).map_err(|_| AppCommandError::invalid_input("无效的分享批次编号"))?;
    tracing::info!(target: "iyw_native_share", batch_id = %id, "shared archive accepted into composer draft");
    with_inbox(app, move |inbox| inbox.complete(&id)).await
}

#[tauri::command]
pub async fn native_share_import(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<ShareBatch, AppCommandError> {
    with_inbox(app, move |inbox| {
        if paths.is_empty() || paths.len() > iyw_share_core::MAX_FILES {
            return Err(iyw_share_core::invalid("请选择 1 到 32 个聊天归档文件"));
        }
        let mut writer = inbox.begin()?;
        for source in paths {
            let path = std::path::Path::new(&source);
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !matches!(extension.to_ascii_lowercase().as_str(), "zip" | "txt") {
                return Err(iyw_share_core::invalid(
                    "手动导入仅支持 ZIP 或 TXT 聊天归档",
                ));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| iyw_share_core::invalid("分享文件名无效"))?;
            writer.copy_file(path, name)?;
        }
        let batch = writer.commit()?;
        tracing::info!(target: "iyw_native_share", batch_id = %batch.id,
            files = batch.items.len(), "manual chat archive import committed");
        Ok(batch)
    })
    .await
}

#[tauri::command]
pub async fn native_share_status(
    app: AppHandle,
) -> Result<native_share::ShareStatus, AppCommandError> {
    tokio::task::spawn_blocking(move || native_share::status(&app))
        .await
        .map_err(|_| AppCommandError::task_execution_failed("无法检查系统分享入口"))?
        .map_err(command_error)
}

#[tauri::command]
pub async fn native_share_register(app: AppHandle) -> Result<(), AppCommandError> {
    native_share::register(&app).await.map_err(command_error)
}

#[tauri::command]
pub fn native_share_open(app: AppHandle) -> Result<(), AppCommandError> {
    crate::commands::windows::show_main_window(&app);
    app.emit_to("main", "native-share://open", ())
        .map_err(|error| AppCommandError::window("无法打开聊天导入窗口", error.to_string()))
}
