use std::sync::Arc;
use axum::{extract::Extension, Json};
use serde::Deserialize;
use serde_json::Value;
use crate::{app_error::AppCommandError, app_state::AppState};

#[derive(Deserialize)]
pub struct Params { action: String, data: Value }

pub async fn action(
    Extension(state): Extension<Arc<AppState>>, Json(params): Json<Params>,
) -> Result<Json<Value>, AppCommandError> {
    Ok(Json(crate::commands::conversation_share::action_core(
        &state.db.conn, params.action, params.data,
    ).await?))
}
