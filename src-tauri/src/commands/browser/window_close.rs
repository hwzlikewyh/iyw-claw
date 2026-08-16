use tauri::Manager;

use crate::browser::{BrowserError, BrowserSessionManager};

const WINDOW_DESTROY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn close_all_browser_windows(
    app: &tauri::AppHandle,
    manager: &BrowserSessionManager,
) -> Result<(), BrowserError> {
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with("browser-"))
        .cloned()
        .collect();
    let window_count = labels.len();
    let mut first_error = None;
    let mut closed_count = 0usize;

    tracing::info!(
        target: "iyw_claw_browser",
        window_count,
        "detached browser window shutdown started"
    );

    for label in labels {
        match close_window_for_shutdown(app, manager, &label).await {
            Ok(()) => {
                closed_count = closed_count.saturating_add(1);
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    if let Some(error) = first_error {
        tracing::error!(
            target: "iyw_claw_browser",
            window_count,
            closed_count,
            error_code = ?error.code,
            "detached browser window shutdown incomplete"
        );
        return Err(error.effect_may_have_occurred(closed_count > 0));
    }
    tracing::info!(
        target: "iyw_claw_browser",
        window_count,
        closed_count,
        "detached browser window shutdown completed"
    );
    Ok(())
}

async fn close_window_for_shutdown(
    app: &tauri::AppHandle,
    manager: &BrowserSessionManager,
    label: &str,
) -> Result<(), BrowserError> {
    close_browser_window(app, label, "runtime_stop")?;
    if !wait_for_window_destroyed(app, label).await {
        return Err(window_error());
    }
    manager.unregister_browser_window(label).await;
    Ok(())
}

async fn wait_for_window_destroyed(app: &tauri::AppHandle, label: &str) -> bool {
    let deadline = std::time::Instant::now() + WINDOW_DESTROY_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if app.get_webview_window(label).is_none() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    app.get_webview_window(label).is_none()
}

pub(super) fn close_browser_window(
    app: &tauri::AppHandle,
    window_label: &str,
    close_source: &'static str,
) -> Result<(), BrowserError> {
    let Some(window) = app.get_webview_window(window_label) else {
        tracing::info!(
            target: "iyw_claw_browser",
            window_label,
            close_source,
            "detached browser window already absent"
        );
        spawn_browser_window_cleanup(app.clone(), window_label.to_string(), close_source);
        return Ok(());
    };

    tracing::info!(
        target: "iyw_claw_browser",
        window_label,
        close_source,
        "detached browser window close started"
    );
    if let Err(error) = window.hide() {
        tracing::warn!(
            target: "iyw_claw_browser",
            window_label,
            close_source,
            error = %error,
            "detached browser window could not be hidden before destroy"
        );
    }
    if let Err(error) = window.destroy() {
        tracing::error!(
            target: "iyw_claw_browser",
            window_label,
            close_source,
            error = %error,
            "detached browser window destroy failed"
        );
        if let Err(show_error) = window.show() {
            tracing::error!(
                target: "iyw_claw_browser",
                window_label,
                close_source,
                error = %show_error,
                "detached browser window could not be restored after destroy failure"
            );
        }
        return Err(window_error());
    }

    tracing::info!(
        target: "iyw_claw_browser",
        window_label,
        close_source,
        "detached browser window destroy requested"
    );
    Ok(())
}

pub(super) fn spawn_browser_window_cleanup(
    app: tauri::AppHandle,
    window_label: String,
    close_source: &'static str,
) {
    tauri::async_runtime::spawn(async move {
        if let Some(manager) = app.try_state::<BrowserSessionManager>() {
            let state = manager.unregister_browser_window(&window_label).await;
            tracing::info!(
                target: "iyw_claw_browser",
                window_label = %window_label,
                close_source,
                remaining_hosts = state.hosts.len(),
                remaining_tabs = state.tabs.len(),
                remaining_claims = state.view_claims.len(),
                "detached browser window resources released"
            );
        } else {
            tracing::warn!(
                target: "iyw_claw_browser",
                window_label = %window_label,
                close_source,
                "detached browser window cleanup skipped without session manager"
            );
        }
    });
}

fn window_error() -> BrowserError {
    BrowserError::new(
        crate::browser::BrowserErrorCode::BrowserViewConflict,
        "The browser window is unavailable",
    )
}
