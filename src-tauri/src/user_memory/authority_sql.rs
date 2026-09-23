use super::authority_types::AuthoritySnapshot;
use crate::app_error::AppCommandError;
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, Value};

pub(super) async fn execute<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<Value>,
) -> Result<u64, AppCommandError> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map(|result| result.rows_affected())
    .map_err(super::index_checkpoint::database_error)
}

pub(super) async fn rows<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<Value>,
) -> Result<Vec<QueryResult>, AppCommandError> {
    db.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map_err(super::index_checkpoint::database_error)
}

pub(super) async fn load<C: ConnectionTrait>(
    db: &C,
    key: &str,
) -> Result<Option<AuthoritySnapshot>, AppCommandError> {
    let rows = rows(db, "SELECT store_id, mode, epoch, source_digest, backup_path, snapshot_json FROM memory_authority WHERE root_key = ?", vec![key.into()]).await?;
    rows.into_iter()
        .next()
        .map(|row| {
            let text: String = field(&row, "snapshot_json")?;
            let data = serde_json::from_str(&text).map_err(|_| {
                AppCommandError::configuration_invalid("Invalid memory authority snapshot")
            })?;
            super::authority_validation::validate(&data)?;
            let digest: String = field(&row, "source_digest")?;
            if super::authority_types::digest(&data)? != digest {
                return Err(AppCommandError::configuration_invalid(
                    "Memory authority digest mismatch",
                ));
            }
            Ok(AuthoritySnapshot {
                store_id: field(&row, "store_id")?,
                mode: field(&row, "mode")?,
                epoch: field(&row, "epoch")?,
                digest,
                backup_path: field(&row, "backup_path")?,
                data,
            })
        })
        .transpose()
}

pub(super) fn field<T: sea_orm::TryGetable>(
    row: &QueryResult,
    key: &str,
) -> Result<T, AppCommandError> {
    row.try_get("", key)
        .map_err(super::index_checkpoint::database_error)
}
