use sea_orm_migration::prelude::*;

/// Task 08 (IYW-CHANNEL): message-channel end-to-end reliability.
///
/// Splits "desired enabled" from "runtime connected" by persisting the last
/// known runtime state on each channel, so the UI can show "已启用但未连接"
/// instead of a single conflated boolean. Also stamps every message-log row
/// with a trace id (one id spanning inbound → dispatcher → agent → outbound)
/// and the provider's message id (outbound idempotency / duplicate
/// protection).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE chat_channel ADD COLUMN runtime_status TEXT NOT NULL DEFAULT 'saved'",
        )
        .await?;
        db.execute_unprepared("ALTER TABLE chat_channel ADD COLUMN last_error TEXT")
            .await?;
        db.execute_unprepared("ALTER TABLE chat_channel ADD COLUMN last_error_at DATETIME")
            .await?;
        db.execute_unprepared("ALTER TABLE chat_channel ADD COLUMN last_connected_at DATETIME")
            .await?;
        db.execute_unprepared("ALTER TABLE chat_channel_message_log ADD COLUMN trace_id TEXT")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE chat_channel_message_log ADD COLUMN provider_message_id TEXT",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_chat_channel_message_log_trace ON \
             chat_channel_message_log (trace_id)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite cannot drop columns in older versions; migrations are
        // forward-only per the distribution plan.
        Ok(())
    }
}
