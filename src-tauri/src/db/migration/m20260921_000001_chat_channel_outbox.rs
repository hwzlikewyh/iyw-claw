use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(CREATE_OUTBOX).await?;
        db.execute_unprepared(CREATE_INDEX).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ChatChannelOutbox::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

const CREATE_OUTBOX: &str = r#"CREATE TABLE IF NOT EXISTS chat_channel_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL,
    recipient_id TEXT NOT NULL,
    content TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'waiting_context',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (channel_id) REFERENCES chat_channel(id) ON DELETE CASCADE,
    CHECK (state IN ('waiting_context'))
)"#;

const CREATE_INDEX: &str =
    "CREATE INDEX IF NOT EXISTS idx_chat_channel_outbox_waiting \
     ON chat_channel_outbox (channel_id, recipient_id, state, id)";

#[derive(DeriveIden)]
enum ChatChannelOutbox {
    Table,
}
