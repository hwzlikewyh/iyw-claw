use crate::browser::{BrowserError, BrowserSessionManager, BrowserStateSnapshot};

#[tauri::command]
pub async fn browser_get_state(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    Ok(manager.snapshot().await)
}

#[tauri::command]
pub async fn browser_refresh_capability(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    Ok(manager.refresh_capability().await)
}

#[tauri::command]
pub async fn browser_start_runtime(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.start_browser_runtime().await
}

#[tauri::command]
pub async fn browser_stop_runtime(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.stop_browser_runtime().await
}
