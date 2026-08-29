use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, Value};

use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::index_checkpoint::database_error;

pub(super) async fn execute<C: ConnectionTrait, const N: usize>(
    conn: &C,
    sql: &str,
    values: [Value; N],
) -> Result<sea_orm::ExecResult, AppCommandError> {
    conn.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map_err(database_error)
}

pub(super) async fn query_one<C: ConnectionTrait, const N: usize>(
    conn: &C,
    sql: &str,
    values: [Value; N],
) -> Result<Option<QueryResult>, AppCommandError> {
    conn.query_one(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map_err(database_error)
}

pub(super) async fn query_all<C: ConnectionTrait, const N: usize>(
    conn: &C,
    sql: &str,
    values: [Value; N],
) -> Result<Vec<QueryResult>, AppCommandError> {
    conn.query_all(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map_err(database_error)
}

pub(super) fn agent_name(agent: AgentType) -> String {
    serde_json::to_value(agent)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}
