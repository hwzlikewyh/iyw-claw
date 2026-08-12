use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::acp::{AgentInputItem, AgentInputPayload};
use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::agent_input::{
    delete_agent_input_core, force_agent_inputs_through_core, list_agent_inputs_core,
    reorder_agent_inputs_core, resume_agent_inputs_core, retry_agent_input_core,
    submit_agent_input_core,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitAgentInputParams {
    pub connection_id: String,
    pub conversation_id: i32,
    pub message_id: String,
    pub payload: AgentInputPayload,
}

pub async fn submit_agent_input(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SubmitAgentInputParams>,
) -> Result<Json<AgentInputItem>, AppCommandError> {
    let item = submit_agent_input_core(
        &state.db,
        &state.connection_manager,
        params.connection_id,
        params.conversation_id,
        params.message_id,
        params.payload,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(item))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAgentInputsParams {
    pub conversation_id: i32,
}

pub async fn list_agent_inputs(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ListAgentInputsParams>,
) -> Result<Json<Vec<AgentInputItem>>, AppCommandError> {
    Ok(Json(
        list_agent_inputs_core(&state.db.conn, params.conversation_id).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutateAgentInputParams {
    pub connection_id: String,
    pub conversation_id: i32,
    pub message_id: String,
}

pub async fn delete_agent_input(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<MutateAgentInputParams>,
) -> Result<Json<AgentInputItem>, AppCommandError> {
    let item = delete_agent_input_core(
        &state.db,
        &state.connection_manager,
        params.connection_id,
        params.conversation_id,
        params.message_id,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(item))
}

pub async fn retry_agent_input(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<MutateAgentInputParams>,
) -> Result<Json<AgentInputItem>, AppCommandError> {
    let item = retry_agent_input_core(
        &state.db,
        &state.connection_manager,
        params.connection_id,
        params.conversation_id,
        params.message_id,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(item))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderAgentInputsParams {
    pub connection_id: String,
    pub conversation_id: i32,
    pub ordered_ids: Vec<String>,
}

pub async fn reorder_agent_inputs(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ReorderAgentInputsParams>,
) -> Result<Json<Vec<AgentInputItem>>, AppCommandError> {
    let items = reorder_agent_inputs_core(
        &state.db,
        &state.connection_manager,
        params.connection_id,
        params.conversation_id,
        params.ordered_ids,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(items))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceAgentInputsParams {
    pub connection_id: String,
    pub conversation_id: i32,
    pub message_id: String,
    pub expected_prefix_ids: Vec<String>,
}

pub async fn force_agent_inputs_through(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ForceAgentInputsParams>,
) -> Result<Json<Vec<AgentInputItem>>, AppCommandError> {
    let items = force_agent_inputs_through_core(
        &state.db,
        &state.connection_manager,
        params.connection_id,
        params.conversation_id,
        params.message_id,
        params.expected_prefix_ids,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(items))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeAgentInputsParams {
    pub connection_id: String,
    pub conversation_id: i32,
}

pub async fn resume_agent_inputs(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<ResumeAgentInputsParams>,
) -> Result<Json<()>, AppCommandError> {
    resume_agent_inputs_core(
        &state.db,
        &state.connection_manager,
        params.connection_id,
        params.conversation_id,
    )
    .await
    .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?;
    Ok(Json(()))
}
