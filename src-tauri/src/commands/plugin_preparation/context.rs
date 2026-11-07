use sea_orm::DatabaseConnection;
use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;
use crate::plugin_runtime::registry::PluginDescriptor;

pub(super) struct Workspace {
    pub conversation_id: i32,
    pub key: String,
    pub directory: std::path::PathBuf,
    pub name: String,
    pub agent: crate::models::AgentType,
}

pub(super) async fn load(
    conn: &DatabaseConnection,
    id: Option<i32>,
) -> Result<Option<Workspace>, AppCommandError> {
    let Some(id) = id else {
        return Ok(None);
    };
    if id <= 0 {
        return Err(AppCommandError::invalid_input("对话标识无效"));
    }
    let conversation = crate::db::service::conversation_service::get_by_id(conn, id)
        .await
        .map_err(AppCommandError::from)?;
    let folder = crate::db::service::folder_service::get_folder_by_id(conn, conversation.folder_id)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::not_found("插件工作区不存在"))?;
    Ok(Some(Workspace {
        conversation_id: id,
        key: crate::commands::skill_inventory::workspace_key(Some(&folder.path)),
        directory: folder.path.into(),
        name: folder.name,
        agent: conversation.agent_type,
    }))
}

pub(super) async fn token(
    conn: &DatabaseConnection,
    plugin: &PluginDescriptor,
    workspace: &Workspace,
) -> Result<String, AppCommandError> {
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let value = serde_json::json!([
        scope,
        workspace.conversation_id,
        workspace.key,
        workspace.agent,
        plugin.version,
        plugin.available,
        crate::plugin_runtime::policy::scope_revision(plugin, &workspace.key),
        plugin.permissions_digest,
        plugin.activations,
        plugin.permission_grants
    ]);
    Ok(format!(
        "{:x}",
        Sha256::digest(value.to_string().as_bytes())
    ))
}
