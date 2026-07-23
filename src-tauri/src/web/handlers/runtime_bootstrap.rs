use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::runtime_bootstrap as rb;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBootstrapParams {
    pub task_id: String,
}

pub async fn runtime_bootstrap(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RuntimeBootstrapParams>,
) -> Json<rb::RuntimeBootstrapReport> {
    let emitter = state.emitter.clone();
    let report = rb::runtime_bootstrap_core(params.task_id, &emitter).await;
    let conn = state.db.conn.clone();
    let data_dir = state.data_dir.clone();
    tokio::spawn(async move {
        crate::system_skills::startup_update_core(&conn, &data_dir, &emitter).await;
    });
    Json(report)
}
