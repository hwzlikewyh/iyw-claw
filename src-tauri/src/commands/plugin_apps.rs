use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

#[path = "plugin_app_resource_meta.rs"]
mod plugin_app_resource_meta;
#[path = "plugin_app_tools.rs"]
mod plugin_app_tools;
pub use plugin_app_resource_meta::{
    PluginAppResourceCsp, PluginAppResourceMeta, PluginAppResourcePermissions,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppOpenRequest {
    pub instance_id: String,
    pub conversation_id: i64,
    #[serde(default)]
    pub display_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppMessageRequest {
    pub instance_id: String,
    pub lease_token: String,
    pub nonce: String,
    pub method: String,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppTeardownRequest {
    pub instance_id: String,
    #[serde(default)]
    pub conversation_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppOpenResponse {
    pub launch: crate::plugin_runtime::app_host::PluginAppLaunch,
    pub html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_meta: Option<PluginAppResourceMeta>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppErrorState {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppMessageResponse {
    pub accepted: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<PluginAppRpcError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppRpcError {
    pub code: i32,
    pub message: String,
}

pub async fn open_core(
    db: &crate::db::AppDatabase,
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    router: &crate::plugin_runtime::router::PluginRouter,
    request: PluginAppOpenRequest,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    let instance =
        crate::db::service::plugin_app_instance_service::find(&db.conn, &request.instance_id)
            .await
            .map_err(AppCommandError::db)?
            .ok_or_else(|| AppCommandError::not_found("Plugin app instance was not found"))?;
    if instance.conversation_id != request.conversation_id || instance.state != "active" {
        return Err(AppCommandError::permission_denied(
            "Plugin app instance is not active for this conversation",
        ));
    }
    let plugin = crate::plugin_runtime::registry::global_snapshot()
        .and_then(|snapshot| snapshot.plugins.get(&instance.plugin_slug).cloned())
        .filter(|plugin| plugin.available)
        .ok_or_else(|| AppCommandError::configuration_missing("Plugin is unavailable"))?;
    if plugin.version != instance.plugin_version {
        return Err(AppCommandError::configuration_missing(
            "Plugin app version is no longer available",
        ));
    }
    plugin_app_tools::authorize_app_message(&db, &instance.instance_id, "resources/read")
        .await
        .map_err(|error| AppCommandError::permission_denied(error.message))?;
    let conversation = crate::db::service::conversation_service::get_by_id(
        &db.conn,
        request.conversation_id as i32,
    )
    .await
    .map_err(AppCommandError::from)?;
    let folder =
        crate::db::service::folder_service::get_folder_by_id(&db.conn, conversation.folder_id)
            .await
            .map_err(AppCommandError::from)?
            .ok_or_else(|| AppCommandError::not_found("Plugin app workspace was not found"))?;
    let agent_type = conversation.agent_type;
    let workspace_key = crate::commands::skill_inventory::workspace_key(Some(&folder.path));
    if workspace_key != instance.workspace_key {
        return Err(AppCommandError::permission_denied(
            "Plugin app workspace no longer matches its conversation",
        ));
    }
    let app_config = plugin_app_config(&plugin, &instance.app_key)?;
    let permission_revision = plugin.permissions_digest.clone();
    let payload = serde_json::from_str::<serde_json::Value>(&instance.launch_payload_json)
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    let stored_mode = payload
        .get("displayMode")
        .and_then(serde_json::Value::as_str)
        .filter(|mode| app_config.display_modes.iter().any(|value| value == mode));
    let mode = request
        .display_mode
        .or_else(|| stored_mode.map(str::to_string))
        .unwrap_or_else(|| "inline".to_string());
    if !app_config.display_modes.iter().any(|value| value == &mode) {
        return Err(AppCommandError::invalid_input(
            "Plugin app display mode is not declared by the manifest",
        ));
    }
    let resource = router
        .read_app_resource(crate::plugin_runtime::types::PluginAppReadRequest {
            plugin_slug: instance.plugin_slug.clone(),
            plugin_version: instance.plugin_version.clone(),
            app_key: instance.app_key.clone(),
            workspace_key: workspace_key.clone(),
            workspace_dir: PathBuf::from(&folder.path),
            agent_type,
            permission_revision: permission_revision.clone(),
            cancellation: tokio_util::sync::CancellationToken::new(),
            authority_cancellation: tokio_util::sync::CancellationToken::new(),
        })
        .await
        .map_err(|error| AppCommandError::configuration_missing(error.message))?;
    let resource_meta = plugin_app_resource_meta::from_resource(&resource, &plugin, &workspace_key);
    let html = resource_html(resource)?;
    let launch = apps
        .renew(crate::plugin_runtime::app_host::PluginAppLeaseInput {
            instance_id: instance.instance_id,
            conversation_id: instance.conversation_id,
            tool_call_id: instance.tool_call_id,
            plugin_slug: instance.plugin_slug,
            plugin_version: instance.plugin_version,
            app_key: instance.app_key,
            resource_uri: app_config.resource_uri,
            display_mode: mode,
            launch_payload: payload,
        })
        .map_err(|error| AppCommandError::configuration_missing(error.message))?;
    Ok(PluginAppOpenResponse {
        launch,
        html,
        resource_meta,
    })
}

pub async fn message_core(
    db: &crate::db::AppDatabase,
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    router: &crate::plugin_runtime::router::PluginRouter,
    manager: &crate::acp::manager::ConnectionManager,
    request: PluginAppMessageRequest,
) -> Result<PluginAppMessageResponse, AppCommandError> {
    let params = request.params.unwrap_or(serde_json::Value::Null);
    let bytes = serde_json::to_vec(&params)
        .map_err(|error| AppCommandError::invalid_input(error.to_string()))?
        .len();
    if let Err(error) = apps.authorize_message(
        &request.instance_id,
        &request.lease_token,
        &request.nonce,
        &request.method,
        bytes,
    ) {
        return Ok(plugin_error_response(error));
    }
    if let Err(error) =
        plugin_app_tools::authorize_app_message(db, &request.instance_id, &request.method).await
    {
        return Ok(plugin_error_response(error));
    }
    let result = if request.method == "tools/call" {
        match plugin_app_tools::route_app_tool(db, router, &request.instance_id, params).await {
            Ok(result) => Some(result),
            Err(error) => return Ok(plugin_error_response(error)),
        }
    } else if request.method == "resources/list" {
        match plugin_app_tools::route_app_resources(db, &request.instance_id).await {
            Ok(result) => Some(result),
            Err(error) => return Ok(plugin_error_response(error)),
        }
    } else if request.method == "resources/read" {
        match plugin_app_tools::route_app_resource(db, router, &request.instance_id, params).await {
            Ok(result) => Some(result),
            Err(error) => return Ok(plugin_error_response(error)),
        }
    } else if request.method == "ui/message" {
        if let Err(error) =
            plugin_app_tools::send_app_message(db, manager, &request.instance_id, params).await
        {
            return Ok(plugin_error_response(error));
        }
        Some(serde_json::json!({}))
    } else {
        None
    };
    Ok(PluginAppMessageResponse {
        accepted: true,
        result,
        error: None,
    })
}

fn plugin_error_response(
    error: crate::plugin_runtime::types::PluginInvokeError,
) -> PluginAppMessageResponse {
    let code = if error.code.contains("invalid") {
        -32602
    } else if error.code.contains("unsupported") {
        -32601
    } else {
        -32000
    };
    PluginAppMessageResponse {
        accepted: false,
        result: None,
        error: Some(PluginAppRpcError {
            code,
            message: error.message,
        }),
    }
}

pub async fn teardown_core(
    db: &crate::db::AppDatabase,
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    request: PluginAppTeardownRequest,
) -> Result<(), AppCommandError> {
    apps.teardown(&request.instance_id);
    crate::db::service::plugin_app_instance_service::mark_inactive(
        &db.conn,
        &request.instance_id,
        request.conversation_id,
    )
    .await
    .map_err(AppCommandError::db)?;
    Ok(())
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_open(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    router: tauri::State<'_, crate::plugin_runtime::router::PluginRouter>,
    request: PluginAppOpenRequest,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    open_core(&db, &apps, &router, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_message(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    router: tauri::State<'_, crate::plugin_runtime::router::PluginRouter>,
    manager: tauri::State<'_, crate::acp::manager::ConnectionManager>,
    request: PluginAppMessageRequest,
) -> Result<PluginAppMessageResponse, AppCommandError> {
    message_core(&db, &apps, &router, &manager, request).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_app_teardown(
    db: tauri::State<'_, crate::db::AppDatabase>,
    apps: tauri::State<'_, crate::plugin_runtime::app_host::PluginAppRegistry>,
    request: PluginAppTeardownRequest,
) -> Result<(), AppCommandError> {
    teardown_core(&db, &apps, request).await
}

struct AppConfig {
    resource_uri: String,
    display_modes: Vec<String>,
}

fn plugin_app_config(
    plugin: &crate::plugin_runtime::registry::PluginDescriptor,
    app_key: &str,
) -> Result<AppConfig, AppCommandError> {
    let config = plugin
        .manifest
        .components
        .iter()
        .find(|component| component.kind == "app" && component.key == app_key)
        .and_then(|component| component.config.as_ref())
        .ok_or_else(|| AppCommandError::configuration_invalid("Plugin app binding is missing"))?;
    let resource_uri = config["resourceUri"]
        .as_str()
        .filter(|value| value.starts_with("ui://"))
        .ok_or_else(|| AppCommandError::configuration_invalid("Plugin app resource is invalid"))?;
    let display_modes = config["displayModes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    Ok(AppConfig {
        resource_uri: resource_uri.to_string(),
        display_modes,
    })
}

fn resource_html(resource: rmcp::model::ReadResourceResult) -> Result<String, AppCommandError> {
    resource
        .contents
        .into_iter()
        .find_map(|content| match content {
            rmcp::model::ResourceContents::TextResourceContents { text, .. } => Some(text),
            rmcp::model::ResourceContents::BlobResourceContents { blob, .. } => {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, blob)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            }
        })
        .filter(|html| !html.trim().is_empty() && html.len() <= 8 * 1024 * 1024)
        .ok_or_else(|| AppCommandError::configuration_invalid("Plugin app resource is empty"))
}
