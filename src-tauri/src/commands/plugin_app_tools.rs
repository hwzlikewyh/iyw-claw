pub(super) async fn authorize_app_message(
    db: &crate::db::AppDatabase,
    instance_id: &str,
    method: &str,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
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
    let permission_revision =
        serde_json::from_str::<serde_json::Value>(&instance.launch_payload_json)
            .ok()
            .and_then(|value| value["permissionRevision"].as_str().map(str::to_string));
    if permission_revision.as_deref() != Some(plugin.permissions_digest.as_str()) {
        return Err(plugin_error(
            "plugin_app_permission_changed",
            "Plugin permissions changed",
        ));
    }
    let granted = plugin.permission_grants.iter().any(|grant| {
        grant.permissions_digest == plugin.permissions_digest
            && grant.grant_state == "granted"
            && (grant.scope == "global" || grant.workspace_key == instance.workspace_key)
    });
    if !granted {
        return Err(plugin_error(
            "plugin_app_unauthorized",
            "Plugin permissions are not granted",
        ));
    }
    authorize_method(plugin, method)
}

pub(super) async fn route_app_tool(
    db: &crate::db::AppDatabase,
    router: &crate::plugin_runtime::router::PluginRouter,
    instance_id: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, crate::plugin_runtime::types::PluginInvokeError> {
    let instance = find_instance(db, instance_id).await?;
    let name = params
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| plugin_error("plugin_app_invalid", "tools/call name is missing"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}))
        .as_object()
        .cloned()
        .ok_or_else(|| {
            plugin_error(
                "plugin_app_invalid",
                "tools/call arguments must be an object",
            )
        })?;
    let workspace = super::plugin_app_context::load_workspace(
        db,
        instance.conversation_id,
        &instance.workspace_key,
    )
    .await
    .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))?;
    router
        .invoke_app_tool(crate::plugin_runtime::types::PluginAppToolCall {
            plugin_slug: instance.plugin_slug,
            plugin_version: instance.plugin_version,
            app_key: instance.app_key,
            tool_name: name.to_string(),
            arguments,
            workspace_key: workspace.workspace_key,
            workspace_dir: workspace.workspace_dir,
            agent_type: workspace.agent_type,
        })
        .await
        .map(|result| serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
}

pub(super) async fn find_instance(
    db: &crate::db::AppDatabase,
    instance_id: &str,
) -> Result<
    crate::db::entities::plugin_app_instance::Model,
    crate::plugin_runtime::types::PluginInvokeError,
> {
    crate::db::service::plugin_app_instance_service::find(&db.conn, instance_id)
        .await
        .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))?
        .filter(|instance| instance.state == "active")
        .ok_or_else(|| {
            plugin_error(
                "plugin_app_unavailable",
                "Plugin app instance was not found",
            )
        })
}

fn authorize_method(
    plugin: &crate::plugin_runtime::registry::PluginDescriptor,
    method: &str,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    let required = match method {
        "ui/open-link" => Some("open-link"),
        "ui/message" => Some("send-message"),
        "ui/update-model-context" => {
            return Err(plugin_error(
                "plugin_app_method_unsupported",
                "Plugin app method is not supported by this host",
            ));
        }
        _ => None,
    };
    if required.is_some_and(|permission| {
        plugin
            .manifest
            .permissions
            .as_ref()
            .is_none_or(|permissions| !permissions.host.iter().any(|item| item == permission))
    }) {
        return Err(plugin_error(
            "plugin_app_unauthorized",
            "Plugin app host permission is not granted",
        ));
    }
    Ok(())
}

pub(super) fn plugin_error(
    code: &'static str,
    message: impl Into<String>,
) -> crate::plugin_runtime::types::PluginInvokeError {
    crate::plugin_runtime::types::PluginInvokeError::before_effect(code, message)
}
