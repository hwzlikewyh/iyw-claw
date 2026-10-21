use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_outbox(manager, DELIVERY_STATES).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let pending = manager
            .get_connection()
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT 1 FROM chat_channel_outbox WHERE state <> 'waiting_context' LIMIT 1",
            ))
            .await?;
        if pending.is_some() {
            return Err(DbErr::Custom(
                "Channel deliveries exist; refusing to discard messages during downgrade".into(),
            ));
        }
        rebuild_outbox(manager, "'waiting_context'").await
    }
}

async fn rebuild_outbox(manager: &SchemaManager<'_>, states: &str) -> Result<(), DbErr> {
    let db = manager.get_connection();
    // 应用迁移入口在同一个 SQLite 事务中执行这些语句，失败时整体回滚。
    // states 仅来自本模块常量，不接受外部输入。
    db.execute_unprepared(&CREATE_OUTBOX.replace("{states}", states))
        .await?;
    for sql in REBUILD_OUTBOX {
        db.execute_unprepared(sql).await?;
    }
    tracing::info!("[channel-delivery] outbox state constraint migrated; queue records preserved");
    Ok(())
}

const DELIVERY_STATES: &str = "'waiting_context', 'delivery_pending', 'delivery_sending', \
    'delivery_context', 'delivery_unknown', 'delivery_blocked', 'delivery_sent'";

const CREATE_OUTBOX: &str = r#"CREATE TABLE chat_channel_outbox_delivery (
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
    CHECK (state IN ({states}))
)"#;

const REBUILD_OUTBOX: &[&str] = &[
    "INSERT INTO chat_channel_outbox_delivery \
     (id, channel_id, recipient_id, content, state, attempts, last_error, created_at, updated_at) \
     SELECT id, channel_id, recipient_id, content, state, attempts, last_error, created_at, updated_at \
     FROM chat_channel_outbox",
    // 即使原表已删空，也保留已分配主键的高水位，避免重新使用历史交付 ID。
    "INSERT INTO sqlite_sequence (name, seq) \
     SELECT 'chat_channel_outbox_delivery', 0 \
     WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'chat_channel_outbox_delivery')",
    "UPDATE sqlite_sequence SET seq = MAX(seq, COALESCE( \
     (SELECT MAX(seq) FROM sqlite_sequence WHERE name = 'chat_channel_outbox'), 0)) \
     WHERE name = 'chat_channel_outbox_delivery'",
    "DROP TABLE chat_channel_outbox",
    "ALTER TABLE chat_channel_outbox_delivery RENAME TO chat_channel_outbox",
    "CREATE INDEX idx_chat_channel_outbox_waiting \
     ON chat_channel_outbox (channel_id, recipient_id, state, id)",
];
