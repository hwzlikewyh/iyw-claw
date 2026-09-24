use std::path::{Path, PathBuf};

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    TransactionTrait,
};

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

const LOCAL_KEYS: [&str; 2] = [
    crate::acp::agent_storage::STORAGE_METADATA_KEY,
    "desktop.install_root.v1",
];

pub(super) async fn open(
    path: &Path,
    read_only: bool,
) -> Result<DatabaseConnection, AppCommandError> {
    let mode = if read_only { "ro" } else { "rw" };
    let mut options = ConnectOptions::new(format!(
        "sqlite:{}?mode={mode}",
        urlencoding::encode(&path.to_string_lossy())
    ));
    options.max_connections(1).sqlx_logging(false);
    Database::connect(options).await.map_err(database_error)
}

pub(super) async fn prepare(
    staging: &Path,
    data_dir: &Path,
    mappings: &[(String, PathBuf)],
) -> Result<(), AppCommandError> {
    let staged = open(&staging.join("db/iyw-claw.db"), false).await?;
    let result = prepare_database(&staged, data_dir, mappings).await;
    let closed = staged.close().await.map_err(database_error);
    result?;
    closed?;
    prepare_native_databases(staging, mappings).await
}

async fn prepare_native_databases(
    staging: &Path,
    mappings: &[(String, PathBuf)],
) -> Result<(), AppCommandError> {
    for (relative, fields) in [
        (
            "external/opencode/opencode.db",
            vec![("session", "directory"), ("project", "worktree")],
        ),
        (
            "external/hermes/state.db",
            vec![("sessions", "cwd"), ("sessions", "model_config")],
        ),
    ] {
        let path = staging.join(relative);
        if !path.is_file() {
            continue;
        }
        let conn = open(&path, false).await?;
        let result = relocate_native_fields(&conn, &fields, mappings).await;
        let closed = conn.close().await.map_err(database_error);
        result?;
        closed?;
    }
    Ok(())
}

async fn relocate_native_fields(
    conn: &DatabaseConnection,
    fields: &[(&str, &str)],
    mappings: &[(String, PathBuf)],
) -> Result<(), AppCommandError> {
    let transaction = conn.begin().await.map_err(database_error)?;
    for field in fields {
        relocate_column(&transaction, *field, mappings).await?;
    }
    transaction.commit().await.map_err(database_error)?;
    conn.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA wal_checkpoint(TRUNCATE)",
    ))
    .await
    .map_err(database_error)?;
    Ok(())
}

async fn prepare_database(
    staged: &DatabaseConnection,
    data_dir: &Path,
    mappings: &[(String, PathBuf)],
) -> Result<(), AppCommandError> {
    preserve_machine_settings(staged, data_dir).await?;
    let transaction = staged.begin().await.map_err(database_error)?;
    relocate_column(&transaction, ("folder", "path"), mappings).await?;
    for column in ["path", "source_path"] {
        relocate_column(&transaction, ("task_artifact", column), mappings).await?;
    }
    transaction.commit().await.map_err(database_error)?;
    staged
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA wal_checkpoint(TRUNCATE)",
        ))
        .await
        .map_err(database_error)?;
    tracing::info!(
        mapped_roots = mappings.len(),
        "[RESTORE] staged database paths relocated; destination storage settings retained"
    );
    Ok(())
}

async fn preserve_machine_settings(
    staged: &DatabaseConnection,
    data_dir: &Path,
) -> Result<(), AppCommandError> {
    let current = open(&data_dir.join(crate::db::database_file_name()), true).await?;
    let result = copy_machine_settings(staged, &current).await;
    let closed = current.close().await.map_err(database_error);
    result?;
    closed
}

async fn copy_machine_settings(
    staged: &DatabaseConnection,
    current: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    for key in LOCAL_KEYS {
        let value = app_metadata_service::get_value(current, key)
            .await
            .map_err(AppCommandError::from)?;
        staged
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "DELETE FROM app_metadata WHERE key = ?",
                [key.into()],
            ))
            .await
            .map_err(database_error)?;
        if let Some(value) = value {
            app_metadata_service::upsert_value(staged, key, &value)
                .await
                .map_err(AppCommandError::from)?;
        }
    }
    Ok(())
}

async fn relocate_column(
    conn: &impl ConnectionTrait,
    field: (&str, &str),
    mappings: &[(String, PathBuf)],
) -> Result<(), AppCommandError> {
    let (table, column) = field;
    if mappings.is_empty() || !column_exists(conn, field).await? {
        return Ok(());
    }
    // 表和列只取本模块常量，值全部使用绑定参数。
    let rows = conn
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("SELECT rowid AS row_id, {column} AS value FROM {table}"),
        ))
        .await
        .map_err(database_error)?;
    for row in rows {
        let id: i64 = row.try_get("", "row_id").map_err(database_error)?;
        let Some(value) = row
            .try_get::<Option<String>>("", "value")
            .map_err(database_error)?
        else {
            continue;
        };
        let Some(next) = rebase_value(&value, mappings).filter(|next| next != &value) else {
            continue;
        };
        conn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("UPDATE {table} SET {column} = ? WHERE rowid = ?"),
            [next.into(), id.into()],
        ))
        .await
        .map_err(database_error)?;
    }
    Ok(())
}

async fn column_exists(
    conn: &impl ConnectionTrait,
    field: (&str, &str),
) -> Result<bool, AppCommandError> {
    let (table, column) = field;
    let columns = conn
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA table_info({table})"),
        ))
        .await
        .map_err(database_error)?;
    Ok(columns.iter().any(|row| {
        row.try_get::<String>("", "name")
            .is_ok_and(|name| name == column)
    }))
}

fn rebase_value(value: &str, mappings: &[(String, PathBuf)]) -> Option<String> {
    if let Some(next) = super::portable::rebase(value, mappings) {
        return Some(next);
    }
    let mut json: serde_json::Value = serde_json::from_str(value).ok()?;
    super::portable_transcripts::relocate_value(&mut json, mappings)
        .then(|| serde_json::to_string(&json).ok())
        .flatten()
}

fn database_error(error: sea_orm::DbErr) -> AppCommandError {
    AppCommandError::database_error("Prepare portable backup database")
        .with_detail(error.to_string())
}
