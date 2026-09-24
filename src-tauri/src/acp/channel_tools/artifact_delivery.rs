use std::sync::Arc;

use futures_util::{stream, StreamExt};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::artifact_targets::ArtifactChannelTarget;
use super::service::{ChannelToolService, MutationStart};
use super::types::{ChannelCaller, SendMessagesInput};
use crate::chat_channel::artifact_notification_policy::{
    ArtifactNotificationMode, ArtifactNotificationOptions,
};
use crate::db::entities::task_artifact;

const MAX_ARTIFACTS: usize = 100;
const DELIVERY_OPERATION: &str = "deliver_task_artifacts";
const CHANNEL_SEND_WORKERS: usize = 4;

pub struct ArtifactDeliveryRequest {
    pub artifact_ids: Vec<i32>,
    pub channel_id: Option<i32>,
    pub request_id: String,
    pub message: Option<String>,
}

impl ChannelToolService {
    pub async fn notify_artifacts(
        self: &Arc<Self>,
        caller: ChannelCaller,
        registration: &Value,
        options: ArtifactNotificationOptions,
    ) -> Value {
        let ids = accepted_ids(registration);
        if ids.is_empty() {
            return json!({"status": "skipped", "reason": "NO_ACCEPTED_ARTIFACTS"});
        }
        let targets = match self.artifact_channel_targets().await {
            Ok(targets) => targets,
            Err(error) => return json!({"status": "failed", "error": error}),
        };
        let targets = targets.into_iter().filter(|target| {
            target.notification_mode == ArtifactNotificationMode::Always
                || (target.notification_mode == ArtifactNotificationMode::Auto && options.notify)
        });
        let registration = (ids.as_slice(), &options);
        let mut results = stream::iter(targets)
            .map(|target| self.notify_artifact_channel(caller.clone(), registration, target))
            .buffer_unordered(CHANNEL_SEND_WORKERS)
            .collect::<Vec<_>>()
            .await;
        results.sort_by_key(|result| result["channel_id"].as_i64());
        json!({"status": super::send_result::batch_status(&results),
            "delivery_unknown": results.iter().any(delivery_unknown), "items": results})
    }

    async fn notify_artifact_channel(
        self: &Arc<Self>,
        mut caller: ChannelCaller,
        registration: (&[i32], &ArtifactNotificationOptions),
        target: ArtifactChannelTarget,
    ) -> Value {
        let (ids, options) = registration;
        caller.caller_scope = "artifact-notifications".into();
        let request = ArtifactDeliveryRequest {
            request_id: format!(
                "artifacts:{:x}:{}",
                Sha256::digest(json!(ids).to_string().as_bytes()),
                target.channel_id,
            ),
            artifact_ids: ids.to_vec(),
            channel_id: Some(target.channel_id),
            message: options.notification_message.clone(),
        };
        let batch = self.deliver_artifacts(caller, request).await;
        let mut result = batch["items"]
            .as_array()
            .and_then(|items| items.first())
            .cloned()
            .unwrap_or(batch);
        result["channel_id"] = json!(target.channel_id);
        result["channel_name"] = json!(target.name);
        result["mode"] = json!(target.notification_mode);
        result
    }

    pub async fn deliver_artifacts(
        self: &Arc<Self>,
        caller: ChannelCaller,
        request: ArtifactDeliveryRequest,
    ) -> Value {
        let service = Arc::clone(self);
        // 调用者断开后继续完成已开始的投递，并保留幂等记录供同一次请求查询。
        tokio::spawn(async move {
            service.deliver_artifacts_inner(caller, request).await
                .unwrap_or_else(|error| {
                    tracing::warn!(error, "[artifact-notifications] delivery failed");
                    json!({"status": if error == "IDEMPOTENCY_UNAVAILABLE" { "unknown" } else { "failed" }, "error": error})
                })
        }).await.unwrap_or_else(|error| {
            tracing::error!(error = %error, "[artifact-notifications] delivery task failed");
            json!({"status": "unknown", "error": "CHANNEL_DELIVERY_UNKNOWN"})
        })
    }

    async fn deliver_artifacts_inner(
        &self,
        caller: ChannelCaller,
        mut request: ArtifactDeliveryRequest,
    ) -> Result<Value, String> {
        request.artifact_ids.sort_unstable();
        request.artifact_ids.dedup();
        if request.artifact_ids.is_empty() || request.artifact_ids.len() > MAX_ARTIFACTS {
            return Err("INVALID_ARTIFACT_IDS".into());
        }
        let digest =
            json!({"artifact_ids": request.artifact_ids, "channel_id": request.channel_id});
        let start = self
            .begin_mutation(&caller, DELIVERY_OPERATION, &request.request_id, digest)
            .await?;
        let model = match start {
            MutationStart::Return(value) => return Ok(value),
            MutationStart::Started(model) => model,
        };
        tracing::info!(
            artifact_count = request.artifact_ids.len(),
            channel_id = request.channel_id,
            "[artifact-notifications] delivery started"
        );
        let result = self
            .dispatch_artifacts(&caller, &request)
            .await
            .unwrap_or_else(|error| json!({"status": "failed", "error": error}));
        tracing::info!(
            status = result["status"].as_str().unwrap_or("unknown"),
            "[artifact-notifications] delivery completed"
        );
        self.finish_mutation(
            &caller,
            DELIVERY_OPERATION,
            &request.request_id,
            model,
            result,
            request.channel_id,
        )
        .await
    }

    async fn dispatch_artifacts(
        &self,
        caller: &ChannelCaller,
        request: &ArtifactDeliveryRequest,
    ) -> Result<Value, String> {
        let artifacts = task_artifact::Entity::find()
            .filter(task_artifact::Column::Id.is_in(request.artifact_ids.clone()))
            .order_by_asc(task_artifact::Column::Id)
            .all(&self.db.conn)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "[artifact-notifications] artifact query failed");
                "ARTIFACT_QUERY_FAILED"
            })?;
        if artifacts.len() != request.artifact_ids.len() {
            return Err("ARTIFACT_NOT_FOUND".into());
        }
        let targets = self
            .artifact_channel_targets()
            .await?
            .into_iter()
            .filter(|target| request.channel_id.is_none_or(|id| id == target.channel_id))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Ok(json!({"status": "skipped", "reason": "NO_ENABLED_CHANNELS", "items": []}));
        }
        let message =
            super::artifact_content::notification_message(&self.db, request.message.as_deref())
                .await?;
        let content = super::artifact_content::prepare(&artifacts, &message).await;
        let content = &content;
        let mut results = stream::iter(targets)
            .map(|target| async move {
                self.deliver_to_artifact_target(caller, request, (&target, content))
                    .await
            })
            .buffer_unordered(CHANNEL_SEND_WORKERS)
            .collect::<Vec<_>>()
            .await;
        results.sort_by_key(|result| result["channel_id"].as_i64());
        Ok(json!({"status": super::send_result::batch_status(&results),
            "delivery_unknown": results.iter().any(delivery_unknown), "items": results}))
    }

    async fn deliver_to_artifact_target(
        &self,
        caller: &ChannelCaller,
        request: &ArtifactDeliveryRequest,
        delivery: (
            &ArtifactChannelTarget,
            &super::artifact_content::ArtifactContent,
        ),
    ) -> Value {
        let (target, content) = delivery;
        let mut result = if let Some(error) = target.error {
            json!({"status": "failed", "error": error})
        } else {
            let items = super::artifact_content::send_items(target, content);
            self.send_messages(
                caller.clone(),
                Ok(SendMessagesInput {
                    request_id: format!(
                        "artifact-channel:{:x}",
                        Sha256::digest(format!("{}:{}", request.request_id, target.channel_id))
                    ),
                    items,
                }),
            )
            .await
            .unwrap_or_else(|error| json!({"status": "failed", "error": error}))
        };
        if result["status"] == "success" {
            result["status"] = json!("sent");
        }
        if !content.errors.is_empty() {
            result["artifact_errors"] = json!(content.errors);
            if result["status"] == "sent" {
                result["status"] = json!("partial_success");
            }
        }
        result["channel_id"] = json!(target.channel_id);
        result["channel_name"] = json!(target.name);
        result["target_name"] = json!(target.target_name);
        tracing::info!(
            channel_id = target.channel_id,
            status = result["status"].as_str().unwrap_or("unknown"),
            error = result["error"].as_str(),
            "[artifact-notifications] channel delivery completed"
        );
        result
    }
}

fn accepted_ids(registration: &Value) -> Vec<i32> {
    let mut ids = registration["accepted"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| i32::try_from(item["id"].as_i64()?).ok())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn delivery_unknown(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            (matches!(key.as_str(), "error" | "message_error" | "file_error")
                && value.as_str().is_some_and(|code| {
                    code.ends_with("_UNKNOWN") || code == "IDEMPOTENCY_UNAVAILABLE"
                }))
                || delivery_unknown(value)
        }),
        Value::Array(items) => items.iter().any(delivery_unknown),
        _ => false,
    }
}
