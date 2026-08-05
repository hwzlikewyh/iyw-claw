use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;

use crate::acp::version_center::{
    bootstrap_init_status as vc_bootstrap_init_status,
    bootstrap_initialize as vc_bootstrap_initialize, InitStatusReport,
};
use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::runtime_bootstrap as rb;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBootstrapParams {
    pub task_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapInitializeParams {
    pub task_id: String,
    pub channel: Option<String>,
}

pub async fn runtime_bootstrap(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RuntimeBootstrapParams>,
) -> Json<rb::RuntimeBootstrapReport> {
    let emitter = state.emitter.clone();
    let defer_while_active = state.connection_manager.has_live_agent_sessions().await;
    let report = rb::runtime_bootstrap_managed_core(
        &state.db.conn,
        &state.data_dir,
        defer_while_active,
        params.task_id,
        &emitter,
    )
    .await;
    let conn = state.db.conn.clone();
    let data_dir = state.data_dir.clone();
    tokio::spawn(async move {
        crate::system_skills::startup_update_core(&conn, &data_dir, &emitter).await;
    });
    Json(report)
}

/// 受管初始化状态查询（只读，不取写入锁）；对应 Tauri command bootstrap_init_status。
pub async fn bootstrap_init_status(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<InitStatusReport>, AppCommandError> {
    Ok(Json(vc_bootstrap_init_status(&state.data_dir).await?))
}

/// 统一初始化 / 修复入口；channel 缺省时按更新偏好读取，读不到则 stable。
pub async fn bootstrap_initialize(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<BootstrapInitializeParams>,
) -> Result<Json<InitStatusReport>, AppCommandError> {
    let channel = match params.channel {
        Some(channel) if !channel.trim().is_empty() => channel,
        _ => crate::update::preferences::load(&state.db.conn)
            .await
            .map(|prefs| prefs.channel.as_str().to_string())
            .unwrap_or_else(|_| "stable".to_string()),
    };
    let emitter = state.emitter.clone();
    // IR-005：活跃会话存在时不切换组件版本，延迟激活由首启消费。
    let defer_while_active = state.connection_manager.has_live_agent_sessions().await;
    let report = vc_bootstrap_initialize(
        &state.db.conn,
        &state.data_dir,
        &channel,
        defer_while_active,
        &params.task_id,
        &emitter,
    )
    .await?;
    Ok(Json(report))
}
