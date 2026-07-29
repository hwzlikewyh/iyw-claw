use std::sync::Arc;

use axum::{extract::Extension, Json};

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::agent_version_center::{
    self, AgentHistoryRequest, AgentPinRequest, ToolHistoryRequest, ToolInstallRequest,
    ToolPinRequest,
};

pub async fn snapshot(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<agent_version_center::AgentVersionCenterSnapshot>, AppCommandError> {
    Ok(Json(
        agent_version_center::snapshot_core(&state.db.conn, &state.agent_catalog).await?,
    ))
}

pub async fn refresh(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<agent_version_center::AgentVersionCenterSnapshot>, AppCommandError> {
    Ok(Json(
        agent_version_center::refresh_core(&state.db.conn, &state.agent_catalog).await?,
    ))
}

pub async fn agent_history(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<AgentHistoryRequest>,
) -> Result<Json<crate::acp::version_center::VersionHistory>, AppCommandError> {
    Ok(Json(
        agent_version_center::agent_history_core(
            &state.db.conn,
            request.agent_type,
            request.channel,
        )
        .await?,
    ))
}

pub async fn tool_history(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<ToolHistoryRequest>,
) -> Result<Json<crate::acp::version_center::VersionHistory>, AppCommandError> {
    Ok(Json(
        agent_version_center::tool_history_core(&state.db.conn, request.tool_id, request.channel)
            .await?,
    ))
}

pub async fn set_agent_pin(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<AgentPinRequest>,
) -> Result<Json<()>, AppCommandError> {
    agent_version_center::set_agent_pin_core(
        &state.db.conn,
        request.agent_type,
        request.version,
        request.channel,
    )
    .await?;
    Ok(Json(()))
}

pub async fn set_tool_pin(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<ToolPinRequest>,
) -> Result<Json<()>, AppCommandError> {
    agent_version_center::set_tool_pin_core(
        &state.db.conn,
        request.tool_id,
        request.version,
        request.channel,
    )
    .await?;
    Ok(Json(()))
}

pub async fn install_tool(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<ToolInstallRequest>,
) -> Result<Json<crate::acp::version_center::ManagedToolInstallResult>, AppCommandError> {
    Ok(Json(
        agent_version_center::install_tool_core(
            &state.db.conn,
            &state.connection_manager,
            &state.data_dir,
            request.tool_id,
            request.version,
            request.channel,
        )
        .await?,
    ))
}
