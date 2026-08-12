use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, Set,
};

use crate::db::entities::chat_channel_tool_request;
use crate::db::error::DbError;

pub async fn find(
    conn: &DatabaseConnection,
    caller_scope: &str,
    operation: &str,
    request_id: &str,
) -> Result<Option<chat_channel_tool_request::Model>, DbError> {
    Ok(chat_channel_tool_request::Entity::find()
        .filter(chat_channel_tool_request::Column::CallerScope.eq(caller_scope))
        .filter(chat_channel_tool_request::Column::Operation.eq(operation))
        .filter(chat_channel_tool_request::Column::RequestId.eq(request_id))
        .one(conn)
        .await?)
}

pub async fn begin(
    conn: &DatabaseConnection,
    caller_scope: &str,
    operation: &str,
    request_id: &str,
    input_digest: &str,
) -> Result<chat_channel_tool_request::Model, DbError> {
    let now = Utc::now();
    Ok(chat_channel_tool_request::ActiveModel {
        id: NotSet,
        caller_scope: Set(caller_scope.to_string()),
        operation: Set(operation.to_string()),
        request_id: Set(request_id.to_string()),
        input_digest: Set(input_digest.to_string()),
        status: Set("processing".to_string()),
        result_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await?)
}

pub async fn finish(
    conn: &DatabaseConnection,
    model: chat_channel_tool_request::Model,
    status: &str,
    result_json: String,
) -> Result<(), DbError> {
    let mut active = model.into_active_model();
    active.status = Set(status.to_string());
    active.result_json = Set(Some(result_json));
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(())
}

pub async fn cancel(
    conn: &DatabaseConnection,
    caller_scope: &str,
    operation: &str,
    request_id: &str,
    result_json: String,
) -> Result<bool, DbError> {
    let Some(model) = find(conn, caller_scope, operation, request_id).await? else {
        return Ok(false);
    };
    if model.status != "processing" {
        return Ok(false);
    }
    let mut active = model.into_active_model();
    active.status = Set("failed".to_string());
    active.result_json = Set(Some(result_json));
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(true)
}
