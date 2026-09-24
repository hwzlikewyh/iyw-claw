use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use crate::acp::channel_tools::{
    ArtifactChannelTarget, ArtifactDeliveryRequest, ChannelCaller, ChannelToolService,
};
use crate::app_error::AppCommandError;
#[cfg(feature = "tauri-runtime")]
use crate::chat_channel::artifact_notification_policy::{self, ArtifactNotificationMode};
use crate::chat_channel::manager::ChatChannelManager;
use crate::db::service::task_artifact_service::management::{get_artifact, ArtifactIdentity};
use crate::db::AppDatabase;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendArtifactParams {
    pub artifact_id: i32,
    pub conversation_id: i32,
    pub channel_id: Option<i32>,
    pub request_id: String,
}

fn service(db: &AppDatabase, manager: &ChatChannelManager) -> Arc<ChannelToolService> {
    Arc::new(ChannelToolService::new(
        Arc::new(AppDatabase {
            conn: db.conn.clone(),
        }),
        manager.clone_ref(),
    ))
}

pub async fn list_artifact_channel_targets_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
) -> Result<Vec<ArtifactChannelTarget>, AppCommandError> {
    service(db, manager)
        .artifact_channel_targets()
        .await
        .map_err(AppCommandError::task_execution_failed)
}

pub async fn send_artifact_to_channels_core(
    db: &AppDatabase,
    manager: &ChatChannelManager,
    params: SendArtifactParams,
) -> Result<Value, AppCommandError> {
    let identity = ArtifactIdentity {
        conversation_id: params.conversation_id,
        artifact_id: params.artifact_id,
    };
    let artifact = get_artifact(&db.conn, identity, false)
        .await?
        .ok_or_else(|| AppCommandError::not_found("Artifact not found"))?;
    if artifact.status != "available" {
        return Err(AppCommandError::invalid_input("Artifact is unavailable"));
    }
    let caller = ChannelCaller {
        agent_type: "user".into(),
        session_ref: format!("conversation:{}", params.conversation_id),
        caller_scope: "artifact-manual".into(),
        working_dir: std::path::PathBuf::new(),
    };
    Ok(service(db, manager)
        .deliver_artifacts(
            caller,
            ArtifactDeliveryRequest {
                artifact_ids: vec![params.artifact_id],
                channel_id: params.channel_id,
                request_id: params.request_id,
                message: None,
            },
        )
        .await)
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn get_artifact_notification_mode(
    db: tauri::State<'_, AppDatabase>,
    channel_id: i32,
) -> Result<ArtifactNotificationMode, AppCommandError> {
    artifact_notification_policy::get_mode(&db, channel_id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn set_artifact_notification_mode(
    db: tauri::State<'_, AppDatabase>,
    channel_id: i32,
    mode: ArtifactNotificationMode,
) -> Result<(), AppCommandError> {
    artifact_notification_policy::set_mode(&db, channel_id, mode).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn list_artifact_channel_targets(
    db: tauri::State<'_, AppDatabase>,
    manager: tauri::State<'_, ChatChannelManager>,
) -> Result<Vec<ArtifactChannelTarget>, AppCommandError> {
    list_artifact_channel_targets_core(&db, &manager).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn send_artifact_to_channels(
    db: tauri::State<'_, AppDatabase>,
    manager: tauri::State<'_, ChatChannelManager>,
    params: SendArtifactParams,
) -> Result<Value, AppCommandError> {
    send_artifact_to_channels_core(&db, &manager, params).await
}
