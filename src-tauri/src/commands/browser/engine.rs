use serde_json::Value;
use tauri::{AppHandle, State, WebviewWindow};

use crate::browser::{BrowserEngineSettings, BrowserSessionManager};

use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_get_engine_settings(
    manager: State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserEngineSettings> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager.refresh_capability().await;
        Ok(manager.browser_engine_settings().await)
    })
}

#[tauri::command(async)]
pub fn browser_set_full_engine(
    app: AppHandle,
    manager: State<'_, BrowserSessionManager>,
    enabled: bool,
) -> BrowserCommandFuture<BrowserEngineSettings> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.set_full_browser_enabled(&app, enabled).await })
}

#[tauri::command(async)]
pub fn browser_install_full_engine(
    app: AppHandle,
    manager: State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<BrowserEngineSettings> {
    let manager = manager.inner().clone();
    browser_command(async move {
        let _writer = crate::managed_environment::lock_writer().await;
        let emitter = crate::web::event_bridge::EventEmitter::Tauri(app.clone());
        crate::managed_environment::repair_with_progress(
            crate::managed_environment::RepairRequest {
                task_id: "browser-full-engine",
                component: Some("chromix"),
            },
            &emitter,
            |_| {},
        )
        .await
        .map_err(|error| {
            crate::browser::BrowserError::new(
                crate::browser::BrowserErrorCode::BrowserRuntimeUnavailable,
                error,
            )
        })?;
        let settings = manager.browser_component_install_finished(&app).await?;
        if !settings.full_browser_installed {
            return Err(crate::browser::BrowserError::new(
                crate::browser::BrowserErrorCode::BrowserRuntimeUnavailable,
                "完整浏览器组件未能安装或校验，请检查网络后重试修复",
            ));
        }
        Ok(settings)
    })
}

#[tauri::command(async)]
pub fn browser_native_view(
    window: WebviewWindow,
    manager: State<'_, BrowserSessionManager>,
    mut request: Value,
) -> BrowserCommandFuture<Value> {
    let manager = manager.inner().clone();
    request["owner"] = Value::String(window.label().to_string());
    browser_command(async move { manager.native_browser_view(request).await })
}

#[tauri::command(async)]
pub fn browser_native_action(
    manager: State<'_, BrowserSessionManager>,
    tab_id: String,
    action: String,
    values: Value,
) -> BrowserCommandFuture<Value> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .native_browser_action(&tab_id, &action, values)
            .await
    })
}
