use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // folder.is_chat (bool) → folder.kind (text enum)
        manager
            .alter_table(
                Table::alter()
                    .table(Folder::Table)
                    .add_column(
                        ColumnDef::new(Folder::Kind)
                            .text()
                            .not_null()
                            .default("regular"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .exec_stmt(
                Query::update()
                    .table(Folder::Table)
                    .value(Folder::Kind, "chat")
                    .and_where(Expr::col(Folder::IsChat).eq(true))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Folder::Table)
                    .drop_column(Folder::IsChat)
                    .to_owned(),
            )
            .await?;

        // conversation.kind
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .add_column(
                        ColumnDef::new(Conversation::Kind)
                            .text()
                            .not_null()
                            .default("regular"),
                    )
                    .to_owned(),
            )
            .await?;
        // Backfill order matters: delegation children first (they may live in
        // chat folders), then chat limited to top-level rows.
        manager
            .exec_stmt(
                Query::update()
                    .table(Conversation::Table)
                    .value(Conversation::Kind, "delegate")
                    .and_where(Expr::col(Conversation::ParentId).is_not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .exec_stmt(
                Query::update()
                    .table(Conversation::Table)
                    .value(Conversation::Kind, "chat")
                    .and_where(Expr::col(Conversation::ParentId).is_null())
                    .and_where(
                        Expr::col(Conversation::FolderId).in_subquery(
                            Query::select()
                                .column(Folder::Id)
                                .from(Folder::Table)
                                .and_where(Expr::col(Folder::Kind).eq("chat"))
                                .to_owned(),
                        ),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .drop_column(Conversation::Kind)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Folder::Table)
                    .add_column(
                        ColumnDef::new(Folder::IsChat)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .exec_stmt(
                Query::update()
                    .table(Folder::Table)
                    .value(Folder::IsChat, true)
                    .and_where(Expr::col(Folder::Kind).eq("chat"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Folder::Table)
                    .drop_column(Folder::Kind)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Folder {
    Table,
    Id,
    Kind,
    IsChat,
}

#[derive(DeriveIden)]
enum Conversation {
    Table,
    Kind,
    ParentId,
    FolderId,
}
