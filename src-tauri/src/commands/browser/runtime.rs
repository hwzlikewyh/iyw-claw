use crate::browser::{BrowserSessionManager, BrowserStateSnapshot};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_get_state(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { Ok(manager.snapshot().await) })
}

#[tauri::command(async)]
pub fn browser_refresh_capability(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { Ok(manager.refresh_capability().await) })
}

#[tauri::command(async)]
pub fn browser_start_runtime(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.start_browser_runtime().await })
}

#[tauri::command(async)]
pub fn browser_stop_runtime(
    manager: tauri::State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.stop_browser_runtime().await })
}
