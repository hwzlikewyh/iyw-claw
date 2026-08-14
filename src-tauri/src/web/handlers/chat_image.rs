use std::path::PathBuf;
use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::chat_image::{
    prepare_chat_image_core, PrepareChatImageRequest, PreparedChatImage,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareChatImageParams {
    pub path: String,
    pub chat_dir: Option<String>,
    pub session_id: Option<String>,
}

pub async fn prepare_chat_image(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<PrepareChatImageParams>,
) -> Result<Json<PreparedChatImage>, AppCommandError> {
    prepare_chat_image_core(
        &state.db.conn,
        PrepareChatImageRequest {
            path: PathBuf::from(params.path),
            data_dir: state.data_dir.clone(),
            chat_dir: params.chat_dir.map(PathBuf::from),
            session_id: params.session_id,
            display_name: None,
        },
    )
    .await
    .map(Json)
}
