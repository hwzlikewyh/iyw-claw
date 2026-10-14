use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter};

use crate::db::entities::conversation::{self, ConversationTitleSource};
use crate::db::error::DbError;

const EMPTY_MESSAGE_COUNT: i32 = 0;

pub struct AutoTitleUpdate<'a> {
    pub conversation_id: i32,
    pub title: &'a str,
    pub source: ConversationTitleSource,
}

pub async fn update_manual(
    conn: &DatabaseConnection,
    conversation_id: i32,
    title: String,
) -> Result<(), DbError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(DbError::Validation("Conversation title is empty".into()));
    }
    let updated = conversation::Entity::update_many()
        .col_expr(conversation::Column::Title, Expr::value(title))
        .col_expr(conversation::Column::TitleLocked, Expr::value(true))
        .col_expr(
            conversation::Column::TitleSource,
            Expr::value(ConversationTitleSource::Manual),
        )
        .col_expr(
            conversation::Column::TitleSummaryAttempted,
            Expr::value(true),
        )
        .col_expr(
            conversation::Column::UpdatedAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .exec(conn)
        .await?;
    if updated.rows_affected == 0 {
        return Err(DbError::NotFound(format!(
            "Conversation not found: {conversation_id}"
        )));
    }
    Ok(())
}

pub async fn refresh(
    conn: &DatabaseConnection,
    update: AutoTitleUpdate<'_>,
) -> Result<bool, DbError> {
    let title = update.title.trim();
    let source = update.source;
    if title.is_empty() || crate::acp::conversation_title_summary::is_private_title_candidate(title)
    {
        return Ok(false);
    }
    let mut query = conversation::Entity::update_many()
        .col_expr(conversation::Column::Title, Expr::value(title))
        .col_expr(
            conversation::Column::TitleSource,
            Expr::value(source.clone()),
        )
        .filter(conversation::Column::Id.eq(update.conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::TitleLocked.eq(false));
    query = match source.clone() {
        ConversationTitleSource::UserFallback => {
            query.filter(fallback_title_filter(conn, update.conversation_id).await?)
        }
        ConversationTitleSource::CodexSummary => query
            .filter(conversation::Column::TitleSource.eq(ConversationTitleSource::UserFallback)),
        ConversationTitleSource::Agent => {
            query.filter(conversation::Column::TitleSource.ne(ConversationTitleSource::Manual))
        }
        ConversationTitleSource::Manual => return Ok(false),
    };
    let changed = query
        .filter(
            sea_orm::Condition::any()
                .add(conversation::Column::Title.is_null())
                .add(conversation::Column::Title.ne(title))
                .add(conversation::Column::TitleSource.ne(source)),
        )
        .exec(conn)
        .await?;
    Ok(changed.rows_affected > 0)
}

async fn fallback_title_filter(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<Condition, DbError> {
    let allowed = Condition::any()
        .add(conversation::Column::TitleSource.eq(ConversationTitleSource::UserFallback));
    let Some(row) = conversation::Entity::find_by_id(conversation_id)
        .one(conn)
        .await?
    else {
        return Ok(allowed);
    };
    let Some(title) = row
        .title
        .as_deref()
        .filter(|title| crate::acp::conversation_title_summary::is_private_title_candidate(title))
    else {
        return Ok(allowed);
    };
    // 仅修复仍与读取值相同的异常自动标题，避免覆盖并发生成的新标题或手动标题。
    tracing::debug!(
        conversation_id,
        "[conversation-title] private auto title recovery candidate"
    );
    Ok(allowed.add(
        Condition::all()
            .add(conversation::Column::Title.eq(title))
            .add(conversation::Column::TitleSource.ne(ConversationTitleSource::Manual)),
    ))
}

pub async fn claim_summary_attempt(
    conn: &DatabaseConnection,
    conversation_id: i32,
) -> Result<bool, DbError> {
    let claimed = conversation::Entity::update_many()
        .col_expr(
            conversation::Column::TitleSummaryAttempted,
            Expr::value(true),
        )
        .filter(conversation::Column::Id.eq(conversation_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::ParentId.is_null())
        .filter(conversation::Column::MessageCount.eq(EMPTY_MESSAGE_COUNT))
        .filter(conversation::Column::TitleLocked.eq(false))
        .filter(conversation::Column::TitleSummaryAttempted.eq(false))
        .filter(conversation::Column::TitleSource.eq(ConversationTitleSource::UserFallback))
        .exec(conn)
        .await?;
    Ok(claimed.rows_affected > 0)
}
