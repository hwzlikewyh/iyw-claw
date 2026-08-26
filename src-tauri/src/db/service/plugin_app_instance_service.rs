use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, IntoActiveModel, Set};

use crate::db::entities::plugin_app_instance;
use crate::db::error::DbError;

#[derive(Debug, Clone)]
pub struct PluginAppInstanceInput {
    pub instance_id: String,
    pub conversation_id: i64,
    pub tool_call_id: String,
    pub plugin_slug: String,
    pub plugin_version: String,
    pub app_key: String,
    pub workspace_key: String,
    pub launch_payload_json: String,
    pub state: String,
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: PluginAppInstanceInput,
) -> Result<plugin_app_instance::Model, DbError> {
    let now = Utc::now();
    let existing = plugin_app_instance::Entity::find_by_id(input.instance_id.clone())
        .one(conn)
        .await?;
    let created_at = existing.as_ref().map_or(now, |value| value.created_at);
    if let Some(model) = existing {
        if model.conversation_id != input.conversation_id
            || model.tool_call_id != input.tool_call_id
            || model.plugin_slug != input.plugin_slug
            || model.plugin_version != input.plugin_version
            || model.app_key != input.app_key
            || model.workspace_key != input.workspace_key
        {
            return Err(DbError::Validation(
                "plugin app instance identity cannot be changed".to_string(),
            ));
        }
        let mut active = model.into_active_model();
        active.launch_payload_json = Set(input.launch_payload_json);
        active.state = Set(input.state);
        active.updated_at = Set(now);
        return active.update(conn).await.map_err(Into::into);
    }
    plugin_app_instance::ActiveModel {
        instance_id: Set(input.instance_id),
        conversation_id: Set(input.conversation_id),
        tool_call_id: Set(input.tool_call_id),
        plugin_slug: Set(input.plugin_slug),
        plugin_version: Set(input.plugin_version),
        app_key: Set(input.app_key),
        workspace_key: Set(input.workspace_key),
        launch_payload_json: Set(input.launch_payload_json),
        state: Set(input.state),
        created_at: Set(created_at),
        updated_at: Set(now),
    }
    .insert(conn)
    .await
    .map_err(Into::into)
}

pub async fn find(
    conn: &DatabaseConnection,
    instance_id: &str,
) -> Result<Option<plugin_app_instance::Model>, DbError> {
    plugin_app_instance::Entity::find_by_id(instance_id.to_string())
        .one(conn)
        .await
        .map_err(Into::into)
}
