use super::route_with_new_remote_window;
use crate::{acp::manager::ConnectionManager, app_error::AppCommandError};
use appearance::build_window;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::Mutex;

mod appearance;

#[derive(Default)]
pub struct ConversationWindowState(Mutex<HashMap<String, Transfer>>);

struct Transfer {
    source: String,
    payload: Value,
    phase: &'static str,
    remote: bool,
    connection_id: Option<String>,
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
    let route = format!("workspace?conversationWindow={label}");
    let (route, remote_id) = route_with_new_remote_window(route, remote_connection_id);
    let connection_id = payload["connection"]["connectionId"]
        .as_str()
        .map(str::to_owned);
    let transfer = Transfer {
        source: window.label().to_owned(),
        payload,
        phase: "preparing",
        remote: remote_connection_id.is_some(),
        connection_id,
    };
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
        payload: (window.label() == label && transfer.phase != "ready")
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
        .filter(|transfer| window.label() == label || window.label() == transfer.source)
        .ok_or_else(|| {
            AppCommandError::invalid_input("Conversation window transfer is unavailable")
        })
}

#[tauri::command]
pub async fn ready_conversation_window(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<(), AppCommandError> {
    let state = app.state::<ConversationWindowState>();
    let mut transfers = state.0.lock().await;
    let transfer = transfers
        .get_mut(window.label())
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
        .get_webview_window(&label)
        .ok_or_else(|| AppCommandError::invalid_input("Conversation window was closed"))?;
    target.show().map_err(|e| {
        AppCommandError::window("Could not show conversation window", e.to_string())
    })?;
    transfer_connection_owner(&app, transfer, &label).await?;
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
        if window.label() == label {
            transfers.remove(&label);
        }
        return Ok(());
    }
    if let Some(target) = app.get_webview_window(&label) {
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

pub fn window_destroyed(app: &AppHandle, label: &str) {
    let app = app.clone();
    let label = label.to_owned();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<ConversationWindowState>();
        let mut transfers = state.0.lock().await;
        let targets: Vec<_> = transfers
            .iter()
            .filter_map(|(target, transfer)| {
                (target == &label || (transfer.source == label && transfer.phase != "committed"))
                    .then(|| target.clone())
            })
            .collect();
        for target in targets {
            transfers.remove(&target);
            if target != label {
                close_abandoned_window(&app, &target);
            }
        }
        drop(transfers);
        if label.starts_with("conversation-") {
            release_idle_connections(&app, &label).await;
        }
    });
}

fn close_abandoned_window(app: &AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else { return };
    if let Err(error) = window.destroy() {
        tracing::warn!(window_label = label, %error, "[conversation-window] abandoned window cleanup failed");
    }
}

async fn release_idle_connections(app: &AppHandle, label: &str) {
    let manager = app.state::<ConnectionManager>();
    let ids: Vec<_> = manager
        .connections
        .lock()
        .await
        .iter()
        .filter(|(_, connection)| connection.owner_window_label == label)
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Err(error) = manager
            .disconnect_if_reclaimable(
                &id,
                std::time::Duration::ZERO,
                true,
                "conversation_window_close",
            )
            .await
        {
            tracing::warn!(connection_id = id, %error, "[conversation-window] idle cleanup failed");
        }
    }
}
