pub mod entities;
pub mod error;
pub mod migration;
pub mod service;

use std::path::Path;
use std::time::Duration;

use sea_orm::sqlx::sqlite::{SqliteJournalMode, SqliteSynchronous};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;

use error::DbError;
use migration::Migrator;

pub struct AppDatabase {
    pub conn: DatabaseConnection,
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
    init_database_inner(app_data_dir, app_version, UserMemoryRestoreRoot::Legacy).await
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
}

enum UserMemoryRestoreRoot<'a> {
    Legacy,
    Resolved(Option<&'a Path>),
}

async fn init_database_inner(
    app_data_dir: impl AsRef<Path>,
    app_version: &str,
    user_memory_root: UserMemoryRestoreRoot<'_>,
) -> Result<AppDatabase, DbError> {
    let app_data_dir = app_data_dir.as_ref();
    std::fs::create_dir_all(app_data_dir)?;

    // Apply any pending restore BEFORE opening a connection — swapping
    // `iyw-claw.db` under a live SQLite handle would corrupt it. A failure here
    // aborts startup loudly (leaving the safety snapshot intact) rather than
    // booting a half-restored data dir.
    let restore = match user_memory_root {
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
    match restore {
        Ok(crate::commands::backup::restore::RestoreApplied::Applied { .. }) => {}
        Ok(crate::commands::backup::restore::RestoreApplied::None) => {}
        Err(e) => return Err(DbError::Io(e)),
    }
    crate::commands::backup::restore::cleanup_transient_dirs(app_data_dir);

    let db_path = app_data_dir.join(database_file_name());
    let db_url = format!(
        "sqlite:{}?mode=rwc",
        urlencoding::encode(&db_path.to_string_lossy())
    );

    // Apply migrations on a dedicated single connection. The runtime pool below
    // keeps several connections open for read concurrency, but sea-orm spreads a
    // migration's statements across whichever pooled connections are free. A
    // statement that references a column an earlier migration just added (e.g.
    // the `is_chat` → `kind` backfill) can then land on a connection whose
    // cached SQLite schema predates the `ALTER TABLE`, producing a flaky
    // `no such column: "is_chat"` under load. One connection observes every DDL
    // change in order, so the schema it compiles against is always current.
    let mut migrate_opts = ConnectOptions::new(db_url.clone());
    migrate_opts
        .max_connections(1)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(10))
        .sqlx_logging(false);
    configure_sqlite_connections(&mut migrate_opts);
    let migrate_conn = Database::connect(migrate_opts).await?;
    apply_migrations(&migrate_conn).await?;
    migrate_conn.close().await?;

    // Runtime connection pool. Migrations are already applied above, so the
    // schema is stable and spreading queries across pooled connections is safe.
    let mut opts = ConnectOptions::new(db_url);
    opts.max_connections(5)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(300))
        .sqlx_logging(false);
    configure_sqlite_connections(&mut opts);
    let conn = Database::connect(opts).await?;

    service::app_metadata_service::update_app_version(&conn, app_version).await?;

    Ok(AppDatabase { conn })
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
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5))
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .pragma("cache_size", "-8000")
    });
}
