use std::borrow::Cow;

use sqlx::SqlitePool;
use sqlx::migrate::{Migration, Migrator};
use sqlx::{AssertSqlSafe, SqlSafeStr};

/// 仅接受同一 SQL 的换行编码差异；原迁移记录和其他校验和门禁保持不变。
pub(crate) async fn compatible_migrator(
    pool: &SqlitePool,
    base: &Migrator,
) -> anyhow::Result<Migrator> {
    let mut migrations = base.migrations.to_vec();
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    if exists {
        let applied: Vec<(i64, Vec<u8>)> =
            sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations WHERE success = 1")
                .fetch_all(pool)
                .await?;
        for migration in &mut migrations {
            if let Some((_, checksum)) = applied.iter().find(|(v, _)| *v == migration.version) {
                accept_line_ending_variant(migration, checksum);
            }
        }
    }
    Ok(Migrator {
        migrations: Cow::Owned(migrations),
        ignore_missing: base.ignore_missing,
        locking: base.locking,
        no_tx: base.no_tx,
        table_name: base.table_name.clone(),
        create_schemas: base.create_schemas.clone(),
    })
}

fn accept_line_ending_variant(migration: &mut Migration, applied: &[u8]) {
    if migration.checksum.as_ref() == applied {
        return;
    }
    let lf = migration.sql.as_str().replace("\r\n", "\n");
    for sql in [lf.clone(), lf.replace('\n', "\r\n")] {
        let variant = Migration::new(
            migration.version,
            migration.description.clone(),
            migration.migration_type,
            AssertSqlSafe(sql).into_sql_str(),
            migration.no_tx,
        );
        if variant.checksum.as_ref() == applied {
            tracing::warn!(
                migration_version = migration.version,
                "accepted migration checksum with equivalent SQL line endings"
            );
            migration.checksum = variant.checksum;
            return;
        }
    }
}
