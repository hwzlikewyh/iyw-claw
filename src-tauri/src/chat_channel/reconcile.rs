//! Unified channel reconcile entry point.
//!
//! Every state-changing path (create, enable, edit, credential save, QR
//! completion, app start, manual connect/disconnect) funnels through
//! `reconcile_channel`, which reads the *latest* DB model, decides credential
//! readiness per channel type, and drives the backend to the desired state:
//!
//! - `desired = false` → idempotent stop/remove (never leaves a running task).
//! - `desired = true`  → validate config → create backend → start → persist
//!   runtime state; on failure keep the `enabled` intent and record the error.
//!
//! Config/credential changes perform a safe reconnect: the new backend wins,
//! and on failure the previous backend is restored (last-known-good).

use sea_orm::DatabaseConnection;

use super::manager::ChatChannelManager;
use super::reconcile_connect::{
    connection_status_label, reconcile_connect, requires_backend_rebuild,
};
pub use super::reconcile_credentials::credential_ready;
use super::types::{ChannelConnectionStatus, ChannelRuntimeStatus};
use crate::app_error::AppCommandError;
use crate::db::service::chat_channel_service;

/// Machine-readable reason for telemetry/logging.
pub type ReconcileReason = &'static str;

/// Outcome of one reconcile attempt, returned to callers (commands) so they
/// can surface "已保存，连接失败" instead of silently closing dialogs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconcileOutcome {
    pub channel_id: i32,
    pub desired_enabled: bool,
    /// Final persisted runtime status.
    pub runtime_status: String,
    pub connected: bool,
    pub error: Option<String>,
}

impl ReconcileOutcome {
    fn ok(channel_id: i32, desired_enabled: bool, runtime_status: &str, connected: bool) -> Self {
        Self {
            channel_id,
            desired_enabled,
            runtime_status: runtime_status.to_string(),
            connected,
            error: None,
        }
    }

    pub(crate) fn failed(
        channel_id: i32,
        desired_enabled: bool,
        runtime_status: &str,
        error: String,
    ) -> Self {
        Self {
            channel_id,
            desired_enabled,
            runtime_status: runtime_status.to_string(),
            connected: false,
            error: Some(error),
        }
    }
}

/// Reconcile one channel to `desired_enabled`. `reason` is a short kebab
/// label for structured logs, e.g. `create`, `enable`, `edit`, `credential`,
/// `qr_completed`, `app_start`, `manual`.
#[allow(clippy::too_many_arguments)]
pub async fn reconcile_channel(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    id: i32,
    desired_enabled: bool,
    reason: ReconcileReason,
) -> Result<ReconcileOutcome, AppCommandError> {
    let _guard = super::operation_lock::lock_channel(id).await;
    reconcile_channel_unlocked(db, manager, id, desired_enabled, reason).await
}

pub(crate) async fn reconcile_channel_unlocked(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    id: i32,
    desired_enabled: bool,
    reason: ReconcileReason,
) -> Result<ReconcileOutcome, AppCommandError> {
    let model = chat_channel_service::get_by_id(db, id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found(format!("Chat channel {id} not found")))?;

    // Disable / delete / credential revoke path: idempotent stop.
    if !desired_enabled {
        manager
            .remove_channel(id)
            .await
            .map_err(AppCommandError::from)?;
        manager.emit_channel_status(id, "disconnected").await;
        let updated = chat_channel_service::update_runtime(
            db,
            id,
            Some(ChannelRuntimeStatus::Disconnected.as_str().to_string()),
            None,
            None,
            None,
        )
        .await
        .map_err(AppCommandError::from)?;
        tracing::info!(
            "[reconcile] channel {id} ({reason}): desired=disabled, runtime=disconnected"
        );
        return Ok(ReconcileOutcome::ok(
            updated.id,
            false,
            &updated.runtime_status,
            false,
        ));
    }

    // Fast path for idempotent connect/app-start calls. Config and credential
    // changes always rebuild so a live backend cannot keep stale values.
    if manager.is_connected(id).await && !requires_backend_rebuild(reason) {
        if let Err(message) = credential_ready(db, manager, &model).await {
            let updated = chat_channel_service::update_runtime(
                db,
                id,
                Some(ChannelRuntimeStatus::Error.as_str().to_string()),
                Some(Some(message.clone())),
                Some(Some(chrono::Utc::now())),
                None,
            )
            .await
            .map_err(AppCommandError::from)?;
            manager.emit_channel_status(id, "error").await;
            return Ok(ReconcileOutcome::failed(
                updated.id,
                true,
                &updated.runtime_status,
                message,
            ));
        }
        let updated = chat_channel_service::update_runtime(
            db,
            id,
            Some(ChannelRuntimeStatus::Connected.as_str().to_string()),
            None,
            None,
            None,
        )
        .await
        .map_err(AppCommandError::from)?;
        return Ok(ReconcileOutcome::ok(
            updated.id,
            true,
            &updated.runtime_status,
            true,
        ));
    }

    // Enable path: mark connecting, then try to build + start the backend.
    chat_channel_service::update_runtime(
        db,
        id,
        Some(ChannelRuntimeStatus::Connecting.as_str().to_string()),
        None,
        None,
        None,
    )
    .await
    .map_err(AppCommandError::from)?;
    manager.emit_channel_status(id, "connecting").await;

    let outcome = match reconcile_connect(db, manager, &model).await {
        Ok(()) => match manager.connection_status(id).await {
            Some(ChannelConnectionStatus::Connected) => {
                let updated = chat_channel_service::update_runtime(
                    db,
                    id,
                    Some(ChannelRuntimeStatus::Connected.as_str().to_string()),
                    Some(None),
                    Some(None),
                    Some(Some(chrono::Utc::now())),
                )
                .await
                .map_err(AppCommandError::from)?;
                tracing::info!(channel_id = id, reason, "[reconcile] transport connected");
                ReconcileOutcome::ok(updated.id, true, &updated.runtime_status, true)
            }
            Some(ChannelConnectionStatus::Connecting) => {
                let updated = chat_channel_service::update_runtime(
                    db,
                    id,
                    Some(ChannelRuntimeStatus::Connecting.as_str().to_string()),
                    None,
                    None,
                    None,
                )
                .await
                .map_err(AppCommandError::from)?;
                tracing::info!(
                    channel_id = id,
                    reason,
                    "[reconcile] backend started; waiting for transport readiness"
                );
                ReconcileOutcome::ok(updated.id, true, &updated.runtime_status, false)
            }
            status => {
                let message = format!(
                    "渠道启动后未建立传输连接（状态：{}）",
                    connection_status_label(status)
                );
                tracing::warn!(
                    channel_id = id,
                    reason,
                    transport_status = connection_status_label(status),
                    "[reconcile] backend startup did not produce a live transport"
                );
                let updated = chat_channel_service::update_runtime(
                    db,
                    id,
                    Some(ChannelRuntimeStatus::Error.as_str().to_string()),
                    Some(Some(message.clone())),
                    Some(Some(chrono::Utc::now())),
                    None,
                )
                .await
                .map_err(AppCommandError::from)?;
                manager.emit_channel_status(id, "error").await;
                ReconcileOutcome::failed(updated.id, true, &updated.runtime_status, message)
            }
        },
        Err(error) => {
            let message = error.to_string();
            tracing::error!(
                channel_id = id,
                reason,
                error = %message,
                "[reconcile] channel startup failed"
            );
            let updated = chat_channel_service::update_runtime(
                db,
                id,
                Some(ChannelRuntimeStatus::Error.as_str().to_string()),
                Some(Some(message.clone())),
                Some(Some(chrono::Utc::now())),
                None,
            )
            .await
            .map_err(AppCommandError::from)?;
            manager.emit_channel_status(id, "error").await;
            ReconcileOutcome::failed(updated.id, true, &updated.runtime_status, message)
        }
    };
    Ok(outcome)
}

/// Reconcile every enabled channel at app start.
pub async fn reconcile_all_enabled(
    manager: &ChatChannelManager,
    db: &DatabaseConnection,
    reason: ReconcileReason,
) {
    let channels = match chat_channel_service::list_enabled(db).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("[reconcile] failed to load enabled channels: {e}");
            return;
        }
    };
    for ch in channels {
        if let Err(error) = reconcile_channel(db, manager, ch.id, true, reason).await {
            tracing::error!("[reconcile] channel {} ({reason}) errored: {error}", ch.id);
        }
    }
}
