use super::plugin_app_context::{load_workspace, PluginAppWorkspace};
use super::plugin_app_resource_meta;
use super::plugin_app_resources::{plugin_app_config, resource_html, AppConfig};
use super::plugin_app_tools;
use super::plugin_app_types::{PluginAppOpenRequest, PluginAppOpenResponse};
use crate::app_error::AppCommandError;

pub(crate) async fn open_core(
    db: &crate::db::AppDatabase,
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    router: &crate::plugin_runtime::router::PluginRouter,
    request: PluginAppOpenRequest,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    let prepared = prepare_open(db, &request).await?;
    let resource = read_app_resource(router, &prepared).await?;
    let html = resource_html(resource.0)?;
    renew_lease(apps, prepared, html, resource.1)
}

struct PreparedOpen {
    instance: crate::db::entities::plugin_app_instance::Model,
    plugin: crate::plugin_runtime::registry::PluginDescriptor,
    workspace: PluginAppWorkspace,
    config: AppConfig,
    payload: serde_json::Value,
    mode: String,
}

async fn prepare_open(
    db: &crate::db::AppDatabase,
    request: &PluginAppOpenRequest,
) -> Result<PreparedOpen, AppCommandError> {
    let (instance, plugin) = load_instance(db, request).await?;
    let workspace = load_workspace(db, instance.conversation_id, &instance.workspace_key).await?;
    let config = plugin_app_config(&plugin, &instance.app_key)?;
    let payload = serde_json::from_str(&instance.launch_payload_json)
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    let mode = display_mode(request, &payload, &config)?;
    Ok(PreparedOpen {
        instance,
        plugin,
        workspace,
        config,
        payload,
        mode,
    })
}

async fn load_instance(
    db: &crate::db::AppDatabase,
    request: &PluginAppOpenRequest,
) -> Result<
    (
        crate::db::entities::plugin_app_instance::Model,
        crate::plugin_runtime::registry::PluginDescriptor,
    ),
    AppCommandError,
> {
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
    plugin_app_tools::authorize_app_message(db, &instance.instance_id, "resources/read")
        .await
        .map_err(|error| AppCommandError::permission_denied(error.message))?;
    let plugin = crate::plugin_runtime::registry::global_snapshot()
        .and_then(|snapshot| snapshot.plugins.get(&instance.plugin_slug).cloned())
        .filter(|plugin| plugin.available && plugin.version == instance.plugin_version)
        .ok_or_else(|| {
            AppCommandError::configuration_missing("Plugin app version is unavailable")
        })?;
    Ok((instance, plugin))
}

fn display_mode(
    request: &PluginAppOpenRequest,
    payload: &serde_json::Value,
    config: &AppConfig,
) -> Result<String, AppCommandError> {
    let stored = payload["displayMode"]
        .as_str()
        .filter(|mode| config.display_modes.iter().any(|value| value == mode));
    let mode = request
        .display_mode
        .clone()
        .or_else(|| stored.map(str::to_string))
        .unwrap_or_else(|| "inline".into());
    if !config.display_modes.contains(&mode) {
        return Err(AppCommandError::invalid_input(
            "Plugin app display mode is not declared by the manifest",
        ));
    }
    Ok(mode)
}

async fn read_app_resource(
    router: &crate::plugin_runtime::router::PluginRouter,
    prepared: &PreparedOpen,
) -> Result<
    (
        rmcp::model::ReadResourceResult,
        Option<super::plugin_app_resource_meta::PluginAppResourceMeta>,
    ),
    AppCommandError,
> {
    let resource = router
        .read_app_resource(app_read_request(prepared))
        .await
        .map_err(|error| AppCommandError::configuration_missing(error.message))?;
    let metadata = plugin_app_resource_meta::from_resource(
        &resource,
        &prepared.plugin,
        &prepared.workspace.workspace_key,
    );
    Ok((resource, metadata))
}

fn app_read_request(prepared: &PreparedOpen) -> crate::plugin_runtime::types::PluginAppReadRequest {
    crate::plugin_runtime::types::PluginAppReadRequest {
        plugin_slug: prepared.instance.plugin_slug.clone(),
        plugin_version: prepared.instance.plugin_version.clone(),
        app_key: prepared.instance.app_key.clone(),
        workspace_key: prepared.workspace.workspace_key.clone(),
        workspace_dir: prepared.workspace.workspace_dir.clone(),
        agent_type: prepared.workspace.agent_type,
        permission_revision: prepared.plugin.permissions_digest.clone(),
        cancellation: tokio_util::sync::CancellationToken::new(),
        authority_cancellation: tokio_util::sync::CancellationToken::new(),
    }
}

fn renew_lease(
    apps: &crate::plugin_runtime::app_host::PluginAppRegistry,
    prepared: PreparedOpen,
    html: String,
    resource_meta: Option<super::plugin_app_resource_meta::PluginAppResourceMeta>,
) -> Result<PluginAppOpenResponse, AppCommandError> {
    let launch = apps
        .renew(crate::plugin_runtime::app_host::PluginAppLeaseInput {
            instance_id: prepared.instance.instance_id,
            conversation_id: prepared.instance.conversation_id,
            tool_call_id: prepared.instance.tool_call_id,
            plugin_slug: prepared.instance.plugin_slug,
            plugin_version: prepared.instance.plugin_version,
            app_key: prepared.instance.app_key,
            resource_uri: prepared.config.resource_uri,
            display_mode: prepared.mode,
            launch_payload: prepared.payload,
        })
        .map_err(|error| AppCommandError::configuration_missing(error.message))?;
    Ok(PluginAppOpenResponse {
        launch,
        html,
        host_version: env!("CARGO_PKG_VERSION").into(),
        resource_meta,
    })
}
