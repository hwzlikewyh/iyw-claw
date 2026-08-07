use std::path::PathBuf;

use axum::Json;
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::commands::chat_image::{prepare_chat_image_core, PreparedChatImage};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareChatImageParams {
    pub path: String,
}

pub async fn prepare_chat_image(
    Json(params): Json<PrepareChatImageParams>,
) -> Result<Json<PreparedChatImage>, AppCommandError> {
    prepare_chat_image_core(PathBuf::from(params.path))
        .await
        .map(Json)
}
