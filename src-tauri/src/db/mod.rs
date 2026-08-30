pub mod entities;
pub mod error;
pub mod migration;
pub mod restore_memory;
pub mod service;

use std::future::Future;
use std::path::Path;
use std::time::{Duration, Instant};

use sea_orm::sqlx::sqlite::{SqliteJournalMode, SqliteSynchronous};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;

use error::DbError;
use migration::Migrator;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(3);
const SQLITE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SQLITE_RUNTIME_MAX_CONNECTIONS: u32 = 3;
const SQLITE_STATEMENT_CACHE_CAPACITY: usize = 256;
const SQLITE_CACHE_SIZE_KIB: i32 = -32_768;
const SQLITE_WAL_AUTOCHECKPOINT_PAGES: i32 = 1_000;
const SQLITE_MMAP_SIZE_BYTES: i64 = 67_108_864;
const SQLITE_MIGRATION_RETRIES: usize = 3;
const SQLITE_MIGRATION_RETRY_BASE: Duration = Duration::from_millis(250);
const SQLITE_MAINTENANCE_RETRIES: usize = 2;
const SQLITE_MAINTENANCE_RETRY_BASE: Duration = Duration::from_millis(150);

pub struct AppDatabase {
    pub conn: DatabaseConnection,
}

pub struct DatabaseInitialization {
    pub database: AppDatabase,
    pub restore_memory: restore_memory::RestoreMemoryStartup,
}

pub(crate) fn database_file_name() -> &'static str {
    if cfg!(all(debug_assertions, feature = "tauri-runtime")) {
        "iyw-claw-dev.db"
    } else {
        "iyw-claw.db"
    }
}

pub async fn init_database(
    app_data_dir: impl AsRef<Path>,
    app_version: &str,
) -> Result<AppDatabase, DbError> {
    init_database_inner(app_data_dir, app_version, UserMemoryRestoreRoot::Legacy)
        .await
        .map(|initialized| initialized.database)
}

pub async fn init_database_with_user_memory_root(
    app_data_dir: impl AsRef<Path>,
    app_version: &str,
    user_memory_root: Option<&Path>,
) -> Result<AppDatabase, DbError> {
    init_database_inner(
        app_data_dir,
        app_version,
        UserMemoryRestoreRoot::Resolved(user_memory_root),
    )
    .await
    .map(|initialized| initialized.database)
}

pub async fn init_database_with_restore_state(
    app_data_dir: impl AsRef<Path>,
    app_version: &str,
    user_memory_root: Option<&Path>,
) -> Result<DatabaseInitialization, DbError> {
    init_database_inner(
        app_data_dir,
        app_version,
        UserMemoryRestoreRoot::Resolved(user_memory_root),
    )
    .await
}

enum UserMemoryRestoreRoot<'a> {
    Legacy,
    Resolved(Option<&'a Path>),
}

async fn init_database_inner(
    app_data_dir: impl AsRef<Path>,
    app_version: &str,
    user_memory_root: UserMemoryRestoreRoot<'_>,
) -> Result<DatabaseInitialization, DbError> {
    let app_data_dir = app_data_dir.as_ref();
    std::fs::create_dir_all(app_data_dir)?;
    let restore_source_changed = apply_pending_restore(app_data_dir, user_memory_root)?;
    crate::commands::backup::restore::cleanup_transient_dirs(app_data_dir);
    let db_url = database_url(app_data_dir);
    migrate_database(&db_url).await?;
    let conn = connect_runtime_database(db_url).await?;
    service::app_metadata_service::update_app_version(&conn, app_version).await?;
    let restore_memory =
        restore_memory::record_restore_source_changed(&conn, app_data_dir, restore_source_changed)
            .await;
    Ok(DatabaseInitialization {
        database: AppDatabase { conn },
        restore_memory,
    })
}

fn apply_pending_restore(
    app_data_dir: &Path,
    user_memory_root: UserMemoryRestoreRoot<'_>,
) -> Result<bool, DbError> {
    use crate::commands::backup::restore::RestoreApplied;

    let result = match user_memory_root {
        UserMemoryRestoreRoot::Legacy => {
            crate::commands::backup::restore::apply_pending_restore_on_startup(app_data_dir)
        }
        UserMemoryRestoreRoot::Resolved(root) => {
            crate::commands::backup::restore::apply_pending_restore_on_startup_with_root(
                app_data_dir,
                root,
            )
        }
    };
    match result.map_err(DbError::Io)? {
        RestoreApplied::Applied {
            restore_source_changed,
            ..
        } => Ok(restore_source_changed),
        RestoreApplied::None => Ok(false),
    }
}

fn database_url(app_data_dir: &Path) -> String {
    let db_path = app_data_dir.join(database_file_name());
    format!(
        "sqlite:{}?mode=rwc",
        urlencoding::encode(&db_path.to_string_lossy())
    )
}

async fn migrate_database(db_url: &str) -> Result<(), DbError> {
    let started = Instant::now();
    for attempt in 0..=SQLITE_MIGRATION_RETRIES {
        match migrate_database_once(db_url).await {
            Ok(()) => {
                if attempt > 0 {
                    tracing::info!(
                        attempt,
                        elapsed_ms = started.elapsed().as_millis(),
                        "SQLite migration recovered after lock contention"
                    );
                }
                return Ok(());
            }
            Err(error) if attempt < SQLITE_MIGRATION_RETRIES && is_sqlite_busy(&error) => {
                let delay = SQLITE_MIGRATION_RETRY_BASE * 2_u32.pow(attempt as u32);
                tracing::warn!(
                    attempt = attempt + 1,
                    max_retries = SQLITE_MIGRATION_RETRIES,
                    delay_ms = delay.as_millis(),
                    error = %error,
                    "SQLite migration is locked; retrying"
                );
                tokio::time::sleep(delay).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("SQLite migration retry loop always returns")
}

async fn migrate_database_once(db_url: &str) -> Result<(), DbError> {
    // A single connection observes every DDL change in migration order.
    let mut migrate_opts = ConnectOptions::new(db_url);
    migrate_opts
        .max_connections(1)
        .min_connections(1)
        .connect_timeout(SQLITE_CONNECT_TIMEOUT)
        .sqlx_logging(false);
    configure_sqlite_connections(&mut migrate_opts);
    let migrate_conn = Database::connect(migrate_opts).await?;
    apply_migrations(&migrate_conn).await?;
    migrate_conn.close().await.map_err(DbError::from)
}

async fn connect_runtime_database(db_url: String) -> Result<DatabaseConnection, DbError> {
    let mut opts = ConnectOptions::new(db_url);
    opts.max_connections(SQLITE_RUNTIME_MAX_CONNECTIONS)
        .min_connections(1)
        .connect_timeout(SQLITE_CONNECT_TIMEOUT)
        .idle_timeout(Duration::from_secs(300))
        .sqlx_logging(false);
    configure_sqlite_connections(&mut opts);
    Database::connect(opts).await.map_err(DbError::from)
}

async fn apply_migrations(conn: &DatabaseConnection) -> Result<(), DbError> {
    execute_sql(conn, "BEGIN IMMEDIATE;").await?;
    if let Err(error) = Migrator::up(conn, None).await {
        rollback_migration(conn, &error.to_string()).await;
        return Err(DbError::Migration(error.to_string()));
    }

    if let Err(error) = execute_sql(conn, "COMMIT;").await {
        rollback_migration(conn, &error.to_string()).await;
        return Err(DbError::Migration(format!("commit failed: {error}")));
    }
    Ok(())
}

async fn rollback_migration(conn: &DatabaseConnection, cause: &str) {
    if let Err(error) = execute_sql(conn, "ROLLBACK;").await {
        tracing::error!(
            cause,
            rollback_error = %error,
            "failed to roll back SQLite migration transaction"
        );
    }
}

async fn execute_sql(conn: &DatabaseConnection, sql: &str) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_string(DbBackend::Sqlite, sql.to_owned()))
        .await
        .map(|_| ())
}

/// Configure every physical SQLite connection opened by this pool. WAL persists
/// in the database header; the other settings are connection-local.
fn configure_sqlite_connections(options: &mut ConnectOptions) {
    options.map_sqlx_sqlite_opts(|sqlite| {
        sqlite
            .statement_cache_capacity(SQLITE_STATEMENT_CACHE_CAPACITY)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .pragma("cache_size", SQLITE_CACHE_SIZE_KIB.to_string())
            .pragma("temp_store", "MEMORY")
            .pragma(
                "wal_autocheckpoint",
                SQLITE_WAL_AUTOCHECKPOINT_PAGES.to_string(),
            )
            .pragma("mmap_size", SQLITE_MMAP_SIZE_BYTES.to_string())
    });
}

fn is_sqlite_busy(error: &DbError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("database is locked")
        || message.contains("database table is locked")
        || message.contains("database schema is locked")
        || message.contains("database is busy")
        || message.contains("database or disk is busy")
        || message.contains("(code: 5)")
        || message.contains("(code: 6)")
        || message.contains("(code: 517)")
}

/// Retry an idempotent maintenance transaction after a transient SQLite lock.
/// Callers must keep the operation transaction-scoped so a failed attempt can
/// be safely replayed after rollback.
pub(crate) async fn retry_sqlite_maintenance<T, F, Fut>(
    operation: &'static str,
    mut task: F,
) -> Result<T, DbError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, DbError>>,
{
    for attempt in 0..=SQLITE_MAINTENANCE_RETRIES {
        match task().await {
            Ok(value) => return Ok(value),
            Err(error) if attempt < SQLITE_MAINTENANCE_RETRIES && is_sqlite_busy(&error) => {
                let delay = SQLITE_MAINTENANCE_RETRY_BASE * 2_u32.pow(attempt as u32);
                tracing::warn!(
                    operation,
                    attempt = attempt + 1,
                    max_retries = SQLITE_MAINTENANCE_RETRIES,
                    delay_ms = delay.as_millis(),
                    error = %error,
                    "SQLite maintenance transaction is locked; retrying"
                );
                tokio::time::sleep(delay).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("SQLite maintenance retry loop always returns")
}
