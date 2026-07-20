use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The Telegram channel type was removed in favor of the WeCom (企业微信)
/// channel. Existing telegram rows would fail backend construction on every
/// auto-connect and show as an unknown type in the UI, so drop them together
/// with their per-conversation topic bindings.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "DELETE FROM chat_channel_thread_binding WHERE channel_id IN \
             (SELECT id FROM chat_channel WHERE channel_type = 'telegram')",
        )
        .await?;
        db.execute_unprepared(
            "DELETE FROM chat_channel_message_log WHERE channel_id IN \
             (SELECT id FROM chat_channel WHERE channel_type = 'telegram')",
        )
        .await?;
        db.execute_unprepared(
            "DELETE FROM chat_channel_sender_context WHERE channel_id IN \
             (SELECT id FROM chat_channel WHERE channel_type = 'telegram')",
        )
        .await?;
        db.execute_unprepared("DELETE FROM chat_channel WHERE channel_type = 'telegram'")
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Deleted rows are gone; nothing to restore.
        Ok(())
    }
}
