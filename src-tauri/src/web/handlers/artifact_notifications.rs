use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;
use serde_json::Value;

use crate::acp::channel_tools::ArtifactChannelTarget;
use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::chat_channel::artifact_notification_policy::{self, ArtifactNotificationMode};
use crate::commands::artifact_notifications::{self, SendArtifactParams};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModeParams {
    pub channel_id: i32,
    pub mode: ArtifactNotificationMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelParams {
    pub channel_id: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendParams {
    pub params: SendArtifactParams,
}

pub async fn get_artifact_notification_mode(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ChannelParams>,
) -> Result<Json<ArtifactNotificationMode>, AppCommandError> {
    Ok(Json(
        artifact_notification_policy::get_mode(&state.db, params.channel_id).await?,
    ))
}

pub async fn set_artifact_notification_mode(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ModeParams>,
) -> Result<Json<()>, AppCommandError> {
    artifact_notification_policy::set_mode(&state.db, params.channel_id, params.mode).await?;
    Ok(Json(()))
}

pub async fn list_artifact_channel_targets(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<ArtifactChannelTarget>>, AppCommandError> {
    Ok(Json(
        artifact_notifications::list_artifact_channel_targets_core(
            &state.db,
            &state.chat_channel_manager,
        )
        .await?,
    ))
}

pub async fn send_artifact_to_channels(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<SendParams>,
) -> Result<Json<Value>, AppCommandError> {
    Ok(Json(
        artifact_notifications::send_artifact_to_channels_core(
            &state.db,
            &state.chat_channel_manager,
            body.params,
        )
        .await?,
    ))
}
