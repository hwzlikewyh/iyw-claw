use std::time::Duration;

use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
    TransactionTrait,
};

use crate::db::entities::{automation_run, chat_channel_message_log, conversation};
use crate::db::error::DbError;

const BATCH_SIZE: u64 = 200;
const MAX_BATCHES: usize = 10;
pub(crate) const ROUND_LIMIT: u64 = BATCH_SIZE * MAX_BATCHES as u64;
const BATCH_PAUSE: Duration = Duration::from_millis(25);

pub(super) async fn cleanup_logs(
    conn: &DatabaseConnection,
    cutoff: DateTime<Utc>,
) -> Result<u64, DbError> {
    let mut deleted = 0;
    for batch in 0..MAX_BATCHES {
        let ids = chat_channel_message_log::Entity::find()
            .select_only()
            .column(chat_channel_message_log::Column::Id)
            .filter(chat_channel_message_log::Column::CreatedAt.lt(cutoff))
            .order_by_asc(chat_channel_message_log::Column::CreatedAt)
            .order_by_asc(chat_channel_message_log::Column::Id)
            .limit(BATCH_SIZE)
            .into_query();
        let result = chat_channel_message_log::Entity::delete_many()
            .filter(chat_channel_message_log::Column::Id.in_subquery(ids))
            .exec(conn)
            .await?;
        deleted += result.rows_affected;
        if result.rows_affected < BATCH_SIZE {
            break;
        }
        if batch + 1 < MAX_BATCHES {
            tokio::time::sleep(BATCH_PAUSE).await;
        }
    }
    Ok(deleted)
}

pub(super) async fn prune_runs(conn: &DatabaseConnection, keep_days: i64) -> Result<u64, DbError> {
    let cutoff = Utc::now() - chrono::Duration::days(keep_days);
    let mut deleted = 0;
    for batch in 0..MAX_BATCHES {
        let count = crate::db::retry_sqlite_maintenance("automation.prune_old_runs", || {
            prune_run_batch(conn, cutoff)
        })
        .await?;
        deleted += count;
        if count < BATCH_SIZE {
            break;
        }
        if batch + 1 < MAX_BATCHES {
            tokio::time::sleep(BATCH_PAUSE).await;
        }
    }
    Ok(deleted)
}

async fn prune_run_batch(conn: &DatabaseConnection, cutoff: DateTime<Utc>) -> Result<u64, DbError> {
    let txn = conn.begin().await?;
    let rows = automation_run::Entity::find()
        .select_only()
        .columns([
            automation_run::Column::Id,
            automation_run::Column::ConversationId,
        ])
        .filter(automation_run::Column::CreatedAt.lt(cutoff))
        .filter(Expr::cust("status <> 'running'"))
        .order_by_asc(automation_run::Column::CreatedAt)
        .order_by_asc(automation_run::Column::Id)
        .limit(BATCH_SIZE)
        .into_tuple::<(i32, Option<i32>)>()
        .all(&txn)
        .await?;
    if rows.is_empty() {
        txn.commit().await?;
        return Ok(0);
    }
    let conversation_ids: Vec<_> = rows.iter().filter_map(|(_, id)| *id).collect();
    if !conversation_ids.is_empty() {
        conversation::Entity::update_many()
            .col_expr(conversation::Column::DeletedAt, Expr::value(Utc::now()))
            .filter(conversation::Column::Id.is_in(conversation_ids))
            .filter(conversation::Column::DeletedAt.is_null())
            .exec(&txn)
            .await?;
    }
    let result = automation_run::Entity::delete_many()
        .filter(automation_run::Column::Id.is_in(rows.into_iter().map(|(id, _)| id)))
        .exec(&txn)
        .await?;
    txn.commit().await?;
    Ok(result.rows_affected)
}
