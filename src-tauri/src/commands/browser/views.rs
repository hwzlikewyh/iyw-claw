use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::Manager;

use crate::browser::{
    BrowserError, BrowserFrameSubscriptionSnapshot, BrowserGenerations, BrowserHostKind,
    BrowserHostRegistration, BrowserSessionManager, BrowserStateSnapshot, BrowserViewClaimSnapshot,
};

use super::window_close::{close_browser_window, spawn_browser_window_cleanup};
use super::{browser_command, BrowserCommandFuture};

#[tauri::command(async)]
pub fn browser_create_window(
    app: tauri::AppHandle,
    manager: tauri::State<'_, BrowserSessionManager>,
) -> BrowserCommandFuture<String> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .run_browser_window_creation(move || create_browser_window(app))
            .await
    })
}

fn create_browser_window(app: tauri::AppHandle) -> Result<String, BrowserError> {
    let detached_count = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with("browser-"))
        .count();
    if detached_count >= crate::browser::MAX_DETACHED_BROWSER_WINDOWS {
        return Err(window_error());
    }
    let label = format!("browser-{}", uuid::Uuid::new_v4());
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App("browser?detached=1".into()),
    )
    .title("原助理浏览器")
    .inner_size(1180.0, 760.0)
    .min_inner_size(480.0, 360.0)
    .closable(true)
    .build()
    .map_err(|error| {
        tracing::error!(
            target: "iyw_claw_browser",
            window_label = %label,
            error = %error,
            "detached browser window creation failed"
        );
        window_error()
    })?;
    if let Err(error) = window.show() {
        tracing::warn!(
            target: "iyw_claw_browser",
            window_label = %label,
            error = %error,
            "detached browser window could not be shown"
        );
    }
    if let Err(error) = window.set_focus() {
        tracing::warn!(
            target: "iyw_claw_browser",
            window_label = %label,
            error = %error,
            "detached browser window could not be focused"
        );
    }
    tracing::info!(
        target: "iyw_claw_browser",
        window_label = %label,
        "detached browser window created"
    );
    Ok(label)
}

#[tauri::command(async)]
pub fn browser_close_window(
    app: tauri::AppHandle,
    window_label: String,
) -> BrowserCommandFuture<()> {
    browser_command(async move {
        validate_browser_window_label(&window_label)?;
        close_browser_window(&app, &window_label, "command")
    })
}

#[tauri::command(async)]
pub fn browser_close_window_preserving_tabs(
    app: tauri::AppHandle,
    manager: tauri::State<'_, BrowserSessionManager>,
    window_label: String,
) -> BrowserCommandFuture<()> {
    let manager = manager.inner().clone();
    browser_command(async move {
        validate_browser_window_label(&window_label)?;
        let host_id = manager.preserve_browser_window_tabs(&window_label).await?;
        if let Err(error) = close_browser_window(&app, &window_label, "agent_display_close") {
            manager.cancel_preserved_browser_window(&host_id).await;
            return Err(error);
        }
        Ok(())
    })
}

#[tauri::command(async)]
pub fn browser_focus_window(
    app: tauri::AppHandle,
    window_label: String,
) -> BrowserCommandFuture<()> {
    browser_command(async move {
        validate_browser_window_label(&window_label)?;
        let window = app
            .get_webview_window(&window_label)
            .ok_or_else(window_error)?;
        window.show().map_err(|_| window_error())?;
        window.set_focus().map_err(|_| window_error())
    })
}

#[tauri::command(async)]
pub fn browser_complete_window_open(
    manager: tauri::State<'_, BrowserSessionManager>,
    request_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { Ok(manager.complete_window_open_request(&request_id).await) })
}

#[tauri::command(async)]
pub fn browser_complete_window_close(
    manager: tauri::State<'_, BrowserSessionManager>,
    request_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { Ok(manager.complete_window_close_request(&request_id).await) })
}

pub fn handle_browser_window_close_requested(app: tauri::AppHandle, window_label: String) {
    if let Some(manager) = app.try_state::<BrowserSessionManager>() {
        let manager = manager.inner().clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            let host_id = manager
                .preserve_browser_window_tabs(&window_label)
                .await
                .ok();
            if let Err(error) = close_browser_window(&app_clone, &window_label, "system_close") {
                if let Some(host_id) = host_id {
                    manager.cancel_preserved_browser_window(&host_id).await;
                }
                tracing::error!(
                    target: "iyw_claw_browser",
                    window_label = %window_label,
                    error_code = ?error.code,
                    "detached browser window close failed"
                );
            }
        });
        return;
    }
    if let Err(error) = close_browser_window(&app, &window_label, "system_close") {
        tracing::error!(
            target: "iyw_claw_browser",
            window_label = %window_label,
            error_code = ?error.code,
            "detached browser window close failed"
        );
    }
}

pub fn handle_browser_window_destroyed(app: tauri::AppHandle, window_label: String) {
    tracing::info!(
        target: "iyw_claw_browser",
        window_label = %window_label,
        "detached browser window destroyed"
    );
    spawn_browser_window_cleanup(app, window_label, "destroyed");
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

#[tauri::command(async)]
pub fn browser_register_host(
    app: tauri::AppHandle,
    manager: tauri::State<'_, BrowserSessionManager>,
    window_label: String,
    kind: BrowserHostKind,
) -> BrowserCommandFuture<BrowserHostRegistration> {
    let manager = manager.inner().clone();
    browser_command(async move {
        let label = window_label.clone();
        manager
            .register_browser_host(window_label, kind, move || {
                validate_browser_host_window(&app, &label, kind)
            })
            .await
    })
}

fn validate_browser_host_window(
    app: &tauri::AppHandle,
    window_label: &str,
    kind: BrowserHostKind,
) -> Result<(), BrowserError> {
    let expected_label = match kind {
        BrowserHostKind::Docked => window_label == "main",
        BrowserHostKind::Detached => validate_browser_window_label(window_label).is_ok(),
    };
    if expected_label && app.get_webview_window(window_label).is_some() {
        return Ok(());
    }
    Err(window_error())
}

#[tauri::command(async)]
pub fn browser_heartbeat_host(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
    generation: u64,
    visible: bool,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .heartbeat_browser_host(&host_id, generation, visible)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_unregister_host(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move { manager.unregister_browser_host(&host_id).await })
}

#[tauri::command(async)]
pub fn browser_set_host_visible(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
    generation: u64,
    visible: bool,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .set_browser_host_visible(&host_id, generation, visible)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_activate_tab(
    manager: tauri::State<'_, BrowserSessionManager>,
    host_id: String,
    host_generation: u64,
    tab_id: String,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .activate_browser_tab(&host_id, host_generation, &tab_id)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_begin_view_claim(
    manager: tauri::State<'_, BrowserSessionManager>,
    tab_id: String,
    source_host_id: Option<String>,
    target_host_id: String,
    target_index: usize,
) -> BrowserCommandFuture<BrowserViewClaimSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .begin_browser_view_claim(&tab_id, source_host_id, target_host_id, target_index)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_subscribe_claim_frames(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    generations: BrowserGenerations,
    on_frame: Channel<InvokeResponseBody>,
) -> BrowserCommandFuture<BrowserFrameSubscriptionSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .subscribe_browser_claim_frames(&claim_id, generations, on_frame)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_ack_claim_frame(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    subscription_id: String,
    generations: BrowserGenerations,
    seq: u64,
) -> BrowserCommandFuture<BrowserViewClaimSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .acknowledge_browser_claim_frame(&claim_id, &subscription_id, generations, seq)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_commit_view_claim(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    subscription_id: String,
    generations: BrowserGenerations,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .commit_browser_view_claim(&claim_id, &subscription_id, generations)
            .await
    })
}

#[tauri::command(async)]
pub fn browser_abort_view_claim(
    manager: tauri::State<'_, BrowserSessionManager>,
    claim_id: String,
    generations: BrowserGenerations,
) -> BrowserCommandFuture<BrowserStateSnapshot> {
    let manager = manager.inner().clone();
    browser_command(async move {
        manager
            .abort_browser_view_claim(&claim_id, generations)
            .await
    })
}
