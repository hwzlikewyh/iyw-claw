use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    TransactionTrait,
};

use super::{rebase, rebase_toml, write_atomic};
use crate::models::AgentType;

const STATE_PATH_FIELDS: &[(&str, &str)] = &[
    ("threads", "rollout_path"),
    ("threads", "cwd"),
    ("rollout_migration_skipped_rollouts", "rollout_path"),
    ("agent_jobs", "input_csv_path"),
    ("agent_jobs", "output_csv_path"),
    ("project_roots", "path"),
];

pub(super) fn rebase_files(roots: (&Path, &Path)) -> Result<(), String> {
    rebase_file(&roots.1.join("config.toml"), roots)?;
    let agents = roots.1.join("agents");
    if !agents.is_dir() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(agents).follow_links(false) {
        let entry = entry.map_err(|e| format!("inspect Xinghe agent config: {e}"))?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "toml")
        {
            rebase_file(entry.path(), roots)?;
        }
    }
    Ok(())
}

fn rebase_file(path: &Path, roots: (&Path, &Path)) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("Xinghe configuration is not a regular file".into()),
        Err(error) => return Err(format!("inspect Xinghe configuration: {error}")),
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read Xinghe configuration: {e}"))?;
    if let Some(next) = rebase_toml(&raw, roots)? {
        let backup = path.with_extension("toml.pre-xinghe-migration");
        if !backup.exists() {
            write_atomic(&backup, raw.as_bytes())?;
        }
        write_atomic(path, next.as_bytes())?;
    }
    Ok(())
}

pub(super) async fn rebase_databases(
    roots: (&Path, &Path),
    database: Option<&DatabaseConnection>,
) -> Result<(), String> {
    let home = database_home(roots, database).await?;
    if !home.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&home).map_err(|e| format!("inspect Xinghe databases: {e}"))? {
        let entry = entry.map_err(|e| format!("inspect Xinghe database: {e}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("state_") && name.ends_with(".sqlite") {
            let metadata = entry
                .file_type()
                .map_err(|e| format!("inspect Xinghe database type: {e}"))?;
            if !metadata.is_file() {
                return Err("Xinghe state database is not a regular file".into());
            }
            rebase_database(&entry.path(), roots).await?;
        }
    }
    Ok(())
}

async fn database_home(
    roots: (&Path, &Path),
    database: Option<&DatabaseConnection>,
) -> Result<PathBuf, String> {
    let snapshot = if let Some(database) = database {
        crate::db::service::agent_setting_service::get_by_agent_type(database, AgentType::Codex)
            .await
            .map_err(db_error)?
            .as_ref()
            .and_then(|setting| {
                crate::acp::xinghe_runtime_config::stored_preferences(Some(setting))
            })
    } else {
        None
    };
    let raw = match snapshot {
        Some(raw) => raw,
        None => match fs::read_to_string(roots.1.join("config.toml")) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("read Xinghe SQLite configuration: {error}")),
        },
    };
    let config: toml::Value =
        toml::from_str(&raw).map_err(|_| "Invalid Xinghe SQLite configuration")?;
    let configured = config
        .get("sqlite_home")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("CODEX_SQLITE_HOME")
                .ok()
                .filter(|path| !path.is_empty())
        });
    let Some(configured) = configured else {
        return Ok(roots.1.to_path_buf());
    };
    let path = PathBuf::from(rebase(&configured, roots).unwrap_or(configured));
    Ok(if path.is_absolute() {
        path
    } else {
        roots.1.join(path)
    })
}

async fn rebase_database(path: &Path, roots: (&Path, &Path)) -> Result<(), String> {
    let mut options = ConnectOptions::new(format!(
        "sqlite:{}?mode=rw",
        urlencoding::encode(&path.to_string_lossy())
    ));
    options.max_connections(1).sqlx_logging(false);
    let conn = Database::connect(options).await.map_err(db_error)?;
    let result = async {
        let transaction = conn.begin().await.map_err(db_error)?;
        for field in STATE_PATH_FIELDS {
            rebase_column(&transaction, *field, roots).await?;
        }
        transaction.commit().await.map_err(db_error)?;
        conn.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA wal_checkpoint(TRUNCATE)",
        ))
        .await
        .map_err(db_error)?;
        Ok::<(), String>(())
    }
    .await;
    let closed = conn.close().await.map_err(db_error);
    result.and(closed)
}

async fn rebase_column(
    conn: &impl ConnectionTrait,
    field: (&str, &str),
    roots: (&Path, &Path),
) -> Result<(), String> {
    let (table, column) = field;
    let exists = conn
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT name FROM pragma_table_info(?) WHERE name = ?",
            [table.into(), column.into()],
        ))
        .await
        .map_err(db_error)?
        .is_some();
    if !exists {
        return Ok(());
    }
    // 表名和字段只来自上方固定列表，所有数据值使用绑定参数。
    let rows = conn
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("SELECT rowid AS row_id, {column} AS value FROM {table}"),
        ))
        .await
        .map_err(db_error)?;
    for row in rows {
        let id: i64 = row.try_get("", "row_id").map_err(db_error)?;
        let value: Option<String> = row.try_get("", "value").map_err(db_error)?;
        let Some(next) = value.as_deref().and_then(|value| rebase(value, roots)) else {
            continue;
        };
        conn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("UPDATE {table} SET {column} = ? WHERE rowid = ?"),
            [next.into(), id.into()],
        ))
        .await
        .map_err(db_error)?;
    }
    Ok(())
}

pub(super) async fn rebase_preferences(
    conn: &DatabaseConnection,
    roots: (&Path, &Path),
) -> Result<(), String> {
    use crate::db::entities::agent_setting::{Column, Entity};
    use crate::db::service::agent_setting_service;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let Some(setting) = agent_setting_service::get_by_agent_type(conn, AgentType::Codex)
        .await
        .map_err(db_error)?
    else {
        return Ok(());
    };
    let Some(raw) = setting.env_json else {
        return Ok(());
    };
    let Some(encoded) = rebased_environment(&raw, roots)? else {
        return Ok(());
    };
    let updated = Entity::update_many()
        .col_expr(Column::EnvJson, sea_orm::sea_query::Expr::value(encoded))
        .col_expr(
            Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now()),
        )
        .filter(Column::Id.eq(setting.id))
        .filter(Column::EnvJson.eq(raw))
        .exec(conn)
        .await
        .map_err(db_error)?;
    if updated.rows_affected != 1 {
        return Err("Xinghe preferences changed during migration; retry startup".into());
    }
    Ok(())
}

fn rebased_environment(raw: &str, roots: (&Path, &Path)) -> Result<Option<String>, String> {
    let mut env: BTreeMap<String, String> =
        serde_json::from_str(raw).map_err(|_| "Invalid stored Xinghe environment")?;
    let mut changed = false;
    for (key, value) in &mut env {
        let next = if key == crate::acp::xinghe_runtime_config::PREFERENCES_KEY {
            rebase_toml(value, roots)?
        } else {
            rebase(value, roots)
        };
        if let Some(next) = next {
            *value = next;
            changed = true;
        }
    }
    if !changed {
        return Ok(None);
    }
    serde_json::to_string(&env)
        .map(Some)
        .map_err(|_| "Cannot encode migrated Xinghe preferences".into())
}

fn db_error(error: impl std::fmt::Display) -> String {
    format!("Xinghe profile database path repair failed: {error}")
}
