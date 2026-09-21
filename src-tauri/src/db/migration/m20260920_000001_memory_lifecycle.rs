use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in TABLES {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Memory lifecycle audit data cannot be downgraded automatically".into(),
        ))
    }
}

const TABLES: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS memory_forget_tombstone (root_key TEXT NOT NULL, source_hash TEXT NOT NULL, created_at TEXT NOT NULL, reason TEXT NOT NULL, PRIMARY KEY(root_key,source_hash))",
    "CREATE TABLE IF NOT EXISTS memory_recall_feedback (id INTEGER PRIMARY KEY AUTOINCREMENT, root_key TEXT NOT NULL, receipt_id INTEGER NOT NULL, record_id TEXT NOT NULL, verdict TEXT NOT NULL CHECK(verdict IN ('used','irrelevant','outdated')), note TEXT, conversation_id TEXT, turn_nonce INTEGER, recorded_at TEXT NOT NULL, UNIQUE(root_key,receipt_id,record_id))",
    "CREATE INDEX IF NOT EXISTS idx_memory_feedback_record ON memory_recall_feedback(root_key,record_id,recorded_at DESC)",
];
