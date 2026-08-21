use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(binding_table()).await?;
        manager.create_index(route_index()).await?;
        manager.create_index(conversation_index()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ChatChannelConversationBinding::Table)
                    .to_owned(),
            )
            .await
    }
}

fn binding_table() -> TableCreateStatement {
    Table::create()
        .table(ChatChannelConversationBinding::Table)
        .if_not_exists()
        .col(
            ColumnDef::new(ChatChannelConversationBinding::Id)
                .integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
        .col(
            ColumnDef::new(ChatChannelConversationBinding::ChannelId)
                .integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(ChatChannelConversationBinding::RouteKey)
                .string()
                .not_null(),
        )
        .col(
            ColumnDef::new(ChatChannelConversationBinding::TargetId)
                .string()
                .not_null(),
        )
        .col(
            ColumnDef::new(ChatChannelConversationBinding::ConversationId)
                .integer()
                .not_null(),
        )
        .col(timestamp(ChatChannelConversationBinding::CreatedAt))
        .col(timestamp(ChatChannelConversationBinding::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_cccb_channel")
                .from(
                    ChatChannelConversationBinding::Table,
                    ChatChannelConversationBinding::ChannelId,
                )
                .to(ChatChannel::Table, ChatChannel::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_cccb_target")
                .from(
                    ChatChannelConversationBinding::Table,
                    ChatChannelConversationBinding::TargetId,
                )
                .to(ChatChannelTarget::Table, ChatChannelTarget::TargetId)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_cccb_conversation")
                .from(
                    ChatChannelConversationBinding::Table,
                    ChatChannelConversationBinding::ConversationId,
                )
                .to(Conversation::Table, Conversation::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn timestamp(column: ChatChannelConversationBinding) -> ColumnDef {
    ColumnDef::new(column)
        .timestamp_with_time_zone()
        .not_null()
        .to_owned()
}

fn route_index() -> IndexCreateStatement {
    Index::create()
        .name("idx_cccb_route")
        .table(ChatChannelConversationBinding::Table)
        .col(ChatChannelConversationBinding::ChannelId)
        .col(ChatChannelConversationBinding::RouteKey)
        .unique()
        .to_owned()
}

fn conversation_index() -> IndexCreateStatement {
    Index::create()
        .name("idx_cccb_conversation")
        .table(ChatChannelConversationBinding::Table)
        .col(ChatChannelConversationBinding::ConversationId)
        .to_owned()
}

#[derive(DeriveIden)]
enum ChatChannelConversationBinding {
    Table,
    Id,
    ChannelId,
    RouteKey,
    TargetId,
    ConversationId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ChatChannel {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum ChatChannelTarget {
    Table,
    TargetId,
}

#[derive(DeriveIden)]
enum Conversation {
    Table,
    Id,
}
