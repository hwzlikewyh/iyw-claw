use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // `title_locked` marks a conversation whose title the user set by hand
        // (rename). Once true, the per-turn auto-title backfill in
        // `get_folder_conversation` must never overwrite it. New rows default to
        // false (eligible for auto-derivation).
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .add_column(
                        ColumnDef::new(Conversation::TitleLocked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // Legacy rows predate the flag, and `update_title` (manual rename)
        // already existed, so some of these titles were hand-picked. We cannot
        // tell a hand-picked title from an auto-derived one after the fact, so
        // conservatively LOCK every pre-existing row that already has a title —
        // the auto-backfill must never silently replace a title a user may have
        // chosen before this column existed. Rows with no title stay unlocked so
        // the backfill can fill them in.
        manager
            .exec_stmt(
                Query::update()
                    .table(Conversation::Table)
                    .value(Conversation::TitleLocked, true)
                    .and_where(Expr::col(Conversation::Title).is_not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .drop_column(Conversation::TitleLocked)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Conversation {
    Table,
    Title,
    TitleLocked,
}

