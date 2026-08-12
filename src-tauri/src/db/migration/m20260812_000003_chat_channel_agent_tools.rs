use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_targets(manager).await?;
        create_audit(manager).await?;
        create_requests(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ChatChannelMessageLog::Table)
                    .add_column(ColumnDef::new(ChatChannelMessageLog::TargetId).string())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_chat_channel_message_log_target")
                    .table(ChatChannelMessageLog::Table)
                    .col(ChatChannelMessageLog::ChannelId)
                    .col(ChatChannelMessageLog::TargetId)
                    .col(ChatChannelMessageLog::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_chat_channel_message_log_target")
                    .table(ChatChannelMessageLog::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ChatChannelMessageLog::Table)
                    .drop_column(ChatChannelMessageLog::TargetId)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(ChatChannelToolRequest::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(ChatChannelAgentAudit::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ChatChannelTarget::Table).to_owned())
            .await
    }
}

async fn create_targets(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(ChatChannelTarget::Table)
                .if_not_exists()
                .col(pk_auto(ChatChannelTarget::Id))
                .col(integer(ChatChannelTarget::ChannelId))
                .col(string(ChatChannelTarget::TargetId))
                .col(string(ChatChannelTarget::TargetKind))
                .col(string(ChatChannelTarget::Source))
                .col(string(ChatChannelTarget::DisplayName))
                .col(string(ChatChannelTarget::Fingerprint))
                .col(boolean(ChatChannelTarget::IsDefault).default(false))
                .col(timestamp_with_time_zone(ChatChannelTarget::FirstSeenAt))
                .col(timestamp_with_time_zone(ChatChannelTarget::LastSeenAt))
                .col(timestamp_with_time_zone(ChatChannelTarget::CreatedAt))
                .col(timestamp_with_time_zone(ChatChannelTarget::UpdatedAt))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_chat_channel_target_channel")
                        .from(ChatChannelTarget::Table, ChatChannelTarget::ChannelId)
                        .to(ChatChannel::Table, ChatChannel::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_chat_channel_target_public_id")
                .table(ChatChannelTarget::Table)
                .col(ChatChannelTarget::TargetId)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_chat_channel_target_identity")
                .table(ChatChannelTarget::Table)
                .col(ChatChannelTarget::ChannelId)
                .col(ChatChannelTarget::Fingerprint)
                .unique()
                .to_owned(),
        )
        .await
}

async fn create_audit(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(ChatChannelAgentAudit::Table)
                .if_not_exists()
                .col(pk_auto(ChatChannelAgentAudit::Id))
                .col(string(ChatChannelAgentAudit::AgentType))
                .col(string(ChatChannelAgentAudit::SessionRef))
                .col(string(ChatChannelAgentAudit::Operation))
                .col(ColumnDef::new(ChatChannelAgentAudit::ChannelId).integer())
                .col(ColumnDef::new(ChatChannelAgentAudit::TargetId).string())
                .col(ColumnDef::new(ChatChannelAgentAudit::TargetLabel).string())
                .col(ColumnDef::new(ChatChannelAgentAudit::FileSummaryJson).text())
                .col(string(ChatChannelAgentAudit::Status))
                .col(ColumnDef::new(ChatChannelAgentAudit::ErrorCode).string())
                .col(string(ChatChannelAgentAudit::RequestId))
                .col(timestamp_with_time_zone(ChatChannelAgentAudit::CreatedAt))
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_chat_channel_agent_audit_created")
                .table(ChatChannelAgentAudit::Table)
                .col(ChatChannelAgentAudit::CreatedAt)
                .to_owned(),
        )
        .await
}

async fn create_requests(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(ChatChannelToolRequest::Table)
                .if_not_exists()
                .col(pk_auto(ChatChannelToolRequest::Id))
                .col(string(ChatChannelToolRequest::CallerScope))
                .col(string(ChatChannelToolRequest::Operation))
                .col(string(ChatChannelToolRequest::RequestId))
                .col(string(ChatChannelToolRequest::InputDigest))
                .col(string(ChatChannelToolRequest::Status))
                .col(ColumnDef::new(ChatChannelToolRequest::ResultJson).text())
                .col(timestamp_with_time_zone(ChatChannelToolRequest::CreatedAt))
                .col(timestamp_with_time_zone(ChatChannelToolRequest::UpdatedAt))
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_chat_channel_tool_request_key")
                .table(ChatChannelToolRequest::Table)
                .col(ChatChannelToolRequest::CallerScope)
                .col(ChatChannelToolRequest::Operation)
                .col(ChatChannelToolRequest::RequestId)
                .unique()
                .to_owned(),
        )
        .await
}

#[derive(Iden)]
enum ChatChannel {
    Table,
    Id,
}

#[derive(Iden)]
enum ChatChannelTarget {
    Table,
    Id,
    ChannelId,
    TargetId,
    TargetKind,
    Source,
    DisplayName,
    Fingerprint,
    IsDefault,
    FirstSeenAt,
    LastSeenAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum ChatChannelAgentAudit {
    Table,
    Id,
    AgentType,
    SessionRef,
    Operation,
    ChannelId,
    TargetId,
    TargetLabel,
    FileSummaryJson,
    Status,
    ErrorCode,
    RequestId,
    CreatedAt,
}

#[derive(Iden)]
enum ChatChannelToolRequest {
    Table,
    Id,
    CallerScope,
    Operation,
    RequestId,
    InputDigest,
    Status,
    ResultJson,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum ChatChannelMessageLog {
    Table,
    ChannelId,
    TargetId,
    CreatedAt,
}

fn pk_auto<T: IntoIden>(column: T) -> ColumnDef {
    ColumnDef::new(column)
        .integer()
        .not_null()
        .auto_increment()
        .primary_key()
        .to_owned()
}

fn integer<T: IntoIden>(column: T) -> ColumnDef {
    ColumnDef::new(column).integer().not_null().to_owned()
}

fn string<T: IntoIden>(column: T) -> ColumnDef {
    ColumnDef::new(column).string().not_null().to_owned()
}

fn boolean<T: IntoIden>(column: T) -> ColumnDef {
    ColumnDef::new(column).boolean().not_null().to_owned()
}

fn timestamp_with_time_zone<T: IntoIden>(column: T) -> ColumnDef {
    ColumnDef::new(column)
        .timestamp_with_time_zone()
        .not_null()
        .to_owned()
}
