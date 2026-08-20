use crate::app_error::AppCommandError;
use crate::chat_channel::config_patch::{apply_config_patch, ChatChannelConfigPatch};
use crate::chat_channel::manager::ChatChannelManager;
use crate::chat_channel::reconcile;
use crate::chat_channel::target_registry;
use crate::db::entities::chat_channel;
use crate::db::service::chat_channel_service;
use crate::db::AppDatabase;

use super::session::Session;
use super::types::ProviderCredentials;

pub(super) enum CommitOutcome {
    Connected,
    Cancelled,
}

pub async fn commit_credentials(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    session: &Session,
    credentials: ProviderCredentials,
) -> Result<CommitOutcome, AppCommandError> {
    let _guard = crate::chat_channel::operation_lock::lock_channel(session.channel_id).await;
    if !session.try_begin_commit() {
        return Ok(CommitOutcome::Cancelled);
    }
    let current = load_current_channel(db, session).await?;
    let config = build_patched_config(&current.config_json, credentials.config_patch)?;
    let updated = persist_credentials(db, session.channel_id, credentials.token, config).await?;
    register_default_target(db, session.channel_id, &updated).await;
    require_connected(db, manager, session.channel_id).await?;
    Ok(CommitOutcome::Connected)
}

async fn load_current_channel(
    db: &AppDatabase,
    session: &Session,
) -> Result<chat_channel::Model, AppCommandError> {
    let current = chat_channel_service::get_by_id(&db.conn, session.channel_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| {
            AppCommandError::not_found(format!("Chat channel {} not found", session.channel_id))
        })?;
    let current_type: crate::chat_channel::types::ChannelType =
        serde_json::from_value(serde_json::Value::String(current.channel_type.clone()))
            .map_err(|_| AppCommandError::configuration_invalid("渠道类型已变更，请重新扫码"))?;
    if current_type != session.channel_type {
        return Err(AppCommandError::invalid_input(
            "扫码渠道类型已变更，请重新扫码",
        ));
    }
    Ok(current)
}

fn build_patched_config(
    current_config: &str,
    config_patch: serde_json::Value,
) -> Result<String, AppCommandError> {
    let patch: ChatChannelConfigPatch = serde_json::from_value(config_patch).map_err(|error| {
        AppCommandError::configuration_invalid("扫码凭据配置无效").with_detail(error.to_string())
    })?;
    apply_config_patch(current_config, &patch).map_err(|error| {
        AppCommandError::configuration_invalid("渠道配置更新失败").with_detail(error)
    })
}

async fn persist_credentials(
    db: &AppDatabase,
    channel_id: i32,
    token: String,
    config: String,
) -> Result<chat_channel::Model, AppCommandError> {
    let backup = crate::keyring_store::try_get_channel_token(channel_id)
        .map_err(|error| AppCommandError::io_error("读取原有渠道凭据失败").with_detail(error))?;
    crate::keyring_store::set_channel_token(channel_id, &token)
        .map_err(|error| AppCommandError::io_error("保存扫码凭据失败").with_detail(error))?;
    match chat_channel_service::update(
        &db.conn,
        channel_id,
        None,
        Some(true),
        Some(config),
        None,
        None,
        None,
    )
    .await
    {
        Ok(updated) => Ok(updated),
        Err(error) => {
            restore_token(channel_id, backup.as_deref());
            Err(AppCommandError::from(error))
        }
    }
}

async fn register_default_target(db: &AppDatabase, channel_id: i32, channel: &chat_channel::Model) {
    if let Err(error) = target_registry::register_default(&db.conn, channel).await {
        tracing::warn!(
            channel_id,
            error = %error,
            "[ChatChannel] QR credential saved but default target registration failed"
        );
    }
}

async fn require_connected(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    channel_id: i32,
) -> Result<(), AppCommandError> {
    let outcome =
        reconcile::reconcile_channel_unlocked(&db.conn, manager, channel_id, true, "qr_completed")
            .await?;
    if outcome.connected {
        return Ok(());
    }
    Err(
        AppCommandError::network("扫码成功，但渠道传输连接未建立").with_detail(
            outcome
                .error
                .unwrap_or_else(|| "runtime_status 未达到 connected".to_string()),
        ),
    )
}

fn restore_token(channel_id: i32, backup: Option<&str>) {
    let result = match backup {
        Some(token) => crate::keyring_store::set_channel_token(channel_id, token),
        None => crate::keyring_store::delete_channel_token(channel_id),
    };
    if let Err(error) = result {
        tracing::error!(channel_id, error = %error, "[ChatChannel] QR credential rollback failed");
    }
}
