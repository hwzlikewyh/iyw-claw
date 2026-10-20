use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

use crate::db::entities::chat_channel_outbox;
use crate::db::error::DbError;

pub const MAX_WAITING_PER_RECIPIENT: u64 = 50;

pub async fn enqueue(
    db: &DatabaseConnection,
    channel_id: i32,
    recipient_id: &str,
    content: &str,
) -> Result<chat_channel_outbox::Model, DbError> {
    let waiting = waiting_query(channel_id, recipient_id).count(db).await?;
    if waiting >= MAX_WAITING_PER_RECIPIENT {
        return Err(DbError::Validation(
            "WeChat deferred message queue is full".to_string(),
        ));
    }
    let now = Utc::now();
    Ok(chat_channel_outbox::ActiveModel {
        id: NotSet,
        channel_id: Set(channel_id),
        recipient_id: Set(recipient_id.to_string()),
        content: Set(content.to_string()),
        state: Set("waiting_context".to_string()),
        attempts: Set(0),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?)
}

pub async fn list_waiting(
    db: &DatabaseConnection,
    channel_id: i32,
    recipient_id: &str,
) -> Result<Vec<chat_channel_outbox::Model>, DbError> {
    Ok(waiting_query(channel_id, recipient_id)
        .order_by_asc(chat_channel_outbox::Column::Id)
        .limit(MAX_WAITING_PER_RECIPIENT)
        .all(db)
        .await?)
}

pub async fn remove(db: &DatabaseConnection, id: i32) -> Result<(), DbError> {
    chat_channel_outbox::Entity::delete_by_id(id)
        .exec(db)
        .await?;
    Ok(())
}

pub async fn record_failure(
    db: &DatabaseConnection,
    item: chat_channel_outbox::Model,
    error: &str,
) -> Result<(), DbError> {
    let mut active = item.into_active_model();
    active.attempts = Set(active.attempts.unwrap().saturating_add(1));
    active.last_error = Set(Some(error.chars().take(256).collect()));
    active.updated_at = Set(Utc::now());
    active.update(db).await?;
    Ok(())
}

fn waiting_query(
    channel_id: i32,
    recipient_id: &str,
) -> sea_orm::Select<chat_channel_outbox::Entity> {
    chat_channel_outbox::Entity::find()
        .filter(chat_channel_outbox::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_outbox::Column::RecipientId.eq(recipient_id))
        .filter(chat_channel_outbox::Column::State.eq("waiting_context"))
}

// 与旧 waiting_context 文字记录隔离，复用原表，不修改数据库结构。
pub async fn enqueue_delivery(
    db: &DatabaseConnection,
    route: (i32, &str),
    content: &str,
) -> Result<chat_channel_outbox::Model, DbError> {
    let (channel_id, recipient_id) = route;
    let count = chat_channel_outbox::Entity::find()
        .filter(chat_channel_outbox::Column::ChannelId.eq(channel_id))
        .filter(chat_channel_outbox::Column::RecipientId.eq(recipient_id))
        .filter(chat_channel_outbox::Column::State.is_in([
            "delivery_pending",
            "delivery_sending",
            "delivery_context",
            "delivery_unknown",
            "delivery_blocked",
        ]))
        .count(db)
        .await?;
    if count >= MAX_WAITING_PER_RECIPIENT {
        return Err(DbError::Validation("Channel delivery queue is full".into()));
    }
    let now = Utc::now();
    Ok(chat_channel_outbox::ActiveModel {
        id: NotSet,
        channel_id: Set(channel_id),
        recipient_id: Set(recipient_id.into()),
        content: Set(content.into()),
        state: Set("delivery_pending".into()),
        attempts: Set(0),
        last_error: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?)
}

pub async fn list_deliveries(
    db: &DatabaseConnection,
) -> Result<Vec<chat_channel_outbox::Model>, DbError> {
    Ok(chat_channel_outbox::Entity::find()
        .filter(chat_channel_outbox::Column::State.is_in(["delivery_pending", "delivery_context"]))
        .order_by_asc(chat_channel_outbox::Column::UpdatedAt)
        .limit(128)
        .all(db)
        .await?)
}

pub async fn deliveries_for_target(
    db: &DatabaseConnection,
    route: (i32, &str),
) -> Result<Vec<chat_channel_outbox::Model>, DbError> {
    Ok(chat_channel_outbox::Entity::find()
        .filter(chat_channel_outbox::Column::ChannelId.eq(route.0))
        .filter(chat_channel_outbox::Column::RecipientId.eq(route.1))
        .filter(chat_channel_outbox::Column::State.is_in(["delivery_pending", "delivery_context"]))
        .limit(MAX_WAITING_PER_RECIPIENT)
        .all(db)
        .await?)
}

pub async fn has_earlier_delivery(
    db: &DatabaseConnection,
    item: &chat_channel_outbox::Model,
) -> Result<bool, DbError> {
    Ok(chat_channel_outbox::Entity::find()
        .filter(chat_channel_outbox::Column::ChannelId.eq(item.channel_id))
        .filter(chat_channel_outbox::Column::RecipientId.eq(&item.recipient_id))
        .filter(chat_channel_outbox::Column::Id.lt(item.id))
        .filter(chat_channel_outbox::Column::State.is_in([
            "delivery_pending",
            "delivery_context",
            "delivery_sending",
        ]))
        .count(db)
        .await?
        > 0)
}

pub async fn claim_delivery(db: &DatabaseConnection, id: i32) -> Result<bool, DbError> {
    use sea_orm::sea_query::Expr;
    Ok(chat_channel_outbox::Entity::update_many()
        .col_expr(
            chat_channel_outbox::Column::State,
            Expr::value("delivery_sending"),
        )
        .col_expr(
            chat_channel_outbox::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(chat_channel_outbox::Column::Id.eq(id))
        .filter(chat_channel_outbox::Column::State.is_in(["delivery_pending", "delivery_context"]))
        .exec(db)
        .await?
        .rows_affected
        == 1)
}

pub async fn finish_delivery(
    db: &DatabaseConnection,
    item: chat_channel_outbox::Model,
    result: (&str, &str, Option<&str>),
) -> Result<(), DbError> {
    let (state, content, error) = result;
    let attempts = item.attempts.saturating_add(1);
    let mut active = item.into_active_model();
    active.state = Set(state.into());
    active.content = Set(content.into());
    active.last_error = Set(error.map(str::to_string));
    active.attempts = Set(attempts);
    active.updated_at = Set(Utc::now());
    active.update(db).await?;
    Ok(())
}

pub async fn recover_deliveries(db: &DatabaseConnection) -> Result<(), DbError> {
    use sea_orm::sea_query::Expr;
    // 发送中退出无法判断平台是否接受，恢复时禁止自动重放。
    chat_channel_outbox::Entity::update_many()
        .col_expr(
            chat_channel_outbox::Column::State,
            Expr::value("delivery_unknown"),
        )
        .col_expr(
            chat_channel_outbox::Column::LastError,
            Expr::value("INTERRUPTED_DELIVERY_UNKNOWN"),
        )
        .filter(chat_channel_outbox::Column::State.eq("delivery_sending"))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn expire_sending(db: &DatabaseConnection) -> Result<(), DbError> {
    use sea_orm::sea_query::Expr;
    chat_channel_outbox::Entity::update_many()
        .col_expr(
            chat_channel_outbox::Column::State,
            Expr::value("delivery_unknown"),
        )
        .col_expr(
            chat_channel_outbox::Column::LastError,
            Expr::value("INTERRUPTED_DELIVERY_UNKNOWN"),
        )
        .filter(chat_channel_outbox::Column::State.eq("delivery_sending"))
        .filter(
            chat_channel_outbox::Column::UpdatedAt.lt(Utc::now() - chrono::Duration::seconds(180)),
        )
        .exec(db)
        .await?;
    Ok(())
}

pub async fn remove_pending(db: &DatabaseConnection, id: i32) -> Result<(), DbError> {
    chat_channel_outbox::Entity::delete_many()
        .filter(chat_channel_outbox::Column::Id.eq(id))
        .filter(chat_channel_outbox::Column::State.is_in(["delivery_pending", "delivery_context"]))
        .exec(db)
        .await?;
    Ok(())
}
