use super::route_with_new_remote_window;
use crate::{acp::manager::ConnectionManager, app_error::AppCommandError};
use appearance::build_window;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::Mutex;

mod appearance;
mod lifecycle;
pub use lifecycle::window_destroyed;
pub mod returning;

#[derive(Default)]
pub struct ConversationWindowState(
    Mutex<HashMap<String, Transfer>>,
    Mutex<HashMap<String, String>>,
    Mutex<HashMap<String, Vec<returning::DropBounds>>>,
);

struct Transfer {
    source: String,
    target: String,
    new_window: bool,
    payload: Value,
    phase: &'static str,
    remote: bool,
    connection_id: Option<String>,
}

impl Transfer {
    fn new(windows: (&str, &str), payload: Value, flags: (bool, bool)) -> Self {
        let (source, target) = windows;
        let (remote, new_window) = flags;
        let connection_id = payload["connection"]["connectionId"]
            .as_str()
            .map(str::to_owned);
        Self {
            source: source.to_owned(),
            target: target.to_owned(),
            new_window,
            payload,
            phase: "preparing",
            remote,
            connection_id,
        }
    }
}

#[derive(Serialize)]
pub struct TransferState {
    phase: &'static str,
    payload: Option<Value>,
}

#[tauri::command]
pub async fn open_conversation_window(
    window: WebviewWindow,
    payload: Value,
    remote_connection_id: Option<i32>,
) -> Result<String, AppCommandError> {
    let app = window.app_handle().clone();
    if !payload["tab"]["id"].is_string() {
        return Err(AppCommandError::invalid_input("Missing conversation tab"));
    }
    let label = format!("conversation-{}", uuid::Uuid::new_v4());
    let route = format!("workspace?conversationOnly=1&conversationWindow={label}");
    let (route, remote_id) = route_with_new_remote_window(route, remote_connection_id);
    let transfer = Transfer::new(
        (window.label(), &label),
        payload,
        (remote_connection_id.is_some(), true),
    );
    app.state::<ConversationWindowState>()
        .1
        .lock()
        .await
        .insert(label.clone(), window.label().to_owned());
    app.state::<ConversationWindowState>()
        .0
        .lock()
        .await
        .insert(label.clone(), transfer);
    if let Err(error) = build_window(&app, &label, (route, remote_id.as_deref())) {
        app.state::<ConversationWindowState>()
            .0
            .lock()
            .await
            .remove(&label);
        app.state::<ConversationWindowState>()
            .1
            .lock()
            .await
            .remove(&label);
        return Err(error);
    }
    tracing::info!(
        source = window.label(),
        target = label,
        "[conversation-window] prepared"
    );
    Ok(label)
}

#[tauri::command]
pub async fn get_conversation_window_transfer(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
) -> Result<TransferState, AppCommandError> {
    let state = app.state::<ConversationWindowState>();
    let transfers = state.0.lock().await;
    let transfer = authorized(&transfers, &window, &label)?;
    Ok(TransferState {
        phase: transfer.phase,
        payload: (window.label() == transfer.target && transfer.phase != "ready")
            .then(|| transfer.payload.clone()),
    })
}

fn authorized<'a>(
    transfers: &'a HashMap<String, Transfer>,
    window: &WebviewWindow,
    label: &str,
) -> Result<&'a Transfer, AppCommandError> {
    transfers
        .get(label)
        .filter(|transfer| window.label() == transfer.target || window.label() == transfer.source)
        .ok_or_else(|| {
            AppCommandError::invalid_input("Conversation window transfer is unavailable")
        })
}

#[tauri::command]
pub async fn ready_conversation_window(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
) -> Result<(), AppCommandError> {
    let state = app.state::<ConversationWindowState>();
    let mut transfers = state.0.lock().await;
    let transfer = transfers
        .get_mut(&label)
        .filter(|item| item.target == window.label())
        .ok_or_else(|| AppCommandError::invalid_input("Conversation window transfer expired"))?;
    if transfer.phase == "preparing" {
        transfer.phase = "ready";
    }
    tracing::info!(target = window.label(), "[conversation-window] ready");
    Ok(())
}

#[tauri::command]
pub async fn commit_conversation_window(
    window: WebviewWindow,
    label: String,
    snapshot: Value,
) -> Result<(), AppCommandError> {
    let app = window.app_handle().clone();
    let state = app.state::<ConversationWindowState>();
    let mut transfers = state.0.lock().await;
    let transfer = authorized(&transfers, &window, &label)?;
    if window.label() != transfer.source || transfer.phase != "ready" {
        return Err(AppCommandError::invalid_input(
            "Conversation window is not ready",
        ));
    }
    if snapshot["tab"]["id"] != transfer.payload["tab"]["id"] {
        return Err(AppCommandError::invalid_input(
            "Conversation tab changed during transfer",
        ));
    }
    let target = app
        .get_webview_window(&transfer.target)
        .ok_or_else(|| AppCommandError::invalid_input("Conversation window was closed"))?;
    target
        .unminimize()
        .and_then(|_| target.show())
        .map_err(|e| {
            AppCommandError::window("Could not show conversation window", e.to_string())
        })?;
    transfer_connection_owner(&app, transfer, &transfer.target).await?;
    let transfer = transfers
        .get_mut(&label)
        .expect("authorized transfer exists");
    transfer.phase = "committed";
    transfer.payload["tab"] = snapshot["tab"].clone();
    transfer.payload["runtime"] = snapshot["runtime"].clone();
    transfer.payload["draft"] = snapshot["draft"].clone();
    tracing::info!(
        source = window.label(),
        target = label,
        "[conversation-window] committed"
    );
    if let Err(error) = target.set_focus() {
        tracing::warn!(target = label, %error, "[conversation-window] focus failed");
    }
    Ok(())
}

async fn transfer_connection_owner(
    app: &AppHandle,
    transfer: &Transfer,
    target: &str,
) -> Result<(), AppCommandError> {
    if transfer.remote
        || transfer.payload["connection"]["isViewer"].as_bool() == Some(true)
        || transfer.payload["connection"]["isDelegationChild"].as_bool() == Some(true)
    {
        return Ok(());
    }
    let Some(id) = transfer.connection_id.as_deref() else {
        return Ok(());
    };
    let manager = app.state::<ConnectionManager>();
    let mut connections = manager.connections.lock().await;
    let connection = connections.get_mut(id).ok_or_else(|| {
        AppCommandError::invalid_input("Conversation connection ended during transfer")
    })?;
    if connection.owner_window_label != transfer.source {
        return Err(AppCommandError::invalid_input(
            "Conversation connection owner changed",
        ));
    }
    connection.owner_window_label = target.to_owned();
    connection.state.write().await.owner_window_label = target.to_owned();
    Ok(())
}

#[tauri::command]
pub async fn finish_conversation_window(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
) -> Result<(), AppCommandError> {
    let state = app.state::<ConversationWindowState>();
    let mut transfers = state.0.lock().await;
    let transfer = authorized(&transfers, &window, &label)?;
    if transfer.phase == "committed" {
        if window.label() == transfer.target {
            transfers.remove(&label);
        }
        return Ok(());
    }
    if let Some(target) = app
        .get_webview_window(&transfer.target)
        .filter(|_| transfer.new_window)
    {
        target.destroy().map_err(|e| {
            AppCommandError::window(
                "Could not close unfinished conversation window",
                e.to_string(),
            )
        })?;
    }
    transfers.remove(&label);
    tracing::info!(target = label, "[conversation-window] rolled back");
    Ok(())
}
