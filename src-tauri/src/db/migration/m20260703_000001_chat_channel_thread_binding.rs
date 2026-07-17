use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(binding_table()).await?;
        manager.create_index(thread_index()).await?;
        manager.create_index(conversation_index()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ChatChannelThreadBinding::Table)
                    .to_owned(),
            )
            .await
    }
}

fn binding_table() -> TableCreateStatement {
    let mut channel_key = channel_foreign_key();
    let mut conversation_key = conversation_foreign_key();
    let mut table = Table::create();
    table
        .table(ChatChannelThreadBinding::Table)
        .if_not_exists()
        .col(integer_pk(ChatChannelThreadBinding::Id))
        .col(integer(ChatChannelThreadBinding::ChannelId))
        .col(string(ChatChannelThreadBinding::ChannelType))
        .col(string(ChatChannelThreadBinding::ChatId))
        .col(string(ChatChannelThreadBinding::ThreadKey))
        .col(string(ChatChannelThreadBinding::ThreadKind))
        .col(integer(ChatChannelThreadBinding::ConversationId))
        .col(ColumnDef::new(ChatChannelThreadBinding::ConnectionId).string())
        .col(string(ChatChannelThreadBinding::CreatedBySenderId))
        .col(ColumnDef::new(ChatChannelThreadBinding::DisplayTitle).string())
        .col(
            ColumnDef::new(ChatChannelThreadBinding::TitleSyncEnabled)
                .boolean()
                .not_null()
                .default(true),
        )
        .col(ColumnDef::new(ChatChannelThreadBinding::ProviderPayloadJson).text())
        .col(timestamp(ChatChannelThreadBinding::CreatedAt))
        .col(timestamp(ChatChannelThreadBinding::UpdatedAt))
        .foreign_key(&mut channel_key)
        .foreign_key(&mut conversation_key);
    table.to_owned()
}

fn integer_pk(column: ChatChannelThreadBinding) -> ColumnDef {
    ColumnDef::new(column)
        .integer()
        .not_null()
        .auto_increment()
        .primary_key()
        .to_owned()
}

fn integer(column: ChatChannelThreadBinding) -> ColumnDef {
    ColumnDef::new(column).integer().not_null().to_owned()
}

fn string(column: ChatChannelThreadBinding) -> ColumnDef {
    ColumnDef::new(column).string().not_null().to_owned()
}

fn timestamp(column: ChatChannelThreadBinding) -> ColumnDef {
    ColumnDef::new(column)
        .timestamp_with_time_zone()
        .not_null()
        .to_owned()
}

fn channel_foreign_key() -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name("fk_cctb_channel_id")
        .from(
            ChatChannelThreadBinding::Table,
            ChatChannelThreadBinding::ChannelId,
        )
        .to(ChatChannel::Table, ChatChannel::Id)
        .on_delete(ForeignKeyAction::Cascade)
        .to_owned()
}

fn conversation_foreign_key() -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name("fk_cctb_conversation_id")
        .from(
            ChatChannelThreadBinding::Table,
            ChatChannelThreadBinding::ConversationId,
        )
        .to(Conversation::Table, Conversation::Id)
        .on_delete(ForeignKeyAction::Cascade)
        .to_owned()
}

fn thread_index() -> IndexCreateStatement {
    Index::create()
        .name("idx_cctb_thread")
        .table(ChatChannelThreadBinding::Table)
        .col(ChatChannelThreadBinding::ChannelId)
        .col(ChatChannelThreadBinding::ThreadKind)
        .col(ChatChannelThreadBinding::ChatId)
        .col(ChatChannelThreadBinding::ThreadKey)
        .unique()
        .to_owned()
}

fn conversation_index() -> IndexCreateStatement {
    Index::create()
        .name("idx_cctb_conversation")
        .table(ChatChannelThreadBinding::Table)
        .col(ChatChannelThreadBinding::ChannelId)
        .col(ChatChannelThreadBinding::ConversationId)
        .to_owned()
}

#[derive(DeriveIden)]
enum ChatChannelThreadBinding {
    Table,
    Id,
    ChannelId,
    ChannelType,
    ChatId,
    ThreadKey,
    ThreadKind,
    ConversationId,
    ConnectionId,
    CreatedBySenderId,
    DisplayTitle,
    TitleSyncEnabled,
    ProviderPayloadJson,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ChatChannel {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Conversation {
    Table,
    Id,
}
