use std::future::Future;
use std::time::Duration;

use sea_orm::{ConnAcquireErr, DatabaseConnection, DbErr};

use crate::db::error::DbError;

const RETRY_DELAY: Duration = Duration::from_millis(150);

/// 历史只读查询遇到短暂连接池拥塞时重试一次，持续失败仍交给调用方。
pub(super) async fn read<T, F, Fut>(
    conn: &DatabaseConnection,
    conversation_id: i32,
    mut query: F,
) -> Result<T, DbError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, DbError>>,
{
    match query().await {
        Err(DbError::Database(DbErr::ConnectionAcquire(ConnAcquireErr::Timeout))) => {
            let pool = conn.get_sqlite_connection_pool();
            tracing::warn!(
                conversation_id,
                pool_size = pool.size(),
                pool_idle = pool.num_idle(),
                "[conversation-history] database connection acquisition timed out; retrying read once"
            );
            tokio::time::sleep(RETRY_DELAY).await;
            query().await
        }
        result => result,
    }
}
