use crate::app_error::AppCommandError;
use crate::db::entities::plugin_app_instance;
use crate::plugin_runtime::{app_host::PluginAppRegistry, policy, registry};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRequest {
    pub instance_id: String,
    pub conversation_id: i64,
    pub lease_token: String,
    pub nonce: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncUpdate {
    pub cursor: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
}

const PAYLOAD_HEADROOM: usize = 1024;
type Recipient = (plugin_app_instance::Model, registry::PluginDescriptor);

pub async fn sync_core(
    db: &crate::db::AppDatabase,
    apps: &PluginAppRegistry,
    request: SyncRequest,
) -> Result<Option<SyncUpdate>, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let scope = crate::commands::mcp_cli_config::account_scope(&db.conn).await?;
    registry::reconcile_global(&db.conn).await?;
    let (recipient, plugin) = authorize(db, apps, &request).await?;
    let keys = matching_apps(&plugin, &recipient.app_key)?;
    let source = crate::db::service::plugin_app_instance_service::find_related_result(
        &db.conn,
        (&recipient, &keys),
    )
    .await
    .map_err(AppCommandError::db)?;
    let update = source
        .map(|source| update(&source, &plugin.permissions_digest))
        .transpose()?
        .flatten();
    let (_, current) = authorize(db, apps, &request).await?;
    if current.version != plugin.version || current.permissions_digest != plugin.permissions_digest
    {
        return Err(AppCommandError::permission_denied(
            "插件安装或权限已变化，请重新打开页面",
        ));
    }
    if crate::commands::mcp_cli_config::account_scope(&db.conn).await? != scope {
        return Err(AppCommandError::permission_denied(
            "账号范围已变化，请重新打开插件",
        ));
    }
    Ok(update)
}

async fn authorize(
    db: &crate::db::AppDatabase,
    apps: &PluginAppRegistry,
    request: &SyncRequest,
) -> Result<Recipient, AppCommandError> {
    apps.authorize_message(
        &request.instance_id,
        &request.lease_token,
        &request.nonce,
        "ping",
        0,
    )
    .map_err(|error| AppCommandError::permission_denied(error.message))?;
    let (instance, plugin) = super::plugin_app_open::load_instance(
        db,
        &super::PluginAppOpenRequest {
            instance_id: request.instance_id.clone(),
            conversation_id: request.conversation_id,
            display_mode: None,
        },
    )
    .await?;
    validate_scope(db, &instance, &plugin).await?;
    Ok((instance, plugin))
}

async fn validate_scope(
    db: &crate::db::AppDatabase,
    instance: &plugin_app_instance::Model,
    plugin: &registry::PluginDescriptor,
) -> Result<(), AppCommandError> {
    let workspace = super::plugin_app_context::load_workspace(
        db,
        instance.conversation_id,
        &instance.workspace_key,
    )
    .await?;
    let snapshot = registry::global_snapshot()
        .ok_or_else(|| AppCommandError::configuration_missing("插件目录未初始化"))?;
    crate::plugin_runtime::dependency_scope::require(
        plugin,
        &snapshot.plugins,
        (
            &workspace.workspace_key,
            workspace.agent_type.as_wire().as_ref(),
        ),
    )
    .map_err(|error| AppCommandError::permission_denied(error.message))?;
    let config = plugin
        .manifest
        .components
        .iter()
        .find(|app| app.kind == "app" && app.key == instance.app_key)
        .and_then(|app| app.config.as_ref())
        .ok_or_else(|| AppCommandError::configuration_missing("插件页面绑定不可用"))?;
    if !policy::activation_enabled(
        plugin,
        (
            config["connectorKey"].as_str().unwrap_or_default(),
            &workspace.workspace_key,
            workspace.agent_type.as_wire().as_ref(),
        ),
    ) {
        return Err(AppCommandError::permission_denied("当前项目已停用该插件"));
    }
    Ok(())
}

fn matching_apps(
    plugin: &registry::PluginDescriptor,
    app_key: &str,
) -> Result<Vec<String>, AppCommandError> {
    let binding = |key: &str| {
        plugin
            .manifest
            .components
            .iter()
            .find(|app| app.kind == "app" && app.key == key)
            .and_then(|app| app.config.as_ref())
    };
    let config = binding(app_key)
        .ok_or_else(|| AppCommandError::configuration_missing("插件页面绑定不可用"))?;
    Ok(plugin
        .manifest
        .components
        .iter()
        .filter(|app| app.kind == "app")
        .filter_map(|app| {
            let candidate = app.config.as_ref()?;
            (candidate["resourceUri"] == config["resourceUri"]
                && candidate["connectorKey"] == config["connectorKey"])
                .then(|| app.key.clone())
        })
        .collect())
}

fn update(
    source: &plugin_app_instance::Model,
    revision: &str,
) -> Result<Option<SyncUpdate>, AppCommandError> {
    if source.launch_payload_json.len()
        > crate::plugin_runtime::app_host::MAX_MESSAGE_BYTES - PAYLOAD_HEADROOM
    {
        return Err(AppCommandError::configuration_invalid(
            "插件结果过大，无法同步到页面",
        ));
    }
    let payload: serde_json::Value = serde_json::from_str(&source.launch_payload_json)
        .map_err(|_| AppCommandError::configuration_invalid("插件结果记录无效"))?;
    if payload["permissionRevision"].as_str() != Some(revision) || !payload["result"].is_object() {
        return Ok(None);
    }
    Ok(Some(SyncUpdate {
        cursor: format!(
            "{}|{}",
            source
                .created_at
                .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            source.instance_id
        ),
        arguments: payload["arguments"].clone(),
        result: payload["result"].clone(),
    }))
}
