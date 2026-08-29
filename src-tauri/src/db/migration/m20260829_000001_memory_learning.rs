use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(HARVEST_OUTBOX).await?;
        db.execute_unprepared(TASK_PROJECTION).await?;
        for sql in INDEXES {
            db.execute_unprepared(sql).await?;
        }
        if let Err(error) = db.execute_unprepared(TASK_FTS).await {
            tracing::info!(error = %error, "optional task history FTS lane is unavailable");
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS session_task_fts")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS session_task_projection")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS memory_harvest_outbox")
            .await?;
        Ok(())
    }
}

const HARVEST_OUTBOX: &str = r#"CREATE TABLE IF NOT EXISTS memory_harvest_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    dedup_key TEXT NOT NULL UNIQUE,
    conversation_id TEXT NOT NULL,
    turn_nonce INTEGER NOT NULL,
    agent_type TEXT NOT NULL,
    workspace_key TEXT,
    stop_reason TEXT,
    user_input_ref TEXT,
    assistant_input_ref TEXT,
    tool_outcome_ref TEXT,
    submitted_at TEXT NOT NULL,
    state TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    failure_kind TEXT,
    failure_detail TEXT,
    noop_reason TEXT,
    candidate_ids TEXT,
    experience_ids TEXT,
    processed_at TEXT,
    processing_ms INTEGER,
    updated_at TEXT NOT NULL,
    CHECK (state IN ('queued','extracting','proposed','noop','failed','dead'))
)"#;

const TASK_PROJECTION: &str = r#"CREATE TABLE IF NOT EXISTS session_task_projection (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    turn_generation INTEGER NOT NULL,
    agent_type TEXT NOT NULL,
    workspace_key TEXT,
    title TEXT,
    intent TEXT NOT NULL,
    result TEXT,
    decisions TEXT,
    failures TEXT,
    pending_items TEXT,
    status TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    UNIQUE (conversation_id, turn_generation)
)"#;

const INDEXES: [&str; 3] = [
    "CREATE INDEX IF NOT EXISTS idx_memory_harvest_state ON memory_harvest_outbox (state, next_attempt_at, id)",
    "CREATE INDEX IF NOT EXISTS idx_memory_harvest_conversation ON memory_harvest_outbox (conversation_id, turn_nonce)",
    "CREATE INDEX IF NOT EXISTS idx_session_task_projection_time ON session_task_projection (occurred_at DESC, conversation_id, turn_generation)",
];

const TASK_FTS: &str = r#"CREATE VIRTUAL TABLE IF NOT EXISTS session_task_fts USING fts5(
    conversation_id UNINDEXED,
    turn_generation UNINDEXED,
    title,
    intent,
    result,
    decisions,
    failures,
    pending_items,
    tokenize='unicode61'
)"#;
