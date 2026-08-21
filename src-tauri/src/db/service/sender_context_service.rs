use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, Set,
};

use crate::db::entities::chat_channel_sender_context;
use crate::db::error::DbError;

pub async fn get_or_create(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let existing = chat_channel_sender_context::Entity::find()
        .filter(chat_channel_sender_context::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_sender_context::Column::SenderId.eq(sender_id))
        .one(conn)
        .await?;

    if let Some(model) = existing {
        return Ok(model);
    }

    let now = Utc::now();
    let active = chat_channel_sender_context::ActiveModel {
        id: NotSet,
        channel_id: Set(channel_id),
        sender_id: Set(sender_id.to_string()),
        current_folder_id: Set(None),
        current_agent_type: Set(None),
        current_conversation_id: Set(None),
        current_connection_id: Set(None),
        weixin_context_token: Set(None),
        auto_approve: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(active.insert(conn).await?)
}

pub async fn update_folder(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    folder_id: Option<i32>,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let model = get_or_create(conn, channel_id, sender_id).await?;
    let mut active = model.into_active_model();
    active.current_folder_id = Set(folder_id);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

pub async fn update_agent(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    agent_type: Option<String>,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let model = get_or_create(conn, channel_id, sender_id).await?;
    let mut active = model.into_active_model();
    active.current_agent_type = Set(agent_type);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

pub async fn update_session(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    conversation_id: Option<i32>,
    connection_id: Option<String>,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let model = get_or_create(conn, channel_id, sender_id).await?;
    let mut active = model.into_active_model();
    active.current_conversation_id = Set(conversation_id);
    active.current_connection_id = Set(connection_id);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

pub async fn clear_session(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
) -> Result<chat_channel_sender_context::Model, DbError> {
    update_session(conn, channel_id, sender_id, None, None).await
}

pub async fn clear_connection(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let model = get_or_create(conn, channel_id, sender_id).await?;
    let mut active = model.into_active_model();
    active.current_connection_id = Set(None);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

/// Clear a connection only when it is still the one that triggered cleanup.
/// Late disconnect/error events must not erase a newer session connection.
pub async fn clear_connection_if_matches(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    expected_connection_id: &str,
) -> Result<u64, DbError> {
    let result = chat_channel_sender_context::Entity::update_many()
        .col_expr(
            chat_channel_sender_context::Column::CurrentConnectionId,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            chat_channel_sender_context::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(chat_channel_sender_context::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_sender_context::Column::SenderId.eq(sender_id))
        .filter(chat_channel_sender_context::Column::CurrentConnectionId.eq(expected_connection_id))
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}

/// Clear both session fields only when the expected connection still owns the
/// row. This is used when a just-created kickoff failed before any reply.
pub async fn clear_session_if_connection_matches(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    expected_connection_id: &str,
) -> Result<u64, DbError> {
    let result = chat_channel_sender_context::Entity::update_many()
        .col_expr(
            chat_channel_sender_context::Column::CurrentConversationId,
            Expr::value(Option::<i32>::None),
        )
        .col_expr(
            chat_channel_sender_context::Column::CurrentConnectionId,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            chat_channel_sender_context::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(chat_channel_sender_context::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_sender_context::Column::SenderId.eq(sender_id))
        .filter(chat_channel_sender_context::Column::CurrentConnectionId.eq(expected_connection_id))
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}

pub async fn update_auto_approve(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    auto_approve: bool,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let model = get_or_create(conn, channel_id, sender_id).await?;
    let mut active = model.into_active_model();
    active.auto_approve = Set(auto_approve);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

pub async fn get_weixin_context_token(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
) -> Result<Option<String>, DbError> {
    let context = chat_channel_sender_context::Entity::find()
        .filter(chat_channel_sender_context::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_sender_context::Column::SenderId.eq(sender_id))
        .one(conn)
        .await?;
    Ok(context.and_then(|model| model.weixin_context_token))
}

pub async fn update_weixin_context_token(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    context_token: Option<String>,
) -> Result<chat_channel_sender_context::Model, DbError> {
    let model = get_or_create(conn, channel_id, sender_id).await?;
    let mut active = model.into_active_model();
    active.weixin_context_token = Set(context_token);
    active.updated_at = Set(Utc::now());
    Ok(active.update(conn).await?)
}

pub async fn clear_weixin_context_token_if_matches(
    conn: &DatabaseConnection,
    channel_id: i32,
    sender_id: &str,
    context_token: &str,
) -> Result<(), DbError> {
    // Compare-and-clear prevents a late failure for an old iLink token from
    // erasing a newer token persisted by a later inbound message.
    chat_channel_sender_context::Entity::update_many()
        .col_expr(
            chat_channel_sender_context::Column::WeixinContextToken,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            chat_channel_sender_context::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(chat_channel_sender_context::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_sender_context::Column::SenderId.eq(sender_id))
        .filter(chat_channel_sender_context::Column::WeixinContextToken.eq(context_token))
        .exec(conn)
        .await?;
    Ok(())
}
