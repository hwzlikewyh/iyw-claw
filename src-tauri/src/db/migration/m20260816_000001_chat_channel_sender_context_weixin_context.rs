use sea_orm_migration::prelude::*;

/// Persists the latest iLink reply context for each channel sender.
///
/// The nullable column keeps every existing sender-context row valid. The
/// token is credential-adjacent data and must never be emitted in logs.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ChatChannelSenderContext::Table)
                    .add_column(
                        ColumnDef::new(ChatChannelSenderContext::WeixinContextToken)
                            .string()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite migrations are forward-only to preserve upgraded databases.
        Ok(())
    }
}

#[derive(DeriveIden)]
enum ChatChannelSenderContext {
    Table,
    WeixinContextToken,
}
