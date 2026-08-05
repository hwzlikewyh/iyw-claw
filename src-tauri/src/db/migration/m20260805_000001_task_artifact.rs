use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TaskArtifact::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TaskArtifact::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TaskArtifact::ConversationId)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(TaskArtifact::Path).string().not_null())
                    .col(
                        ColumnDef::new(TaskArtifact::DisplayName)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(TaskArtifact::SourcePath).string().not_null())
                    .col(
                        ColumnDef::new(TaskArtifact::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TaskArtifact::LastCheckedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TaskArtifact::Status)
                            .string()
                            .not_null()
                            .default("available"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_task_artifact_conversation")
                            .from(TaskArtifact::Table, TaskArtifact::ConversationId)
                            .to(Conversation::Table, Conversation::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_task_artifact_conversation_path")
                    .table(TaskArtifact::Table)
                    .col(TaskArtifact::ConversationId)
                    .col(TaskArtifact::Path)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_task_artifact_conversation_created")
                    .table(TaskArtifact::Table)
                    .col(TaskArtifact::ConversationId)
                    .col(TaskArtifact::CreatedAt)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TaskArtifact::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum TaskArtifact {
    Table,
    Id,
    ConversationId,
    Path,
    DisplayName,
    SourcePath,
    CreatedAt,
    LastCheckedAt,
    Status,
}

#[derive(Iden)]
enum Conversation {
    Table,
    Id,
}
