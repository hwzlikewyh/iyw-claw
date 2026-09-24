use serde::Serialize;

use super::service::ChannelToolService;
use crate::chat_channel::artifact_notification_policy::{self, ArtifactNotificationMode};
use crate::db::entities::chat_channel;
use crate::db::service::{chat_channel_service, chat_channel_target_service};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactChannelTarget {
    pub channel_id: i32,
    pub name: String,
    pub notification_mode: ArtifactNotificationMode,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub error: Option<&'static str>,
}

impl ChannelToolService {
    pub async fn artifact_channel_targets(&self) -> Result<Vec<ArtifactChannelTarget>, String> {
        let channels = chat_channel_service::list_enabled(&self.db.conn)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "[artifact-notifications] channel query failed");
                "CHANNEL_QUERY_FAILED"
            })?;
        let mut targets = Vec::with_capacity(channels.len());
        for channel in channels {
            targets.push(self.artifact_channel_target(channel).await);
        }
        Ok(targets)
    }

    async fn artifact_channel_target(&self, channel: chat_channel::Model) -> ArtifactChannelTarget {
        let mut result = ArtifactChannelTarget {
            channel_id: channel.id,
            name: channel.name,
            notification_mode: artifact_notification_policy::from_config(&channel.config_json)
                .unwrap_or_else(|error| {
                    tracing::warn!(channel_id = channel.id, error = %error,
                        "[artifact-notifications] invalid channel policy; automatic notifications disabled");
                    ArtifactNotificationMode::Off
                }),
            target_id: None,
            target_name: None,
            error: None,
        };
        let targets =
            match chat_channel_target_service::list_by_channel(&self.db.conn, channel.id).await {
                Ok(targets) => targets,
                Err(error) => {
                    tracing::error!(channel_id = channel.id, error = %error,
                    "[artifact-notifications] target query failed");
                    result.error = Some("TARGET_QUERY_FAILED");
                    return result;
                }
            };
        // 使用宿主已登记的默认目标，不能把所有历史联系人都当作收件人。
        if let Some(target) = targets.into_iter().find(|target| target.is_default) {
            result.target_id = Some(target.target_id);
            result.target_name = Some(target.display_name);
        } else {
            result.error = Some("TARGET_NOT_FOUND");
        }
        if channel.runtime_status != "connected" || !self.manager.is_connected(channel.id).await {
            result.error = Some("CHANNEL_NOT_CONNECTED");
        }
        result
    }
}
