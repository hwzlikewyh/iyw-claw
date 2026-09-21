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
        Err(DbErr::Custom(
            "Conversation session lineage cannot be downgraded automatically".into(),
        ))
    }
}

const STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS conversation_session_segment (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id INTEGER NOT NULL, agent_type TEXT NOT NULL, external_id TEXT, ordinal INTEGER NOT NULL, predecessor_id INTEGER, history_mode TEXT NOT NULL CHECK(history_mode IN ('root','continuation','full_fork')), status TEXT NOT NULL CHECK(status IN ('pending','active','closed','failed')), boundary_generation INTEGER NOT NULL DEFAULT 0, recovery_attempt_id TEXT NOT NULL UNIQUE, context_digest TEXT, created_at TEXT NOT NULL, closed_at TEXT, FOREIGN KEY(conversation_id) REFERENCES conversation(id) ON DELETE CASCADE, FOREIGN KEY(predecessor_id) REFERENCES conversation_session_segment(id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_segment_ordinal ON conversation_session_segment(conversation_id,ordinal)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_segment_external ON conversation_session_segment(agent_type,external_id) WHERE external_id IS NOT NULL",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_segment_active ON conversation_session_segment(conversation_id) WHERE status='active'",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_segment_pending ON conversation_session_segment(conversation_id) WHERE status='pending'",
    "CREATE INDEX IF NOT EXISTS idx_conversation_segment_history ON conversation_session_segment(conversation_id,ordinal,status)",
    "INSERT OR IGNORE INTO conversation_session_segment (conversation_id,agent_type,external_id,ordinal,predecessor_id,history_mode,status,boundary_generation,recovery_attempt_id,context_digest,created_at,closed_at) SELECT id,agent_type,external_id,1,NULL,'root','active',last_completed_turn_generation,'legacy:' || id || ':' || agent_type || ':' || external_id,NULL,created_at,NULL FROM conversation WHERE external_id IS NOT NULL AND TRIM(external_id) <> ''",
];
