use crate::browser::{BrowserSessionManager, BrowserVisibilityUpdate};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_set_visibility(
    app: tauri::AppHandle,
    manager: tauri::State<'_, BrowserSessionManager>,
    request: BrowserVisibilityUpdate,
) -> BrowserCommandFuture<bool> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.update_browser_visibility(&app, request).await })
}
