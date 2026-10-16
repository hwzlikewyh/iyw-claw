use chrono::{Duration, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::db::entities::{automation_run, chat_channel};
use crate::db::error::DbError;
use crate::models::AutomationRunStatus;

const DELIVERY_BATCH_SIZE: u64 = 20;
const ABANDONED_DELIVERY_MINUTES: i64 = 5;

#[derive(Default)]
pub struct NotificationSnapshot {
    pub channel_id: Option<i32>,
    pub channel_name: Option<String>,
    pub target_json: Option<String>,
    pub status: Option<String>,
}

pub async fn validate_channel(
    conn: &DatabaseConnection,
    config: &serde_json::Value,
) -> Result<(), DbError> {
    let Some(value) = config
        .get("notification_channel_id")
        .filter(|value| !value.is_null())
    else {
        return Ok(());
    };
    let id = value
        .as_i64()
        .and_then(|id| i32::try_from(id).ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| DbError::Validation("invalid notification channel".into()))?;
    if chat_channel::Entity::find_by_id(id)
        .one(conn)
        .await?
        .is_none()
    {
        return Err(DbError::Validation(
            "notification channel no longer exists".into(),
        ));
    }
    Ok(())
}

pub async fn snapshot(
    conn: &DatabaseConnection,
    config: &serde_json::Value,
) -> NotificationSnapshot {
    let Some(id) = config
        .get("notification_channel_id")
        .and_then(|value| value.as_i64())
        .and_then(|id| i32::try_from(id).ok())
    else {
        return NotificationSnapshot::default();
    };
    let channel_name = chat_channel::Entity::find_by_id(id)
        .one(conn)
        .await
        .ok()
        .flatten()
        .map(|channel| channel.name);
    let target_json = match crate::chat_channel::target_registry::resolve_default(conn, id).await {
        Ok(Some((_, target))) => serde_json::to_string(&target).ok(),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(channel_id = id, error = %error, "[automation] notification target snapshot unavailable");
            None
        }
    };
    NotificationSnapshot {
        channel_id: Some(id),
        channel_name,
        target_json,
        status: Some("pending".into()),
    }
}

pub async fn pending(conn: &DatabaseConnection) -> Result<Vec<automation_run::Model>, DbError> {
    Ok(automation_run::Entity::find()
        .filter(automation_run::Column::NotificationStatus.eq("pending"))
        .filter(automation_run::Column::Status.is_in([
            AutomationRunStatus::Succeeded,
            AutomationRunStatus::Failed,
            AutomationRunStatus::Cancelled,
        ]))
        .order_by_asc(automation_run::Column::Id)
        .limit(DELIVERY_BATCH_SIZE)
        .all(conn)
        .await?)
}

pub async fn claim(conn: &DatabaseConnection, id: i32) -> Result<bool, DbError> {
    let result = automation_run::Entity::update_many()
        .col_expr(
            automation_run::Column::NotificationStatus,
            Expr::value("sending"),
        )
        .col_expr(
            automation_run::Column::NotificationUpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(automation_run::Column::Id.eq(id))
        .filter(automation_run::Column::NotificationStatus.eq("pending"))
        .exec(conn)
        .await?;
    Ok(result.rows_affected == 1)
}

pub async fn finish(
    conn: &DatabaseConnection,
    id: i32,
    outcome: (&str, Option<String>),
) -> Result<(), DbError> {
    automation_run::Entity::update_many()
        .col_expr(
            automation_run::Column::NotificationStatus,
            Expr::value(outcome.0),
        )
        .col_expr(
            automation_run::Column::NotificationError,
            Expr::value(outcome.1),
        )
        .col_expr(
            automation_run::Column::NotificationUpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(automation_run::Column::Id.eq(id))
        .filter(automation_run::Column::NotificationStatus.eq("sending"))
        .exec(conn)
        .await?;
    Ok(())
}

pub async fn recover_abandoned(conn: &DatabaseConnection) -> Result<u64, DbError> {
    let result = automation_run::Entity::update_many()
        .col_expr(
            automation_run::Column::NotificationStatus,
            Expr::value("failed"),
        )
        .col_expr(
            automation_run::Column::NotificationError,
            Expr::value(
                "Delivery interrupted; result unknown. Not resent to avoid duplicate messages.",
            ),
        )
        .filter(automation_run::Column::NotificationStatus.eq("sending"))
        .filter(
            automation_run::Column::NotificationUpdatedAt
                .lt(Utc::now() - Duration::minutes(ABANDONED_DELIVERY_MINUTES)),
        )
        .exec(conn)
        .await?;
    Ok(result.rows_affected)
}
