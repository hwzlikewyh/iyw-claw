use std::collections::HashMap;

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
};

use crate::db::entities::conversation::ConversationKind;
use crate::db::entities::{automation_run, conversation, folder};
use crate::db::error::DbError;
use crate::models::AgentType;

const SQLITE_TITLE_QUERY_CHUNK_SIZE: usize = 500;

/// Refresh every visible, unlocked Codex conversation whose external session
/// id has a title in `titles`. Failed chunks and rows are logged and skipped so
/// successful updates can still be returned for sidebar notifications.
pub(crate) async fn refresh_codex_auto_titles(
    conn: &DatabaseConnection,
    titles: &HashMap<String, String>,
) -> Vec<i32> {
    let external_ids = indexed_session_ids(titles);
    let mut refreshed = Vec::new();

    for external_id_chunk in external_ids.chunks(SQLITE_TITLE_QUERY_CHUNK_SIZE) {
        let candidates = match select_candidates(conn, external_id_chunk).await {
            Ok(candidates) => candidates,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    session_count = external_id_chunk.len(),
                    "failed to select Codex title refresh candidates; skipping chunk"
                );
                continue;
            }
        };
        refresh_candidates(conn, titles, candidates, &mut refreshed).await;
    }

    refreshed
}

fn indexed_session_ids(titles: &HashMap<String, String>) -> Vec<String> {
    titles
        .iter()
        .filter(|(external_id, title)| !external_id.trim().is_empty() && !title.trim().is_empty())
        .map(|(external_id, _)| external_id.clone())
        .collect()
}

async fn select_candidates(
    conn: &DatabaseConnection,
    external_ids: &[String],
) -> Result<Vec<conversation::Model>, DbError> {
    conversation::Entity::find()
        .filter(conversation::Column::AgentType.eq(AgentType::Codex.as_wire().into_owned()))
        .filter(conversation::Column::ExternalId.is_in(external_ids.iter().cloned()))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::TitleLocked.eq(false))
        .filter(conversation::Column::Kind.ne(ConversationKind::Loop))
        .filter(non_automation_condition())
        .filter(live_folder_condition())
        .order_by_asc(conversation::Column::Id)
        .all(conn)
        .await
        .map_err(DbError::from)
}

fn live_folder_condition() -> sea_orm::sea_query::SimpleExpr {
    conversation::Column::FolderId.in_subquery(
        sea_orm::sea_query::Query::select()
            .column(folder::Column::Id)
            .from(folder::Entity)
            .and_where(folder::Column::DeletedAt.is_null())
            .to_owned(),
    )
}

fn non_automation_condition() -> sea_orm::sea_query::SimpleExpr {
    let automation_conversations = automation_run::Entity::find()
        .select_only()
        .column(automation_run::Column::ConversationId)
        .filter(automation_run::Column::ConversationId.is_not_null())
        .into_query();
    conversation::Column::Id
        .into_expr()
        .not_in_subquery(automation_conversations)
}

async fn refresh_candidates(
    conn: &DatabaseConnection,
    titles: &HashMap<String, String>,
    candidates: Vec<conversation::Model>,
    refreshed: &mut Vec<i32>,
) {
    for candidate in candidates {
        let Some(external_id) = candidate.external_id.as_deref() else {
            continue;
        };
        let Some(title) = titles.get(external_id) else {
            continue;
        };
        match refresh_candidate(conn, &candidate, title).await {
            Ok(true) => refreshed.push(candidate.id),
            Ok(false) => {}
            Err(error) => tracing::warn!(
                error = %error,
                conversation_id = candidate.id,
                external_id,
                "failed to refresh Codex title candidate; skipping row"
            ),
        }
    }
}

async fn refresh_candidate(
    conn: &DatabaseConnection,
    candidate: &conversation::Model,
    title: &str,
) -> Result<bool, DbError> {
    use sea_orm::sea_query::Expr;

    let title = title.trim();
    let Some(external_id) = candidate.external_id.as_deref() else {
        return Ok(false);
    };
    if title.is_empty() || candidate.title.as_deref() == Some(title) {
        return Ok(false);
    }

    let old_title = observed_title_condition(candidate.title.as_deref());
    let result = conversation::Entity::update_many()
        .col_expr(conversation::Column::Title, Expr::value(title))
        .filter(conversation::Column::Id.eq(candidate.id))
        .filter(conversation::Column::AgentType.eq(AgentType::Codex.as_wire().into_owned()))
        .filter(conversation::Column::ExternalId.eq(external_id))
        .filter(conversation::Column::DeletedAt.is_null())
        .filter(conversation::Column::TitleLocked.eq(false))
        .filter(non_automation_condition())
        .filter(live_folder_condition())
        .filter(old_title)
        .exec(conn)
        .await?;
    Ok(result.rows_affected > 0)
}

fn observed_title_condition(old_title: Option<&str>) -> sea_orm::sea_query::SimpleExpr {
    match old_title {
        Some(title) => conversation::Column::Title.eq(title),
        None => conversation::Column::Title.is_null(),
    }
}
