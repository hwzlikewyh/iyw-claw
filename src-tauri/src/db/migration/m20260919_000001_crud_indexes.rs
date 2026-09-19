use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("m20260919_000001_crud_indexes.sql"))
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_conversation_folder_id ON conversation(folder_id); \
             CREATE INDEX IF NOT EXISTS idx_conversation_parent_id ON conversation(parent_id);"
        ).await?;
        for name in [
            "idx_conversation_live_updated",
            "idx_conversation_root_updated",
            "idx_conversation_folder_live_created",
            "idx_automation_run_conversation",
            "idx_channel_log_retention",
            "idx_automation_run_retention",
        ] {
            manager
                .drop_index(Index::drop().name(name).to_owned())
                .await?;
        }
        Ok(())
    }
}
