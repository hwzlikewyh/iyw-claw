use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement};

use super::recall_query::{LaneCollection, MAX_LANE_CANDIDATES};
use super::recall_rank::{add_rows, Candidate, LaneScore};
use super::recall_scope::UserMemoryRecallScope;
use super::recall_validity::{push_query_at, valid_at_sql};

const FTS_SCAN_LIMIT: usize = MAX_LANE_CANDIDATES * 4;

pub(super) struct FtsQuery<'a> {
    pub query: &'a str,
    pub query_at: &'a str,
    pub table: &'static str,
    pub weight: f64,
    pub scope: &'a UserMemoryRecallScope,
}

struct FtsMatchQueries {
    primary: String,
    fallback: Option<String>,
}

pub(super) async fn collect_fts<C: ConnectionTrait>(
    db: &C,
    query: FtsQuery<'_>,
    out: &mut BTreeMap<String, Candidate>,
) -> Result<LaneCollection, String> {
    let lane = fts_lane_name(query.table);
    let minimum_token_chars = if query.table.ends_with("_trigram") {
        3
    } else {
        1
    };
    let Some(match_queries) = fts_queries(query.query, minimum_token_chars) else {
        return Ok(LaneCollection::skipped("query_has_no_tokens"));
    };
    let mut rows = collect_fts_variant(db, &query, &match_queries.primary, lane).await?;
    if rows.is_empty() {
        if let Some(fallback) = match_queries.fallback.as_deref() {
            rows = collect_fts_variant(db, &query, fallback, lane).await?;
        }
    }
    let candidate_count = rows.len();
    add_rows(
        rows,
        out,
        LaneScore {
            name: lane,
            weight: query.weight,
        },
    );
    Ok(LaneCollection::collected(candidate_count))
}

async fn collect_fts_variant<C: ConnectionTrait>(
    db: &C,
    query: &FtsQuery<'_>,
    match_query: &str,
    lane: &str,
) -> Result<Vec<QueryResult>, String> {
    let rows = execute_fts_query(
        db,
        fast_fts_statement(query, match_query),
        query.table,
        lane,
    )
    .await?;
    if rows.len() >= MAX_LANE_CANDIDATES
        || !has_fts_match(db, query.table, match_query, lane).await?
    {
        return Ok(rows);
    }
    // 预取页不足时必须回退到过滤后截断，避免不可见高排名项遮挡后续合法项。
    execute_fts_query(
        db,
        filtered_fts_statement(query, match_query),
        query.table,
        lane,
    )
    .await
}

fn fast_fts_statement(query: &FtsQuery<'_>, match_query: &str) -> Statement {
    let validity = valid_at_sql("memory_item_current");
    let scope = query.scope.predicate("memory_item_current");
    let sql = format!(
        "SELECT memory_item_current.id FROM (SELECT {table}.rowid, rank AS fts_rank FROM {table} WHERE {table} MATCH ? LIMIT ?) AS matches JOIN memory_item_current ON memory_item_current.row_id = matches.rowid WHERE {scope} AND memory_item_current.trust_class = 'host_confirmed' AND memory_item_current.sensitive = 0 AND memory_item_current.superseded_by IS NULL{validity} ORDER BY matches.fts_rank, memory_item_current.id LIMIT ?",
        table = query.table,
    );
    let mut values = vec![
        match_query.to_string().into(),
        (FTS_SCAN_LIMIT as i64).into(),
    ];
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_LANE_CANDIDATES as i64).into());
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}

async fn has_fts_match<C: ConnectionTrait>(
    db: &C,
    table: &str,
    match_query: &str,
    lane: &str,
) -> Result<bool, String> {
    let sql = format!("SELECT rowid FROM {table} WHERE {table} MATCH ? LIMIT 1");
    let statement =
        Statement::from_sql_and_values(DbBackend::Sqlite, sql, [match_query.to_string().into()]);
    execute_fts_query(db, statement, table, lane)
        .await
        .map(|rows| !rows.is_empty())
}

fn filtered_fts_statement(query: &FtsQuery<'_>, match_query: &str) -> Statement {
    let validity = valid_at_sql("memory_item_current");
    let scope = query.scope.predicate("memory_item_current");
    let sql = format!(
        "SELECT memory_item_current.id FROM (SELECT {table}.rowid, rank AS fts_rank FROM {table} WHERE {table} MATCH ? AND EXISTS (SELECT 1 FROM memory_item_current WHERE memory_item_current.row_id = {table}.rowid AND {scope} AND memory_item_current.trust_class = 'host_confirmed' AND memory_item_current.sensitive = 0 AND memory_item_current.superseded_by IS NULL{validity}) ORDER BY rank LIMIT ?) AS matches JOIN memory_item_current ON memory_item_current.row_id = matches.rowid ORDER BY matches.fts_rank, memory_item_current.id",
        table = query.table,
    );
    let mut values = vec![match_query.to_string().into()];
    query.scope.push_bind(&mut values);
    push_query_at(&mut values, query.query_at);
    values.push((MAX_LANE_CANDIDATES as i64).into());
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}

async fn execute_fts_query<C: ConnectionTrait>(
    db: &C,
    statement: Statement,
    table: &str,
    lane: &str,
) -> Result<Vec<QueryResult>, String> {
    db.query_all(statement).await.map_err(|error| {
        tracing::debug!(table, error = %error, "[memory-recall] FTS lane unavailable");
        format!("fts_{lane}_error")
    })
}

fn fts_lane_name(table: &'static str) -> &'static str {
    table.strip_prefix("memory_item_fts_").unwrap_or(table)
}

fn fts_queries(query: &str, minimum_token_chars: usize) -> Option<FtsMatchQueries> {
    let tokens = query
        .split_whitespace()
        .filter(|token| token.chars().count() >= minimum_token_chars)
        .map(|token| format!("\"{}\"", token.replace('"', " ")))
        .collect::<Vec<_>>();
    (!tokens.is_empty()).then(|| FtsMatchQueries {
        primary: tokens.join(" "),
        fallback: (tokens.len() > 1).then(|| tokens.join(" OR ")),
    })
}
