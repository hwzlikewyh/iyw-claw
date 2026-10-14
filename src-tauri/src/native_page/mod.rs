//! 交互页面的原生子视图；不启动受管浏览器，也不向网页提供应用 IPC。
mod actions;
mod build;
pub(crate) mod capture;
mod downloads;
pub(crate) mod find;
pub(crate) mod platform;
mod state;
#[cfg(target_os = "windows")]
pub(crate) mod window_hook;

use serde_json::Value;
use tauri::WebviewWindow;
use tokio::sync::oneshot;

const COMMAND_TIMEOUT_SECS: u64 = 30;

#[tauri::command(async)]
pub async fn native_page_command(window: WebviewWindow, request: Value) -> Result<Value, String> {
    let (sender, receiver) = oneshot::channel();
    let owner = window.clone();
    window
        .run_on_main_thread(move || actions::dispatch(owner, request, sender))
        .map_err(|error| error.to_string())?;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(COMMAND_TIMEOUT_SECS),
        receiver,
    )
    .await
    .map_err(|_| "Native page operation timed out".to_string())?
    .map_err(|_| "Native page operation was interrupted".to_string())?;
    if let Err(error) = &result {
        tracing::warn!(owner = window.label(), error = %error, "native page operation failed");
    }
    result
}

pub(crate) fn close_owner(owner: &str) {
    // Tauri WindowEvent 在主线程回调，直接清理原生子视图。
    state::close_owner(owner);
}
