use std::sync::Arc;

use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::Value;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::mcp as mcp_commands;
use crate::commands::mcp::{
    LocalMcpServer, McpAppType, McpMarketplaceItem, McpMarketplaceProvider,
    McpMarketplaceServerDetail,
};

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMarketplaceParams {
    pub provider_id: String,
    pub query: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMarketplaceServerDetailParams {
    pub provider_id: String,
    pub server_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallFromMarketplaceParams {
    pub provider_id: String,
    pub server_id: String,
    pub spec_override: Option<Value>,
    pub option_id: Option<String>,
    pub protocol: Option<String>,
    pub parameter_values: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertLocalServerParams {
    pub server_id: String,
    pub spec: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetServerAppsParams {
    pub server_id: String,
    pub apps: Vec<McpAppType>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetServerEnabledParams {
    pub server_id: String,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveServerParams {
    pub server_id: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn mcp_scan_local(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<LocalMcpServer>>, AppCommandError> {
    let result = mcp_commands::mcp_scan_local_core(&state.db.conn).await?;
    Ok(Json(result))
}

pub async fn mcp_list_marketplaces() -> Result<Json<Vec<McpMarketplaceProvider>>, AppCommandError> {
    let result = mcp_commands::mcp_list_marketplaces().await?;
    Ok(Json(result))
}

pub async fn mcp_search_marketplace(
    Json(params): Json<SearchMarketplaceParams>,
) -> Result<Json<Vec<McpMarketplaceItem>>, AppCommandError> {
    let result =
        mcp_commands::mcp_search_marketplace(params.provider_id, params.query, params.limit)
            .await?;
    Ok(Json(result))
}

pub async fn mcp_get_marketplace_server_detail(
    Json(params): Json<GetMarketplaceServerDetailParams>,
) -> Result<Json<McpMarketplaceServerDetail>, AppCommandError> {
    let result =
        mcp_commands::mcp_get_marketplace_server_detail(params.provider_id, params.server_id)
            .await?;
    Ok(Json(result))
}

pub async fn mcp_install_from_marketplace(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<InstallFromMarketplaceParams>,
) -> Result<Json<LocalMcpServer>, AppCommandError> {
    let result = mcp_commands::mcp_install_from_marketplace_core(
        &state.db.conn,
        params.provider_id,
        params.server_id,
        params.spec_override,
        params.option_id,
        params.protocol,
        params.parameter_values,
    )
    .await?;
    Ok(Json(result))
}

pub async fn mcp_upsert_local_server(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<UpsertLocalServerParams>,
) -> Result<Json<LocalMcpServer>, AppCommandError> {
    let result =
        mcp_commands::mcp_upsert_local_server_core(&state.db.conn, params.server_id, params.spec)
            .await?;
    Ok(Json(result))
}

pub async fn mcp_set_server_enabled(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetServerEnabledParams>,
) -> Result<Json<Option<LocalMcpServer>>, AppCommandError> {
    let result =
        mcp_commands::mcp_set_server_enabled_core(&state.db.conn, params.server_id, params.enabled)
            .await?;
    Ok(Json(result))
}

pub async fn mcp_set_server_apps(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<SetServerAppsParams>,
) -> Result<Json<Option<LocalMcpServer>>, AppCommandError> {
    let result =
        mcp_commands::mcp_set_server_apps_core(&state.db.conn, params.server_id, params.apps)
            .await?;
    Ok(Json(result))
}

pub async fn mcp_remove_server(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RemoveServerParams>,
) -> Result<Json<bool>, AppCommandError> {
    let result = mcp_commands::mcp_remove_server_core(&state.db.conn, params.server_id).await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn install_payload_does_not_require_target_apps() {
        let params: InstallFromMarketplaceParams = serde_json::from_value(json!({
            "providerId": "official_registry",
            "serverId": "filesystem",
        }))
        .expect("global distribution must not require target apps");

        assert_eq!(params.server_id, "filesystem");
    }

    #[test]
    fn upsert_payload_does_not_require_target_apps() {
        let params: UpsertLocalServerParams = serde_json::from_value(json!({
            "serverId": "filesystem",
            "spec": { "type": "stdio", "command": "npx" },
        }))
        .expect("global distribution must not require target apps");

        assert_eq!(params.server_id, "filesystem");
    }
}
