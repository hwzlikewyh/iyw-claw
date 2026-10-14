use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in STATEMENTS {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        _manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS intelligent_ui_state")
            .await?;
        Ok(())
    }
}

const STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS intelligent_ui_state (conversation_id INTEGER NOT NULL, turn_id TEXT NOT NULL, block_id TEXT NOT NULL, source_hash TEXT NOT NULL, state_json TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1, updated_at TEXT NOT NULL, PRIMARY KEY(conversation_id,block_id,source_hash), FOREIGN KEY(conversation_id) REFERENCES conversation(id) ON DELETE CASCADE)",
    "CREATE INDEX IF NOT EXISTS idx_intelligent_ui_state_source ON intelligent_ui_state(conversation_id,source_hash)",
    "CREATE UNIQUE INDEX IF NOT EXISTS uq_intelligent_ui_state_source ON intelligent_ui_state(conversation_id,block_id,source_hash)",
    "CREATE TRIGGER IF NOT EXISTS intelligent_ui_state_limit BEFORE INSERT ON intelligent_ui_state WHEN (SELECT COUNT(*) FROM intelligent_ui_state WHERE conversation_id=NEW.conversation_id)>=256 AND NOT EXISTS (SELECT 1 FROM intelligent_ui_state WHERE conversation_id=NEW.conversation_id AND block_id=NEW.block_id AND source_hash=NEW.source_hash) BEGIN SELECT RAISE(ABORT,'intelligent UI state limit reached'); END",
];
