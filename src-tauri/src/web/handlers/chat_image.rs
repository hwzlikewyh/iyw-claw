use std::path::PathBuf;
use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::chat_image::{prepare_chat_image_core, PreparedChatImage};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareChatImageParams {
    pub path: String,
}

pub async fn prepare_chat_image(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<PrepareChatImageParams>,
) -> Result<Json<PreparedChatImage>, AppCommandError> {
    prepare_chat_image_core(&state.db.conn, PathBuf::from(params.path))
        .await
        .map(Json)
}
