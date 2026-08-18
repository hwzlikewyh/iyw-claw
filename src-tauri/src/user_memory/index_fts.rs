use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::index_integrity::check_fts_integrity;

const CREATE_UNICODE_FTS: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS memory_item_fts_unicode USING fts5(content, content='memory_item_current', content_rowid='row_id', tokenize='unicode61')";
const CREATE_TRIGRAM_FTS: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS memory_item_fts_trigram USING fts5(content, content='memory_item_current', content_rowid='row_id', tokenize='trigram case_sensitive 0')";
const REBUILD_UNICODE_FTS: &str =
    "INSERT INTO memory_item_fts_unicode(memory_item_fts_unicode) VALUES ('rebuild')";
const REBUILD_TRIGRAM_FTS: &str =
    "INSERT INTO memory_item_fts_trigram(memory_item_fts_trigram) VALUES ('rebuild')";
const DROP_UNICODE_FTS: &str = "DROP TABLE IF EXISTS memory_item_fts_unicode";
const DROP_TRIGRAM_FTS: &str = "DROP TABLE IF EXISTS memory_item_fts_trigram";

#[derive(Clone, Copy)]
pub(super) enum FtsLane {
    Unicode,
    Trigram,
}

impl FtsLane {
    fn name(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Trigram => "trigram",
        }
    }

    fn table(self) -> &'static str {
        match self {
            Self::Unicode => "memory_item_fts_unicode",
            Self::Trigram => "memory_item_fts_trigram",
        }
    }

    fn create_sql(self) -> &'static str {
        match self {
            Self::Unicode => CREATE_UNICODE_FTS,
            Self::Trigram => CREATE_TRIGRAM_FTS,
        }
    }

    fn rebuild_sql(self) -> &'static str {
        match self {
            Self::Unicode => REBUILD_UNICODE_FTS,
            Self::Trigram => REBUILD_TRIGRAM_FTS,
        }
    }

    fn drop_sql(self) -> &'static str {
        match self {
            Self::Unicode => DROP_UNICODE_FTS,
            Self::Trigram => DROP_TRIGRAM_FTS,
        }
    }
}

pub(super) async fn rebuild_fts<C: ConnectionTrait>(conn: &C, lane: FtsLane) -> String {
    if let Err(error) = execute(conn, lane.create_sql()).await {
        tracing::warn!(
            lane = lane.name(),
            error = %error,
            "[memory-index] FTS lane creation failed"
        );
        return "unavailable".to_string();
    }
    match rebuild_and_check(conn, lane).await {
        Ok(()) => "ready".to_string(),
        Err((status, error)) => repair_fts(conn, lane, status, error).await,
    }
}

async fn rebuild_and_check<C: ConnectionTrait>(
    conn: &C,
    lane: FtsLane,
) -> Result<(), (&'static str, sea_orm::DbErr)> {
    execute(conn, lane.rebuild_sql())
        .await
        .map_err(|error| ("error", error))?;
    check_fts_integrity(conn, lane.table())
        .await
        .map_err(|error| ("integrity_error", error))
}

async fn repair_fts<C: ConnectionTrait>(
    conn: &C,
    lane: FtsLane,
    original_status: &'static str,
    original_error: sea_orm::DbErr,
) -> String {
    tracing::warn!(
        lane = lane.name(),
        status = original_status,
        error = %original_error,
        "[memory-index] FTS lane is unhealthy; recreating derived table"
    );
    if let Err(error) = execute(conn, lane.drop_sql()).await {
        tracing::warn!(lane = lane.name(), error = %error, "[memory-index] FTS repair drop failed");
        return original_status.to_string();
    }
    if let Err(error) = execute(conn, lane.create_sql()).await {
        tracing::warn!(lane = lane.name(), error = %error, "[memory-index] FTS repair create failed");
        return "unavailable".to_string();
    }
    match rebuild_and_check(conn, lane).await {
        Ok(()) => "ready".to_string(),
        Err((status, error)) => {
            tracing::warn!(lane = lane.name(), status, error = %error, "[memory-index] FTS repair verification failed");
            status.to_string()
        }
    }
}

async fn execute<C: ConnectionTrait>(conn: &C, sql: &'static str) -> Result<(), sea_orm::DbErr> {
    conn.execute(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement, TransactionTrait};

    use super::{rebuild_fts, FtsLane};

    #[tokio::test]
    async fn missing_unicode_table_is_created_and_rebuilt() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE memory_item_current (row_id INTEGER PRIMARY KEY, content TEXT NOT NULL)",
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "INSERT INTO memory_item_current (row_id, content) VALUES (1, 'recoverable memory')",
        )
        .await
        .unwrap();

        let txn = db.begin().await.unwrap();
        assert_eq!(rebuild_fts(&txn, FtsLane::Unicode).await, "ready");
        assert!(txn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT rowid FROM memory_item_fts_unicode WHERE memory_item_fts_unicode MATCH 'recoverable'",
            ))
            .await
            .unwrap()
            .is_some());
        txn.commit().await.unwrap();
    }

    #[tokio::test]
    async fn invalid_unicode_table_is_recreated_as_fts() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE memory_item_current (row_id INTEGER PRIMARY KEY, content TEXT NOT NULL)",
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "INSERT INTO memory_item_current (row_id, content) VALUES (1, 'repairable memory')",
        )
        .await
        .unwrap();
        db.execute_unprepared("CREATE TABLE memory_item_fts_unicode (content TEXT NOT NULL)")
            .await
            .unwrap();

        let txn = db.begin().await.unwrap();
        assert_eq!(rebuild_fts(&txn, FtsLane::Unicode).await, "ready");
        assert!(txn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT rowid FROM memory_item_fts_unicode WHERE memory_item_fts_unicode MATCH 'repairable'",
            ))
            .await
            .unwrap()
            .is_some());
        txn.commit().await.unwrap();
    }
}
