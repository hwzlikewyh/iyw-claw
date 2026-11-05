use super::plugin_app_tools::{find_instance, plugin_error};
use crate::app_error::AppCommandError;

pub(super) async fn route_app_resource(
    db: &crate::db::AppDatabase,
    router: &crate::plugin_runtime::router::PluginRouter,
    instance_id: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, crate::plugin_runtime::types::PluginInvokeError> {
    let instance = find_instance(db, instance_id).await?;
    let uri = params
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| plugin_error("plugin_app_invalid", "resources/read uri is missing"))?;
    let snapshot = crate::plugin_runtime::registry::global_snapshot()
        .ok_or_else(|| plugin_error("plugin_app_unavailable", "Plugin registry is unavailable"))?;
    let plugin = snapshot
        .plugins
        .get(&instance.plugin_slug)
        .filter(|plugin| plugin.available && plugin.version == instance.plugin_version)
        .ok_or_else(|| {
            plugin_error(
                "plugin_app_unavailable",
                "Plugin app version is unavailable",
            )
        })?;
    let app = plugin
        .manifest
        .components
        .iter()
        .find(|component| component.kind == "app" && component.key == instance.app_key)
        .and_then(|component| component.config.as_ref())
        .ok_or_else(|| plugin_error("plugin_app_unavailable", "Plugin app binding is missing"))?;
    validate_resource_binding(&plugin, app, uri)?;
    let workspace = app_workspace(db, &instance).await?;
    let request = resource_request(&instance, &plugin, workspace);
    let result = router
        .read_app_resource_uri(request, Some(uri.to_string()))
        .await?;
    serde_json::to_value(result)
        .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))
}

fn validate_resource_binding(
    plugin: &crate::plugin_runtime::registry::PluginDescriptor,
    app: &serde_json::Value,
    uri: &str,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    let native = crate::plugin_runtime::native_compat::connector(
        std::path::Path::new(&plugin.install_root),
        app["connectorKey"].as_str().unwrap_or_default(),
    )?;
    if app["resourceUri"].as_str() != Some(uri)
        && !native.is_some_and(|value| {
            value
                .resources
                .iter()
                .any(|item| item["uri"].as_str() == Some(uri))
        })
    {
        return Err(plugin_error(
            "plugin_app_unauthorized",
            "Plugin app resource is not declared by its binding",
        ));
    }
    Ok(())
}

async fn app_workspace(
    db: &crate::db::AppDatabase,
    instance: &crate::db::entities::plugin_app_instance::Model,
) -> Result<
    super::plugin_app_context::PluginAppWorkspace,
    crate::plugin_runtime::types::PluginInvokeError,
> {
    super::plugin_app_context::load_workspace(db, instance.conversation_id, &instance.workspace_key)
        .await
        .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))
}

fn resource_request(
    instance: &crate::db::entities::plugin_app_instance::Model,
    plugin: &crate::plugin_runtime::registry::PluginDescriptor,
    workspace: super::plugin_app_context::PluginAppWorkspace,
) -> crate::plugin_runtime::types::PluginAppReadRequest {
    crate::plugin_runtime::types::PluginAppReadRequest {
        plugin_slug: instance.plugin_slug.clone(),
        plugin_version: instance.plugin_version.clone(),
        app_key: instance.app_key.clone(),
        workspace_key: workspace.workspace_key,
        workspace_dir: workspace.workspace_dir,
        agent_type: workspace.agent_type,
        permission_revision: plugin.permissions_digest.clone(),
        cancellation: tokio_util::sync::CancellationToken::new(),
        authority_cancellation: tokio_util::sync::CancellationToken::new(),
    }
}

pub(super) async fn route_app_resources(
    db: &crate::db::AppDatabase,
    instance_id: &str,
) -> Result<serde_json::Value, crate::plugin_runtime::types::PluginInvokeError> {
    let instance = find_instance(db, instance_id).await?;
    let snapshot = crate::plugin_runtime::registry::global_snapshot()
        .ok_or_else(|| plugin_error("plugin_app_unavailable", "Plugin registry is unavailable"))?;
    let plugin = snapshot
        .plugins
        .get(&instance.plugin_slug)
        .filter(|plugin| plugin.available && plugin.version == instance.plugin_version)
        .ok_or_else(|| {
            plugin_error(
                "plugin_app_unavailable",
                "Plugin app version is unavailable",
            )
        })?;
    let app = plugin
        .manifest
        .components
        .iter()
        .find(|component| component.kind == "app" && component.key == instance.app_key)
        .and_then(|component| component.config.as_ref())
        .ok_or_else(|| plugin_error("plugin_app_unavailable", "Plugin app binding is missing"))?;
    let uri = app["resourceUri"].as_str().ok_or_else(|| {
        plugin_error(
            "plugin_app_unavailable",
            "Plugin app resource URI is missing",
        )
    })?;
    if let Some(native) = crate::plugin_runtime::native_compat::connector(
        std::path::Path::new(&plugin.install_root),
        app["connectorKey"].as_str().unwrap_or_default(),
    )? {
        return Ok(serde_json::json!({ "resources": native.resources }));
    }
    Ok(serde_json::json!({
        "resources": [{
            "uri": uri,
            "name": instance.app_key,
            "mimeType": "text/html;profile=mcp-app"
        }]
    }))
}

pub(super) struct AppConfig {
    pub resource_uri: String,
    pub display_modes: Vec<String>,
}

pub(super) fn plugin_app_config(
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

pub(super) fn resource_html(
    resource: rmcp::model::ReadResourceResult,
) -> Result<String, AppCommandError> {
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
