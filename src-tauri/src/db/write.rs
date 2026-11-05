use sea_orm::{DatabaseConnection, DatabaseTransaction, SqliteTransactionMode};
use sea_orm::{TransactionOptions, TransactionTrait};

use super::{error::DbError, retry_sqlite_maintenance};

pub(crate) async fn begin_sqlite_write(
    conn: &DatabaseConnection,
) -> Result<DatabaseTransaction, DbError> {
    // 在读取前取得写锁，避免 WAL 快照在升级为写事务时立即失败。
    retry_sqlite_maintenance("begin_sqlite_write", || async {
        conn.begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
        .map_err(DbError::from)
    })
    .await
}
