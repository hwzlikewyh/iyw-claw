use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, Value};

use crate::app_error::AppCommandError;

use super::recall_scope::UserMemoryRecallScope;
use super::recall_status::database_error;
use super::recall_types::{
    bounded_recall_content, UserMemoryRecallItem, MAX_RECALL_ITEM_CHARS, MAX_RECALL_TOTAL_CHARS,
};
use super::recall_validity::{push_query_at, valid_at_sql};

pub(super) struct HydrateRequest<'a> {
    pub ranked: Vec<(String, f64, Vec<String>)>,
    pub query_at: &'a str,
    pub limit: usize,
    pub scope: &'a UserMemoryRecallScope,
}

pub(super) async fn hydrate<C: ConnectionTrait>(
    db: &C,
    request: HydrateRequest<'_>,
) -> Result<Vec<UserMemoryRecallItem>, AppCommandError> {
    if request.ranked.is_empty() {
        return Ok(Vec::new());
    }
    let ids = request
        .ranked
        .iter()
        .map(|(id, _, _)| id.clone())
        .collect::<Vec<_>>();
    let mut by_id = load_hydrated_items(db, &ids, request.query_at, request.scope).await?;
    let mut remaining = MAX_RECALL_TOTAL_CHARS;
    let mut items = Vec::new();
    for (id, score, lanes) in request.ranked {
        let Some(mut item) = by_id.remove(&id) else {
            continue;
        };
        let Some(content) =
            bounded_recall_content(&item.content, remaining.min(MAX_RECALL_ITEM_CHARS))
        else {
            continue;
        };
        remaining = remaining.saturating_sub(content.chars().count());
        item.content = content;
        item.score = score;
        item.lanes = lanes;
        items.push(item);
        if items.len() == request.limit {
            break;
        }
    }
    Ok(items)
}

async fn load_hydrated_items<C: ConnectionTrait>(
    db: &C,
    ids: &[String],
    query_at: &str,
    scope: &UserMemoryRecallScope,
) -> Result<BTreeMap<String, UserMemoryRecallItem>, AppCommandError> {
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let validity = valid_at_sql("memory_item_current");
    let scope_predicate = scope.predicate("memory_item_current");
    let sql = format!(
        "SELECT id, kind, content, confidence, importance, source_revision FROM memory_item_current INDEXED BY sqlite_autoindex_memory_item_current_1 WHERE id IN ({placeholders}) AND {scope_predicate} AND trust_class = 'host_confirmed' AND sensitive = 0 AND superseded_by IS NULL{validity}"
    );
    let expected_count = ids.len();
    let mut values = ids.iter().cloned().map(Value::from).collect::<Vec<_>>();
    scope.push_bind(&mut values);
    push_query_at(&mut values, query_at);
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await
        .map_err(database_error)?;
    hydrated_by_id(rows, expected_count)
}

fn hydrated_by_id(
    rows: Vec<QueryResult>,
    expected_count: usize,
) -> Result<BTreeMap<String, UserMemoryRecallItem>, AppCommandError> {
    let hydrated = rows
        .into_iter()
        .map(item_row)
        .collect::<Result<Vec<_>, _>>()?;
    if hydrated.len() != expected_count {
        return Err(incomplete_hydration(expected_count, hydrated.len()));
    }
    let by_id = hydrated
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != expected_count {
        return Err(AppCommandError::database_error(
            "User memory recall hydration contains duplicate ids",
        ));
    }
    Ok(by_id)
}

fn incomplete_hydration(expected: usize, actual: usize) -> AppCommandError {
    AppCommandError::database_error("User memory recall hydration is incomplete")
        .with_detail(format!("expected {expected} rows, got {actual}"))
}

fn item_row(row: QueryResult) -> Result<UserMemoryRecallItem, AppCommandError> {
    Ok(UserMemoryRecallItem {
        id: row.try_get("", "id").map_err(database_error)?,
        kind: row.try_get("", "kind").map_err(database_error)?,
        content: row.try_get("", "content").map_err(database_error)?,
        confidence: row.try_get("", "confidence").map_err(database_error)?,
        importance: row.try_get("", "importance").map_err(database_error)?,
        source_revision: row.try_get("", "source_revision").map_err(database_error)?,
        score: 0.0,
        lanes: Vec::new(),
    })
}
