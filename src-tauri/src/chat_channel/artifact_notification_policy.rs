use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::db::service::chat_channel_service;
use crate::db::AppDatabase;

const POLICY_KEY: &str = "artifact_notification_mode";
pub const MAX_NOTIFICATION_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactNotificationMode {
    #[default]
    Off,
    Auto,
    Always,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactNotificationOptions {
    #[serde(default)]
    pub notify: bool,
    pub notification_message: Option<String>,
}

impl ArtifactNotificationOptions {
    pub fn validate(&mut self) -> Result<(), String> {
        if let Some(message) = &mut self.notification_message {
            *message = message.trim().to_owned();
            if message.is_empty()
                || message.contains('\0')
                || message.chars().count() > MAX_NOTIFICATION_CHARS
            {
                return Err(format!(
                    "notification_message must contain 1 to {MAX_NOTIFICATION_CHARS} characters without NUL"
                ));
            }
        }
        Ok(())
    }
}

pub fn from_config(config_json: &str) -> Result<ArtifactNotificationMode, AppCommandError> {
    let config = super::config_patch::parse_config_object(config_json)
        .map_err(AppCommandError::configuration_invalid)?;
    match config.get(POLICY_KEY) {
        None => Ok(ArtifactNotificationMode::Off),
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| {
            AppCommandError::configuration_invalid("Invalid artifact notification mode")
        }),
    }
}

pub async fn get_mode(
    db: &AppDatabase,
    channel_id: i32,
) -> Result<ArtifactNotificationMode, AppCommandError> {
    let channel = chat_channel_service::get_by_id(&db.conn, channel_id)
        .await?
        .ok_or_else(|| AppCommandError::not_found("Channel not found"))?;
    from_config(&channel.config_json)
}

pub async fn set_mode(
    db: &AppDatabase,
    channel_id: i32,
    mode: ArtifactNotificationMode,
) -> Result<(), AppCommandError> {
    let _guard = super::operation_lock::lock_channel(channel_id).await;
    let channel = chat_channel_service::get_by_id(&db.conn, channel_id)
        .await?
        .ok_or_else(|| AppCommandError::not_found("Channel not found"))?;
    let mut config = super::config_patch::parse_config_object(&channel.config_json)
        .map_err(AppCommandError::configuration_invalid)?;
    config.insert(POLICY_KEY.into(), serde_json::json!(mode));
    let json = serde_json::Value::Object(config).to_string();
    chat_channel_service::update(
        &db.conn,
        channel_id,
        None,
        None,
        Some(json),
        None,
        None,
        None,
    )
    .await?;
    tracing::info!(channel_id, mode = ?mode, "[artifact-notifications] channel policy updated");
    Ok(())
}
