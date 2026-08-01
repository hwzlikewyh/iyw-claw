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

use super::backends;
use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::types::{ChannelRuntimeStatus, ChannelType};
use crate::app_error::AppCommandError;
use crate::db::entities::chat_channel;
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

    fn failed(channel_id: i32, desired_enabled: bool, runtime_status: &str, error: String) -> Self {
        Self {
            channel_id,
            desired_enabled,
            runtime_status: runtime_status.to_string(),
            connected: false,
            error: Some(error),
        }
    }
}

/// Per-type credential readiness. WeCom owns credentials inside wecom-cli
/// (QR-scan auth) and must NOT be gated on a channel token
/// (IYW-CHANNEL-003); Lark/Weixin use the keyring token.
pub async fn credential_ready(
    db: &DatabaseConnection,
    model: &chat_channel::Model,
) -> Result<(), String> {
    let channel_type = parse_channel_type(model)?;
    match channel_type {
        ChannelType::Wecom => {
            if !backends::wecom::cli_installed() {
                return Err("wecom-cli 未安装，请先点击授权安装".to_string());
            }
            match backends::wecom::auth_status().await {
                Ok(true) => Ok(()),
                Ok(false) => Err("企微尚未完成扫码授权，请先在设置中完成授权".to_string()),
                Err(error) => Err(format!("企微授权状态检查失败：{error}")),
            }
        }
        ChannelType::Lark | ChannelType::Weixin => {
            if crate::keyring_store::get_channel_token(model.id).is_none() {
                let hint = if channel_type == ChannelType::Weixin {
                    "请先扫码完成微信授权"
                } else {
                    "请先保存 App Secret"
                };
                Err(format!("缺少渠道凭据（{hint}）"))
            } else {
                Ok(())
            }
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
    let model = chat_channel_service::get_by_id(db, id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found(format!("Chat channel {id} not found")))?;

    // Disable / delete / credential revoke path: idempotent stop.
    if !desired_enabled {
        if manager.remove_channel(id).await.is_ok() {
            manager.emit_channel_status(id, "disconnected").await;
        }
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

    // Fast path: already connected with intact credentials — nothing to
    // rebuild. Keeps the wecom QR polling loop (and repeated saves) cheap and
    // idempotent.
    if manager.is_connected(id).await {
        if let Err(message) = credential_ready(db, &model).await {
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
        Ok(()) => {
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
            tracing::info!("[reconcile] channel {id} ({reason}): connected");
            ReconcileOutcome::ok(updated.id, true, &updated.runtime_status, true)
        }
        Err(error) => {
            let message = error.to_string();
            tracing::error!("[reconcile] channel {id} ({reason}) failed: {message}");
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

/// Build + start the backend for an enabled channel, performing a safe
/// reconnect: on failure the previously running backend is restored.
async fn reconcile_connect(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
) -> Result<(), ChatChannelError> {
    let channel_type = parse_channel_type(model)?;
    let config: serde_json::Value = serde_json::from_str(&model.config_json).map_err(|e| {
        ChatChannelError::ConfigurationInvalid(format!(
            "渠道配置不是有效 JSON（{e}）；请重新保存配置修复"
        ))
    })?;

    // Credential gate BEFORE factory construction (IYW-CHANNEL-003).
    if let Err(message) = credential_ready(db, model).await {
        return Err(ChatChannelError::AuthenticationFailed(message));
    }

    let token = match channel_type {
        ChannelType::Wecom => String::new(),
        ChannelType::Lark | ChannelType::Weixin => {
            crate::keyring_store::get_channel_token(model.id).unwrap_or_default()
        }
    };

    let previous = manager.take_backend(model.id).await;

    // Build the new backend inside the same fallible block so a config error
    // restores the previous (last-known-good) backend instead of dropping it.
    let result = async {
        let backend = backends::create_backend(model.id, channel_type, &config, token)
            .map_err(|e| ChatChannelError::ConfigurationInvalid(e.to_string()))?;
        manager
            .upsert_channel(
                model.id,
                model.name.clone(),
                channel_type,
                std::sync::Arc::from(backend),
            )
            .await
    }
    .await;

    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            // Last-known-good rollback: restore the previous backend if the
            // new config/credential failed to build or start.
            if let Some(previous_backend) = previous {
                let restore = manager
                    .restore_backend(
                        model.id,
                        model.name.clone(),
                        channel_type,
                        previous_backend,
                    )
                    .await;
                if let Err(restore_error) = restore {
                    tracing::error!(
                        "[reconcile] channel {} restore failed: {restore_error}",
                        model.id
                    );
                }
            }
            Err(error)
        }
    }
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

fn parse_channel_type(model: &chat_channel::Model) -> Result<ChannelType, ChatChannelError> {
    serde_json::from_value(serde_json::Value::String(model.channel_type.clone())).map_err(|_| {
        ChatChannelError::ConfigurationInvalid(format!(
            "未知渠道类型：{}",
            model.channel_type
        ))
    })
}
