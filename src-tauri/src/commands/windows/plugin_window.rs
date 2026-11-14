use super::{conversation_window::appearance::build_window, route_with_new_remote_window};
use crate::app_error::AppCommandError;
use serde::Deserialize;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tauri::{Manager, WebviewWindow};
use tokio::sync::oneshot;

const OPEN_TIMEOUT: Duration = Duration::from_secs(780);
type Ready = oneshot::Sender<Result<(), String>>;
#[derive(Default)]
pub struct PluginWindowState(Mutex<HashMap<String, Ready>>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPanelWindowRequest {
    pub conversation_id: i64,
    pub plugin_slug: String,
    pub app_key: String,
    pub expected_version: String,
    pub label: String,
    pub relay_id: String,
}

#[tauri::command]
pub async fn open_plugin_panel_window(
    window: WebviewWindow,
    request: PluginPanelWindowRequest,
    remote_connection_id: Option<i32>,
) -> Result<String, AppCommandError> {
    let route = route(&request)?;
    let app = window.app_handle();
    let label = format!("plugin-{}", uuid::Uuid::new_v4());
    let (route, remote_id) = route_with_new_remote_window(route, remote_connection_id);
    let (sender, receiver) = oneshot::channel();
    app.state::<PluginWindowState>()
        .0
        .lock()
        .unwrap()
        .insert(label.clone(), sender);
    let result = async {
        build_window(app, &label, (route, remote_id.as_deref()))?;
        let target = app
            .get_webview_window(&label)
            .ok_or_else(|| invalid("插件窗口已关闭"))?;
        watch_close(&target);
        tokio::time::timeout(OPEN_TIMEOUT, receiver)
            .await
            .map_err(|_| invalid("插件窗口打开超时，原页面已保留"))?
            .map_err(|_| invalid("插件窗口已关闭，原页面已保留"))?
            .map_err(|message| invalid(&message))?;
        target.show().map_err(|error| invalid(&error.to_string()))?;
        if let Err(error) = target.set_focus() {
            tracing::warn!(%error, "[plugin-window] focus failed");
        }
        Ok::<(), AppCommandError>(())
    }
    .await;
    app.state::<PluginWindowState>()
        .0
        .lock()
        .unwrap()
        .remove(&label);
    if let Err(error) = result {
        if let Some(target) = app.get_webview_window(&label) {
            let _ = target.destroy();
        }
        tracing::warn!(plugin = %request.plugin_slug, %error, "[plugin-window] open failed");
        return Err(error);
    }
    tracing::info!(plugin = %request.plugin_slug, conversation_id = request.conversation_id, "[plugin-window] ready");
    Ok(label)
}

fn route(request: &PluginPanelWindowRequest) -> Result<String, AppCommandError> {
    if request.conversation_id <= 0
        || request.plugin_slug.trim().is_empty()
        || request.app_key.trim().is_empty()
        || request.expected_version.trim().is_empty()
        || uuid::Uuid::parse_str(&request.relay_id).is_err()
    {
        return Err(invalid("插件窗口参数无效"));
    }
    Ok(format!(
        "plugin-window?conversationId={}&pluginSlug={}&appKey={}&version={}&label={}&relayId={}",
        request.conversation_id,
        urlencoding::encode(&request.plugin_slug),
        urlencoding::encode(&request.app_key),
        urlencoding::encode(&request.expected_version),
        urlencoding::encode(&request.label),
        urlencoding::encode(&request.relay_id)
    ))
}

fn watch_close(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            app.state::<PluginWindowState>()
                .0
                .lock()
                .unwrap()
                .remove(&label);
        }
    });
}

#[tauri::command]
pub fn plugin_panel_window_ready(window: WebviewWindow, error: Option<String>) {
    let app = window.app_handle();
    if let Some(sender) = app
        .state::<PluginWindowState>()
        .0
        .lock()
        .unwrap()
        .remove(window.label())
    {
        let _ = sender.send(error.map_or(Ok(()), Err));
    }
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
