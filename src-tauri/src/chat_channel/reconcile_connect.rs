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
    let data_dir = manager.data_dir().await;
    if channel_type == ChannelType::Wecom {
        let data_dir = data_dir.as_deref().ok_or_else(|| {
            ChatChannelError::ConfigurationInvalid(
                "应用数据目录尚未初始化，无法准备企业微信 CLI".into(),
            )
        })?;
        crate::wecom_ai::ensure_cli(data_dir)
            .await
            .map_err(|error| ChatChannelError::ConnectionFailed(error.to_string()))?;
    }
    if let Err(message) = credential_ready(db, manager, model).await {
        return Err(ChatChannelError::AuthenticationFailed(message));
    }

    let token = channel_token(model.id, channel_type);
    let previous = manager.take_backend(model.id).await;
    let request = backends::CreateBackendRequest {
        channel_id: model.id,
        channel_type,
        config: &config,
        token,
        database: db.clone(),
        data_dir,
    };
    let result = build_backend(manager, model, request).await;
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
    manager: &ChatChannelManager,
    model: &chat_channel::Model,
    request: backends::CreateBackendRequest<'_>,
) -> Result<(), ChatChannelError> {
    let channel_type = request.channel_type;
    let backend = backends::create_backend(request)
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
