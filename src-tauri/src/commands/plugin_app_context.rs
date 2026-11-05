use std::path::PathBuf;

use crate::app_error::AppCommandError;

pub(super) struct PluginAppWorkspace {
    pub agent_type: crate::models::AgentType,
    pub workspace_key: String,
    pub workspace_dir: PathBuf,
}

pub(super) async fn load_workspace(
    db: &crate::db::AppDatabase,
    conversation_id: i64,
    expected_workspace: &str,
) -> Result<PluginAppWorkspace, AppCommandError> {
    let conversation =
        crate::db::service::conversation_service::get_by_id(&db.conn, conversation_id as i32)
            .await
            .map_err(AppCommandError::from)?;
    let folder =
        crate::db::service::folder_service::get_folder_by_id(&db.conn, conversation.folder_id)
            .await
            .map_err(AppCommandError::from)?
            .ok_or_else(|| AppCommandError::not_found("Plugin app workspace was not found"))?;
    let workspace_key = crate::commands::skill_inventory::workspace_key(Some(&folder.path));
    if workspace_key != expected_workspace {
        return Err(AppCommandError::permission_denied(
            "Plugin app workspace no longer matches its conversation",
        ));
    }
    Ok(PluginAppWorkspace {
        agent_type: conversation.agent_type,
        workspace_key,
        workspace_dir: folder.path.into(),
    })
}
