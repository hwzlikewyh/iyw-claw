use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, QueryResult, Statement, Value};

use crate::app_error::AppCommandError;

#[derive(Debug)]
pub(crate) struct TaskHistoryHit {
    pub conversation_id: i32,
    pub turn_generation: i64,
    pub intent: String,
    pub result: Option<String>,
    pub decisions: Option<String>,
    pub failures: Option<String>,
    pub pending_items: Option<String>,
}

pub(crate) async fn search_task_history(
    conn: &DatabaseConnection,
    query: &str,
    limit: usize,
) -> Result<Vec<TaskHistoryHit>, AppCommandError> {
    let rows = if broad_query(query) {
        query_rows(
            conn,
            BROAD_SQL,
            vec![(limit.saturating_mul(4) as i64).into()],
        )
        .await?
    } else {
        let tokens = fts_query(query);
        let pattern = format!("%{}%", query.trim().to_lowercase());
        let values = vec![
            tokens.into(),
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.into(),
            (limit.saturating_mul(4) as i64).into(),
        ];
        match query_rows(conn, FTS_SQL, values).await {
            Ok(rows) => rows,
            Err(_) => {
                let pattern = format!("%{}%", query.trim().to_lowercase());
                query_rows(
                    conn,
                    LIKE_SQL,
                    vec![
                        pattern.clone().into(),
                        pattern.clone().into(),
                        pattern.clone().into(),
                        pattern.clone().into(),
                        pattern.into(),
                        (limit.saturating_mul(4) as i64).into(),
                    ],
                )
                .await?
            }
        }
    };
    deduplicate(rows, limit)
}

async fn query_rows(
    conn: &DatabaseConnection,
    sql: &str,
    values: Vec<Value>,
) -> Result<Vec<QueryResult>, AppCommandError> {
    conn.query_all(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map_err(|error| AppCommandError::database_error(error.to_string()))
}

fn deduplicate(
    rows: Vec<QueryResult>,
    limit: usize,
) -> Result<Vec<TaskHistoryHit>, AppCommandError> {
    let mut seen = std::collections::BTreeSet::new();
    rows.into_iter()
        .filter_map(|row| {
            let conversation_id = row.try_get::<i32>("", "conversation_id").ok()?;
            seen.insert(conversation_id).then_some(decode(row))
        })
        .take(limit)
        .collect()
}

fn decode(row: QueryResult) -> Result<TaskHistoryHit, AppCommandError> {
    let error = |error: sea_orm::DbErr| AppCommandError::database_error(error.to_string());
    Ok(TaskHistoryHit {
        conversation_id: row.try_get("", "conversation_id").map_err(error)?,
        turn_generation: row.try_get("", "turn_generation").map_err(error)?,
        intent: row.try_get("", "intent").map_err(error)?,
        result: row.try_get("", "result").ok(),
        decisions: row.try_get("", "decisions").ok(),
        failures: row.try_get("", "failures").ok(),
        pending_items: row.try_get("", "pending_items").ok(),
    })
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .take(8)
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn broad_query(query: &str) -> bool {
    let markers = ["之前", "以前", "历史", "做过", "任务", "what did"];
    markers.iter().any(|marker| query.contains(marker)) && query.chars().count() <= 32
}

const BROAD_SQL: &str = "SELECT conversation_id, turn_generation, intent, result, decisions, failures, pending_items FROM session_task_projection ORDER BY occurred_at DESC LIMIT ?";
const LIKE_SQL: &str = "SELECT conversation_id, turn_generation, intent, result, decisions, failures, pending_items FROM session_task_projection WHERE LOWER(intent) LIKE ? OR LOWER(COALESCE(result,'')) LIKE ? OR LOWER(COALESCE(decisions,'')) LIKE ? OR LOWER(COALESCE(failures,'')) LIKE ? OR LOWER(COALESCE(pending_items,'')) LIKE ? ORDER BY occurred_at DESC LIMIT ?";
const FTS_SQL: &str = "SELECT conversation_id, turn_generation, intent, result, decisions, failures, pending_items FROM session_task_projection AS p WHERE p.conversation_id IN (SELECT CAST(conversation_id AS INTEGER) FROM session_task_fts WHERE session_task_fts MATCH ?) OR LOWER(p.intent) LIKE ? OR LOWER(COALESCE(p.result,'')) LIKE ? OR LOWER(COALESCE(p.decisions,'')) LIKE ? OR LOWER(COALESCE(p.failures,'')) LIKE ? OR LOWER(COALESCE(p.pending_items,'')) LIKE ? ORDER BY p.occurred_at DESC LIMIT ?";
