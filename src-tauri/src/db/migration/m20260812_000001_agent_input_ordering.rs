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
        drop_column(manager, AgentInputOutbox::ForceRequestedAt).await?;
        drop_column(manager, AgentInputOutbox::ForceBatchId).await?;
        drop_column(manager, AgentInputOutbox::SortIndex).await
    }
}

async fn add_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    add_column_if_missing(
        manager,
        "sort_index",
        ColumnDef::new(AgentInputOutbox::SortIndex)
            .big_integer()
            .not_null()
            .default(0)
            .to_owned(),
    )
    .await?;
    add_column_if_missing(
        manager,
        "force_batch_id",
        ColumnDef::new(AgentInputOutbox::ForceBatchId)
            .string()
            .to_owned(),
    )
    .await?;
    add_column_if_missing(
        manager,
        "force_requested_at",
        ColumnDef::new(AgentInputOutbox::ForceRequestedAt)
            .timestamp_with_time_zone()
            .to_owned(),
    )
    .await
}

async fn add_column_if_missing(
    manager: &SchemaManager<'_>,
    column_name: &str,
    column: ColumnDef,
) -> Result<(), DbErr> {
    if manager
        .has_column(AgentInputOutbox::Table.to_string(), column_name)
        .await?
    {
        tracing::info!(
            table = "agent_input_outbox",
            column = column_name,
            "skipping existing migration column"
        );
        return Ok(());
    }

    tracing::info!(
        table = "agent_input_outbox",
        column = column_name,
        "adding missing migration column"
    );
    manager
        .alter_table(
            Table::alter()
                .table(AgentInputOutbox::Table)
                .add_column(column)
                .to_owned(),
        )
        .await
}

async fn drop_column<T: IntoIden>(manager: &SchemaManager<'_>, column: T) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(AgentInputOutbox::Table)
                .drop_column(column)
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
         )) * {SORT_INDEX_STEP} \
         WHERE conversation_id IN (\
           SELECT conversation_id FROM agent_input_outbox \
           GROUP BY conversation_id \
           HAVING COUNT(*) > 1 AND MIN(sort_index) = 0 AND MAX(sort_index) = 0\
         )"
    );
    manager
        .get_connection()
        .execute(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    Ok(())
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_index_if_missing(
        manager,
        "idx_agent_input_outbox_conversation_status_order",
        Index::create()
            .if_not_exists()
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
    create_index_if_missing(
        manager,
        "idx_agent_input_outbox_force_order",
        Index::create()
            .if_not_exists()
            .name("idx_agent_input_outbox_force_order")
            .table(AgentInputOutbox::Table)
            .col(AgentInputOutbox::ForceBatchId)
            .col(AgentInputOutbox::SortIndex)
            .to_owned(),
    )
    .await
}

async fn create_index_if_missing(
    manager: &SchemaManager<'_>,
    index_name: &str,
    index: IndexCreateStatement,
) -> Result<(), DbErr> {
    if manager
        .has_index(AgentInputOutbox::Table.to_string(), index_name)
        .await?
    {
        tracing::info!(
            table = "agent_input_outbox",
            index = index_name,
            "skipping existing migration index"
        );
        return Ok(());
    }

    tracing::info!(
        table = "agent_input_outbox",
        index = index_name,
        "creating missing migration index"
    );
    manager.create_index(index).await
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
