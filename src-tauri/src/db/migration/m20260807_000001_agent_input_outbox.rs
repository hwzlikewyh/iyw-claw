use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_outbox_table(manager).await?;
        create_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AgentInputOutbox::Table).to_owned())
            .await
    }
}

async fn create_outbox_table(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager.create_table(outbox_table()).await
}

fn outbox_table() -> TableCreateStatement {
    let mut table = Table::create();
    table.table(AgentInputOutbox::Table).if_not_exists();
    add_identity_columns(&mut table);
    add_payload_columns(&mut table);
    add_timestamps(&mut table);
    table.foreign_key(conversation_foreign_key()).to_owned()
}

fn add_identity_columns(table: &mut TableCreateStatement) {
    table
        .col(
            ColumnDef::new(AgentInputOutbox::Id)
                .string()
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(AgentInputOutbox::ConversationId)
                .integer()
                .not_null(),
        )
        .col(ColumnDef::new(AgentInputOutbox::ConnectionId).string())
        .col(ColumnDef::new(AgentInputOutbox::TargetTurnGeneration).big_integer())
        .col(
            ColumnDef::new(AgentInputOutbox::AgentType)
                .string()
                .not_null(),
        );
}

fn add_payload_columns(table: &mut TableCreateStatement) {
    table
        .col(
            ColumnDef::new(AgentInputOutbox::PayloadJson)
                .text()
                .not_null(),
        )
        .col(ColumnDef::new(AgentInputOutbox::Strategy).string())
        .col(
            ColumnDef::new(AgentInputOutbox::Status)
                .string()
                .not_null()
                .default("waiting"),
        )
        .col(
            ColumnDef::new(AgentInputOutbox::DispatchAttempt)
                .integer()
                .not_null()
                .default(0),
        )
        .col(ColumnDef::new(AgentInputOutbox::LastError).text());
}

fn add_timestamps(table: &mut TableCreateStatement) {
    table
        .col(
            ColumnDef::new(AgentInputOutbox::CreatedAt)
                .timestamp_with_time_zone()
                .not_null(),
        )
        .col(ColumnDef::new(AgentInputOutbox::DispatchedAt).timestamp_with_time_zone())
        .col(ColumnDef::new(AgentInputOutbox::ConsumedAt).timestamp_with_time_zone())
        .col(ColumnDef::new(AgentInputOutbox::DeletedAt).timestamp_with_time_zone());
}

fn conversation_foreign_key() -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name("fk_agent_input_outbox_conversation")
        .from(AgentInputOutbox::Table, AgentInputOutbox::ConversationId)
        .to(Conversation::Table, Conversation::Id)
        .on_delete(ForeignKeyAction::Cascade)
        .on_update(ForeignKeyAction::Cascade)
        .to_owned()
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let indexes = [
        Index::create()
            .name("idx_agent_input_outbox_conversation_status_created")
            .table(AgentInputOutbox::Table)
            .col(AgentInputOutbox::ConversationId)
            .col(AgentInputOutbox::Status)
            .col(AgentInputOutbox::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_agent_input_outbox_connection_status_created")
            .table(AgentInputOutbox::Table)
            .col(AgentInputOutbox::ConnectionId)
            .col(AgentInputOutbox::Status)
            .col(AgentInputOutbox::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_agent_input_outbox_turn_status")
            .table(AgentInputOutbox::Table)
            .col(AgentInputOutbox::TargetTurnGeneration)
            .col(AgentInputOutbox::Status)
            .to_owned(),
    ];
    for index in indexes {
        manager.create_index(index).await?;
    }
    Ok(())
}

#[derive(Iden)]
enum AgentInputOutbox {
    Table,
    Id,
    ConversationId,
    ConnectionId,
    TargetTurnGeneration,
    AgentType,
    PayloadJson,
    Strategy,
    Status,
    DispatchAttempt,
    LastError,
    CreatedAt,
    DispatchedAt,
    ConsumedAt,
    DeletedAt,
}

#[derive(Iden)]
enum Conversation {
    Table,
    Id,
}
