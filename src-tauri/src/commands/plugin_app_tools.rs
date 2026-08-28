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
    let conversation = crate::db::service::conversation_service::get_by_id(
        &db.conn,
        instance.conversation_id as i32,
    )
    .await
    .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))?;
    let folder =
        crate::db::service::folder_service::get_folder_by_id(&db.conn, conversation.folder_id)
            .await
            .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))?
            .ok_or_else(|| {
                plugin_error(
                    "plugin_app_unavailable",
                    "Plugin app workspace was not found",
                )
            })?;
    let workspace_key = crate::commands::skill_inventory::workspace_key(Some(&folder.path));
    if workspace_key != instance.workspace_key {
        return Err(plugin_error(
            "plugin_app_unauthorized",
            "Plugin app workspace no longer matches its conversation",
        ));
    }
    router
        .invoke_app_tool(crate::plugin_runtime::types::PluginAppToolCall {
            plugin_slug: instance.plugin_slug,
            plugin_version: instance.plugin_version,
            app_key: instance.app_key,
            tool_name: name.to_string(),
            arguments,
            workspace_key,
            workspace_dir: std::path::PathBuf::from(folder.path),
            agent_type: conversation.agent_type,
        })
        .await
        .map(|result| serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
}

async fn find_instance(
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
    if app["resourceUri"].as_str() != Some(uri) {
        return Err(plugin_error(
            "plugin_app_unauthorized",
            "Plugin app resource is not declared by its binding",
        ));
    }
    let conversation = crate::db::service::conversation_service::get_by_id(
        &db.conn,
        instance.conversation_id as i32,
    )
    .await
    .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))?;
    let folder =
        crate::db::service::folder_service::get_folder_by_id(&db.conn, conversation.folder_id)
            .await
            .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))?
            .ok_or_else(|| {
                plugin_error(
                    "plugin_app_unavailable",
                    "Plugin app workspace was not found",
                )
            })?;
    let workspace_key = crate::commands::skill_inventory::workspace_key(Some(&folder.path));
    if workspace_key != instance.workspace_key {
        return Err(plugin_error(
            "plugin_app_unauthorized",
            "Plugin app workspace no longer matches its conversation",
        ));
    }
    let result = router
        .read_app_resource(crate::plugin_runtime::types::PluginAppReadRequest {
            plugin_slug: instance.plugin_slug,
            plugin_version: instance.plugin_version,
            app_key: instance.app_key,
            workspace_key,
            workspace_dir: std::path::PathBuf::from(folder.path),
            agent_type: conversation.agent_type,
            permission_revision: plugin.permissions_digest.clone(),
            cancellation: tokio_util::sync::CancellationToken::new(),
            authority_cancellation: tokio_util::sync::CancellationToken::new(),
        })
        .await?;
    serde_json::to_value(result)
        .map_err(|error| plugin_error("plugin_app_unavailable", error.to_string()))
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
    Ok(serde_json::json!({
        "resources": [{
            "uri": uri,
            "name": instance.app_key,
            "mimeType": "text/html;profile=mcp-app"
        }]
    }))
}

pub(super) async fn send_app_message(
    db: &crate::db::AppDatabase,
    manager: &crate::acp::manager::ConnectionManager,
    instance_id: &str,
    params: serde_json::Value,
) -> Result<(), crate::plugin_runtime::types::PluginInvokeError> {
    let instance = find_instance(db, instance_id).await?;
    let role = params.get("role").and_then(serde_json::Value::as_str);
    if role != Some("user") {
        return Err(plugin_error(
            "plugin_app_invalid",
            "ui/message only accepts user messages",
        ));
    }
    let content = params
        .get("content")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| plugin_error("plugin_app_invalid", "ui/message content is invalid"))?;
    let mut blocks = Vec::new();
    let mut display_text = String::new();
    for item in content {
        let kind = item.get("type").and_then(serde_json::Value::as_str);
        match kind {
            Some("text") => {
                let text = item
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        plugin_error("plugin_app_invalid", "ui/message text is invalid")
                    })?;
                if text.trim().is_empty() {
                    continue;
                }
                if !display_text.is_empty() {
                    display_text.push('\n');
                }
                display_text.push_str(text);
                blocks.push(crate::acp::types::PromptInputBlock::Text {
                    text: text.to_string(),
                });
            }
            Some("image") => {
                let data = item
                    .get("data")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let mime_type = item
                    .get("mimeType")
                    .or_else(|| item.get("mime_type"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if data.is_empty() || mime_type.is_empty() {
                    return Err(plugin_error(
                        "plugin_app_invalid",
                        "ui/message image is invalid",
                    ));
                }
                blocks.push(crate::acp::types::PromptInputBlock::Image {
                    data: data.to_string(),
                    mime_type: mime_type.to_string(),
                    uri: item
                        .get("uri")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    local_path: None,
                });
            }
            _ => {
                return Err(plugin_error(
                    "plugin_app_invalid",
                    "ui/message content block is unsupported",
                ));
            }
        }
    }
    if blocks.is_empty() {
        return Err(plugin_error(
            "plugin_app_invalid",
            "ui/message content is empty",
        ));
    }
    crate::commands::agent_input::queue_agent_input_core(
        db,
        manager,
        instance.conversation_id as i32,
        format!("plugin-app-{}", uuid::Uuid::new_v4()),
        crate::acp::AgentInputPayload {
            blocks,
            display_text,
            mode_id: None,
        },
    )
    .await
    .map(|_| ())
    .map_err(|error| plugin_error("plugin_app_message_failed", error.to_string()))
}

fn plugin_error(
    code: &'static str,
    message: impl Into<String>,
) -> crate::plugin_runtime::types::PluginInvokeError {
    crate::plugin_runtime::types::PluginInvokeError::before_effect(code, message)
}
