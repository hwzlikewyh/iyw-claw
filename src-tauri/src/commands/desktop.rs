use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::app_error::AppCommandError;
use crate::commands::windows;
use crate::preferences::{self, CloseBehavior};

pub const MAIN_CLOSE_REQUESTED_EVENT: &str = "app://main-close-requested";
const AUTOSTART_ARG: &str = "--from-autostart";

static CLOSE_PROMPT_OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseRequestPayload {
    pub can_hide_to_tray: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseAction {
    Tray,
    Exit,
}

pub enum CloseRequestOutcome {
    Prompt(CloseRequestPayload),
    Ignored,
    Completed,
}

pub fn launched_from_autostart() -> bool {
    args_request_autostart(std::env::args())
}

pub fn should_hide_on_autostart() -> bool {
    launched_from_autostart() && windows::can_hide_to_tray()
}

pub fn args_request_autostart(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|arg| arg == AUTOSTART_ARG)
}

pub fn handle_main_close_request(app: &AppHandle) -> CloseRequestOutcome {
    let can_hide_to_tray = windows::can_hide_to_tray();

    match preferences::load().close_behavior {
        Some(CloseBehavior::Tray) if can_hide_to_tray => {
            tracing::info!(
                close_behavior = "tray",
                remembered = true,
                "[window] main close requested"
            );
            if let Err(error) = hide_main_window(app) {
                tracing::error!(error = %error.message, "[window] remembered tray close failed");
                crate::desktop_shutdown::request_exit(app);
                return CloseRequestOutcome::Completed;
            }
            CloseRequestOutcome::Completed
        }
        Some(CloseBehavior::Exit) | Some(CloseBehavior::Tray) => {
            tracing::info!(
                close_behavior = "exit",
                remembered = true,
                "[window] main close requested"
            );
            crate::desktop_shutdown::request_exit(app);
            CloseRequestOutcome::Completed
        }
        None if !can_hide_to_tray => {
            tracing::info!(
                close_behavior = "exit",
                remembered = false,
                tray_available = false,
                "[window] main close requested"
            );
            crate::desktop_shutdown::request_exit(app);
            CloseRequestOutcome::Completed
        }
        None => {
            if CLOSE_PROMPT_OPEN.swap(true, Ordering::AcqRel) {
                CloseRequestOutcome::Ignored
            } else {
                tracing::info!(
                    tray_available = true,
                    "[window] requesting close behavior choice"
                );
                CloseRequestOutcome::Prompt(CloseRequestPayload { can_hide_to_tray })
            }
        }
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn get_pending_main_close_request() -> Option<CloseRequestPayload> {
    CLOSE_PROMPT_OPEN
        .load(Ordering::Acquire)
        .then(|| CloseRequestPayload {
            can_hide_to_tray: windows::can_hide_to_tray(),
        })
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn complete_main_close(
    app: AppHandle,
    action: CloseAction,
    remember: bool,
) -> Result<(), AppCommandError> {
    if matches!(action, CloseAction::Tray) && !windows::can_hide_to_tray() {
        return Err(AppCommandError::window(
            "Cannot hide the workspace to the system tray",
            "The system tray is unavailable on this platform",
        ));
    }

    if remember {
        preferences::update(|prefs| {
            prefs.close_behavior = Some(action.into());
        })
        .map_err(|error| {
            AppCommandError::io_error("Failed to persist close behavior")
                .with_detail(error.to_string())
        })?;
    }

    tracing::info!(?action, remember, "[window] applying close behavior choice");
    CLOSE_PROMPT_OPEN.store(false, Ordering::Release);
    match action {
        CloseAction::Tray => hide_main_window(&app),
        CloseAction::Exit => {
            crate::desktop_shutdown::request_exit(&app);
            Ok(())
        }
    }
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub fn cancel_main_close() {
    tracing::info!("[window] close behavior choice cancelled");
    CLOSE_PROMPT_OPEN.store(false, Ordering::Release);
}

fn hide_main_window(app: &AppHandle) -> Result<(), AppCommandError> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppCommandError::window("Main window is unavailable", String::new()))?;
    window.hide().map_err(|error| {
        AppCommandError::window("Failed to hide main window", error.to_string())
    })?;
    crate::webview_memory::note_hidden(app);
    Ok(())
}

impl From<CloseAction> for CloseBehavior {
    fn from(action: CloseAction) -> Self {
        match action {
            CloseAction::Tray => Self::Tray,
            CloseAction::Exit => Self::Exit,
        }
    }
}
