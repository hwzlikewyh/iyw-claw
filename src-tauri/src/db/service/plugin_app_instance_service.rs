use chrono::Utc;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, Set,
};

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

pub async fn list_for_conversation(
    conn: &DatabaseConnection,
    conversation_id: i64,
) -> Result<Vec<plugin_app_instance::Model>, DbError> {
    plugin_app_instance::Entity::find()
        .filter(plugin_app_instance::Column::ConversationId.eq(conversation_id))
        .all(conn)
        .await
        .map_err(Into::into)
}

pub async fn mark_plugin_inactive(
    conn: &DatabaseConnection,
    plugin_slug: &str,
) -> Result<u64, DbError> {
    mark_plugin_inactive_version(conn, plugin_slug, None).await
}

pub async fn mark_plugin_inactive_version(
    conn: &DatabaseConnection,
    plugin_slug: &str,
    plugin_version: Option<&str>,
) -> Result<u64, DbError> {
    let mut query = plugin_app_instance::Entity::update_many()
        .col_expr(plugin_app_instance::Column::State, Expr::value("inactive"))
        .col_expr(
            plugin_app_instance::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(plugin_app_instance::Column::PluginSlug.eq(plugin_slug));
    if let Some(plugin_version) = plugin_version {
        query = query.filter(plugin_app_instance::Column::PluginVersion.eq(plugin_version));
    }
    query
        .exec(conn)
        .await
        .map(|result| result.rows_affected)
        .map_err(Into::into)
}

pub async fn mark_inactive(
    conn: &DatabaseConnection,
    instance_id: &str,
    conversation_id: Option<i64>,
) -> Result<bool, DbError> {
    let mut query = plugin_app_instance::Entity::update_many()
        .col_expr(plugin_app_instance::Column::State, Expr::value("inactive"))
        .col_expr(
            plugin_app_instance::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(plugin_app_instance::Column::InstanceId.eq(instance_id));
    if let Some(conversation_id) = conversation_id {
        query = query.filter(plugin_app_instance::Column::ConversationId.eq(conversation_id));
    }
    query
        .exec(conn)
        .await
        .map(|result| result.rows_affected > 0)
        .map_err(Into::into)
}
