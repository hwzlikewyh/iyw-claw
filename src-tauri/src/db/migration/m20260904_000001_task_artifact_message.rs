use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uq_task_artifact_conversation_path")
                    .table(TaskArtifact::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskArtifact::Table)
                    .add_column(ColumnDef::new(TaskArtifact::MessageId).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_task_artifact_conversation_message")
                    .table(TaskArtifact::Table)
                    .col(TaskArtifact::ConversationId)
                    .col(TaskArtifact::MessageId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_task_artifact_conversation_path_message")
                    .table(TaskArtifact::Table)
                    .col(TaskArtifact::ConversationId)
                    .col(TaskArtifact::Path)
                    .col(TaskArtifact::MessageId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uq_task_artifact_conversation_path_message")
                    .table(TaskArtifact::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_task_artifact_conversation_message")
                    .table(TaskArtifact::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskArtifact::Table)
                    .drop_column(TaskArtifact::MessageId)
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
            .await
    }
}

#[derive(Iden)]
enum TaskArtifact {
    Table,
    ConversationId,
    Path,
    MessageId,
}
