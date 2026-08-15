use tauri::Manager;

use crate::browser::{BrowserError, BrowserSessionManager};

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
