use sea_orm::DatabaseConnection;

use super::backends;
use super::error::ChatChannelError;
use super::manager::ChatChannelManager;
use super::reconcile_credentials::{credential_ready, parse_channel_type};
use super::types::{ChannelConnectionStatus, ChannelType};
use crate::db::entities::chat_channel;

pub(super) fn connection_status_label(status: Option<ChannelConnectionStatus>) -> &'static str {
    match status {
        Some(ChannelConnectionStatus::Connected) => "connected",
        Some(ChannelConnectionStatus::Connecting) => "connecting",
        Some(ChannelConnectionStatus::Disconnected) => "disconnected",
        Some(ChannelConnectionStatus::Error) => "error",
        None => "missing",
    }
}

pub(super) fn requires_backend_rebuild(reason: &'static str) -> bool {
    matches!(reason, "edit" | "credential" | "qr_completed")
}

pub(super) async fn reconcile_connect(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
) -> Result<(), ChatChannelError> {
    let channel_type = parse_channel_type(model)?;
    let config: serde_json::Value = serde_json::from_str(&model.config_json).map_err(|error| {
        ChatChannelError::ConfigurationInvalid(format!(
            "渠道配置不是有效 JSON（{error}）；请重新保存配置修复"
        ))
    })?;
    if channel_type == ChannelType::WecomAgent {
        super::backends::wecom_agent::ensure_ready_config(&config)?;
    }
    if let Err(message) = credential_ready(db, model).await {
        return Err(ChatChannelError::AuthenticationFailed(message));
    }

    let token = channel_token(model.id, channel_type);
    let previous = manager.take_backend(model.id).await;
    let result = build_backend(db, manager, model, channel_type, config, token).await;
    if let Err(error) = result {
        restore_previous(manager, model, channel_type, previous).await;
        return Err(error);
    }
    Ok(())
}

fn channel_token(channel_id: i32, channel_type: ChannelType) -> String {
    match channel_type {
        ChannelType::Wecom => String::new(),
        _ => crate::keyring_store::get_channel_token(channel_id).unwrap_or_default(),
    }
}

async fn build_backend(
    db: &DatabaseConnection,
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
    channel_type: ChannelType,
    config: serde_json::Value,
    token: String,
) -> Result<(), ChatChannelError> {
    let backend = backends::create_backend(model.id, channel_type, &config, token, db.clone())
        .map_err(|error| ChatChannelError::ConfigurationInvalid(error.to_string()))?;
    manager
        .upsert_channel(
            model.id,
            model.name.clone(),
            channel_type,
            std::sync::Arc::from(backend),
        )
        .await
}

async fn restore_previous(
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
    channel_type: ChannelType,
    previous: Option<std::sync::Arc<dyn super::traits::ChatChannelBackend>>,
) {
    let Some(previous) = previous else {
        return;
    };
    if let Err(error) = manager
        .restore_backend(model.id, model.name.clone(), channel_type, previous)
        .await
    {
        tracing::error!(channel_id = model.id, error = %error, "[reconcile] channel restore failed");
    }
}
