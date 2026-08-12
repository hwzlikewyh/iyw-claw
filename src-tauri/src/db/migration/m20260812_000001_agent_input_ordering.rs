use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

const SORT_INDEX_STEP: i64 = 1024;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_columns(manager).await?;
        backfill_sort_index(manager).await?;
        create_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_indexes(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(AgentInputOutbox::Table)
                    .drop_column(AgentInputOutbox::ForceRequestedAt)
                    .drop_column(AgentInputOutbox::ForceBatchId)
                    .drop_column(AgentInputOutbox::SortIndex)
                    .to_owned(),
            )
            .await
    }
}

async fn add_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(AgentInputOutbox::Table)
                .add_column(
                    ColumnDef::new(AgentInputOutbox::SortIndex)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .add_column(ColumnDef::new(AgentInputOutbox::ForceBatchId).string())
                .add_column(
                    ColumnDef::new(AgentInputOutbox::ForceRequestedAt).timestamp_with_time_zone(),
                )
                .to_owned(),
        )
        .await
}

async fn backfill_sort_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let sql = format!(
        "UPDATE agent_input_outbox SET sort_index = (\
         SELECT COUNT(*) FROM agent_input_outbox AS earlier \
         WHERE earlier.conversation_id = agent_input_outbox.conversation_id AND (\
           earlier.created_at < agent_input_outbox.created_at OR \
           (earlier.created_at = agent_input_outbox.created_at AND earlier.id < agent_input_outbox.id)\
         )) * {SORT_INDEX_STEP}"
    );
    manager
        .get_connection()
        .execute(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    Ok(())
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_agent_input_outbox_conversation_status_order")
                .table(AgentInputOutbox::Table)
                .col(AgentInputOutbox::ConversationId)
                .col(AgentInputOutbox::Status)
                .col(AgentInputOutbox::SortIndex)
                .col(AgentInputOutbox::CreatedAt)
                .col(AgentInputOutbox::Id)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_agent_input_outbox_force_order")
                .table(AgentInputOutbox::Table)
                .col(AgentInputOutbox::ForceBatchId)
                .col(AgentInputOutbox::SortIndex)
                .to_owned(),
        )
        .await
}

async fn drop_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for name in [
        "idx_agent_input_outbox_force_order",
        "idx_agent_input_outbox_conversation_status_order",
    ] {
        manager
            .drop_index(
                Index::drop()
                    .name(name)
                    .table(AgentInputOutbox::Table)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

#[derive(Iden)]
enum AgentInputOutbox {
    Table,
    Id,
    ConversationId,
    Status,
    SortIndex,
    ForceBatchId,
    ForceRequestedAt,
    CreatedAt,
}
