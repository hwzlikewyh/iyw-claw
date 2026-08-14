use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::Manager;

use crate::browser::{
    BrowserError, BrowserFrameSubscriptionSnapshot, BrowserGenerations, BrowserHostKind,
    BrowserHostRegistration, BrowserSessionManager, BrowserStateSnapshot, BrowserViewClaimSnapshot,
};

#[tauri::command]
pub fn browser_create_window(app: tauri::AppHandle) -> Result<String, BrowserError> {
    let detached_count = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with("browser-"))
        .count();
    if detached_count >= crate::browser::MAX_DETACHED_BROWSER_WINDOWS {
        return Err(window_error());
    }
    let label = format!("browser-{}", uuid::Uuid::new_v4());
    tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App("browser?detached=1".into()),
    )
    .title("原助理浏览器")
    .inner_size(1180.0, 760.0)
    .min_inner_size(720.0, 520.0)
    .build()
    .map_err(|_| {
        BrowserError::new(
            crate::browser::BrowserErrorCode::BrowserViewConflict,
            "The browser window could not be created",
        )
    })?;
    Ok(label)
}

#[tauri::command]
pub fn browser_close_window(
    app: tauri::AppHandle,
    window_label: String,
) -> Result<(), BrowserError> {
    validate_browser_window_label(&window_label)?;
    if let Some(window) = app.get_webview_window(&window_label) {
        window.close().map_err(|_| window_error())?;
    }
    Ok(())
}

pub fn handle_browser_window_close(app: tauri::AppHandle, window_label: String) {
    tauri::async_runtime::spawn(async move {
        if let Some(manager) = app.try_state::<BrowserSessionManager>() {
            manager.unregister_browser_window(&window_label).await;
        }
        if let Some(window) = app.get_webview_window(&window_label) {
            if let Err(error) = window.destroy() {
                tracing::warn!(
                    target: "iyw_claw_browser",
                    window_label,
                    error = %error,
                    "browser window could not be destroyed after host release"
                );
            }
        }
    });
}

fn validate_browser_window_label(label: &str) -> Result<(), BrowserError> {
    label
        .strip_prefix("browser-")
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .map(|_| ())
        .ok_or_else(window_error)
}

fn window_error() -> BrowserError {
    BrowserError::new(
        crate::browser::BrowserErrorCode::BrowserViewConflict,
        "The browser window is unavailable",
    )
}

#[tauri::command]
pub async fn browser_register_host(
    manager: tauri::State<'_, BrowserSessionManager>,
    window_label: String,
    kind: BrowserHostKind,
) -> Result<BrowserHostRegistration, BrowserError> {
    manager.register_browser_host(window_label, kind).await
}

#[tauri::command]
pub async fn browser_heartbeat_host(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
    generation: u64,
    visible: bool,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .heartbeat_browser_host(&host_id, generation, visible)
        .await
}

#[tauri::command]
pub async fn browser_unregister_host(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager.unregister_browser_host(&host_id).await
}

#[tauri::command]
pub async fn browser_set_host_visible(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
    generation: u64,
    visible: bool,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .set_browser_host_visible(&host_id, generation, visible)
        .await
}

#[tauri::command]
pub async fn browser_activate_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
    host_generation: u64,
    tab_id: String,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .activate_browser_tab(&host_id, host_generation, &tab_id)
        .await
}

#[tauri::command]
pub async fn browser_begin_view_claim(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    source_host_id: Option<String>,
    target_host_id: String,
    target_index: usize,
) -> Result<BrowserViewClaimSnapshot, BrowserError> {
    manager
        .begin_browser_view_claim(&tab_id, source_host_id, target_host_id, target_index)
        .await
}

#[tauri::command]
pub async fn browser_subscribe_claim_frames(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    generations: BrowserGenerations,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<BrowserFrameSubscriptionSnapshot, BrowserError> {
    manager
        .subscribe_browser_claim_frames(&claim_id, generations, on_frame)
        .await
}

#[tauri::command]
pub async fn browser_ack_claim_frame(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    subscription_id: String,
    generations: BrowserGenerations,
    seq: u64,
) -> Result<BrowserViewClaimSnapshot, BrowserError> {
    manager
        .acknowledge_browser_claim_frame(&claim_id, &subscription_id, generations, seq)
        .await
}

#[tauri::command]
pub async fn browser_commit_view_claim(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    subscription_id: String,
    generations: BrowserGenerations,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .commit_browser_view_claim(&claim_id, &subscription_id, generations)
        .await
}

#[tauri::command]
pub async fn browser_abort_view_claim(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    generations: BrowserGenerations,
) -> Result<BrowserStateSnapshot, BrowserError> {
    manager
        .abort_browser_view_claim(&claim_id, generations)
        .await
}
