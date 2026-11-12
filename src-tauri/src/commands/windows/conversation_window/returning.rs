use super::{ConversationWindowState, Transfer};
use crate::app_error::AppCommandError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Manager, WebviewWindow};

#[derive(Clone, Deserialize, Serialize)]
pub struct DropBounds {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
pub async fn set_conversation_drop_bounds(window: WebviewWindow, bounds: Vec<DropBounds>) {
    if window.label().starts_with("conversation-") {
        return;
    }
    window
        .app_handle()
        .state::<ConversationWindowState>()
        .2
        .lock()
        .await
        .insert(window.label().to_owned(), bounds);
}

#[tauri::command]
pub async fn return_conversation_window(
    window: WebviewWindow,
    payload: Value,
    drop_only: bool,
) -> Result<Option<String>, AppCommandError> {
    let app = window.app_handle();
    let state = app.state::<ConversationWindowState>();
    let owner = state
        .1
        .lock()
        .await
        .get(window.label())
        .cloned()
        .ok_or_else(|| {
            AppCommandError::invalid_input("Original conversation window is unavailable")
        })?;
    let target = app
        .get_webview_window(&owner)
        .ok_or_else(|| AppCommandError::invalid_input("Original workspace was closed"))?;
    if drop_only && !is_drop_target(&target).await {
        return Ok(None);
    }
    let id = format!("return-{}", uuid::Uuid::new_v4());
    let mut transfers = state.0.lock().await;
    if transfers.values().any(|item| item.target == owner) {
        return Err(AppCommandError::invalid_input(
            "The workspace is already receiving a conversation",
        ));
    }
    let remote = target.url().ok().is_some_and(|url| {
        url.query_pairs()
            .any(|(key, _)| key == "remoteConnectionId")
    });
    let transfer = Transfer::new((window.label(), &owner), payload, (remote, false));
    transfers.insert(id.clone(), transfer);
    tracing::info!(
        source = window.label(),
        target = owner,
        transfer_id = id,
        "[conversation-window] returning"
    );
    Ok(Some(id))
}

async fn is_drop_target(window: &WebviewWindow) -> bool {
    if window.is_minimized().unwrap_or(true) {
        return false;
    }
    let Ok(cursor) = window.cursor_position() else {
        return false;
    };
    let Ok(origin) = window.inner_position() else {
        return false;
    };
    let Ok(scale) = window.scale_factor() else {
        return false;
    };
    let x = (cursor.x - origin.x as f64) / scale;
    let y = (cursor.y - origin.y as f64) / scale;
    let state = window.app_handle().state::<ConversationWindowState>();
    let bounds = state.2.lock().await;
    bounds.get(window.label()).is_some_and(|areas| {
        areas.iter().any(|area| {
            x >= area.left
                && x <= area.left + area.width
                && y >= area.top
                && y <= area.top + area.height
        })
    })
}

#[tauri::command]
pub async fn pending_conversation_window(window: WebviewWindow) -> Option<String> {
    let state = window.app_handle().state::<ConversationWindowState>();
    let transfers = state.0.lock().await;
    transfers.iter().find_map(|(id, transfer)| {
        (transfer.target == window.label() && !transfer.new_window && transfer.phase == "preparing")
            .then(|| id.clone())
    })
}
