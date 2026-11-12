use super::ConversationWindowState;
use crate::acp::manager::ConnectionManager;
use tauri::{AppHandle, Manager};

pub fn window_destroyed(app: &AppHandle, label: &str) {
    let app = app.clone();
    let label = label.to_owned();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<ConversationWindowState>();
        let mut transfers = state.0.lock().await;
        let targets: Vec<_> = transfers
            .iter()
            .filter_map(|(target, transfer)| {
                (transfer.target == label
                    || (transfer.source == label && transfer.phase != "committed"))
                    .then(|| target.clone())
            })
            .collect();
        for target in targets {
            let removed = transfers.remove(&target);
            if target != label && removed.is_some_and(|item| item.new_window) {
                close_abandoned_window(&app, &target);
            }
        }
        drop(transfers);
        state.1.lock().await.remove(&label);
        state.2.lock().await.remove(&label);
        if label.starts_with("conversation-") {
            release_idle_connections(&app, &label).await;
        }
    });
}

fn close_abandoned_window(app: &AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
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
