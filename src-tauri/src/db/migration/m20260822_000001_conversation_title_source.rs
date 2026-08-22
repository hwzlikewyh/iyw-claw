use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_columns(manager).await?;
        mark_existing_rows(manager).await?;
        repair_leaked_titles(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .drop_column(Conversation::TitleSummaryAttempted)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .drop_column(Conversation::TitleSource)
                    .to_owned(),
            )
            .await
    }
}

async fn add_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Conversation::Table)
                .add_column(
                    ColumnDef::new(Conversation::TitleSource)
                        .string()
                        .not_null()
                        .default("user_fallback"),
                )
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(Conversation::Table)
                .add_column(
                    ColumnDef::new(Conversation::TitleSummaryAttempted)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await
}

async fn mark_existing_rows(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    // Existing conversations predate automatic summaries. Avoid charging for
    // them when a resumed process starts its turn generation from one again.
    manager
        .exec_stmt(
            Query::update()
                .table(Conversation::Table)
                .value(Conversation::TitleSummaryAttempted, true)
                .to_owned(),
        )
        .await?;
    manager
        .exec_stmt(
            Query::update()
                .table(Conversation::Table)
                .value(Conversation::TitleSource, "manual")
                .and_where(Expr::col(Conversation::TitleLocked).eq(true))
                .to_owned(),
        )
        .await
}

async fn repair_leaked_titles(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .exec_stmt(
            Query::update()
                .table(Conversation::Table)
                .value(Conversation::Title, Expr::value(Option::<String>::None))
                .value(Conversation::TitleSource, "user_fallback")
                .value(Conversation::TitleSummaryAttempted, false)
                .and_where(
                    Expr::col(Conversation::Title).like("## Current-turn final artifact delivery%"),
                )
                .and_where(Expr::col(Conversation::TitleLocked).eq(false))
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum Conversation {
    Table,
    Title,
    TitleLocked,
    TitleSource,
    TitleSummaryAttempted,
}
