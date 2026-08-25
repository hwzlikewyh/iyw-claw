use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .add_column(
                        ColumnDef::new(Conversation::LastCompletedTurnGeneration)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskArtifact::Table)
                    .add_column(
                        ColumnDef::new(TaskArtifact::TurnGeneration)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_task_artifact_conversation_turn")
                    .table(TaskArtifact::Table)
                    .col(TaskArtifact::ConversationId)
                    .col(TaskArtifact::TurnGeneration)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_task_artifact_conversation_turn")
                    .table(TaskArtifact::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskArtifact::Table)
                    .drop_column(TaskArtifact::TurnGeneration)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Conversation::Table)
                    .drop_column(Conversation::LastCompletedTurnGeneration)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Conversation {
    Table,
    LastCompletedTurnGeneration,
}

#[derive(Iden)]
enum TaskArtifact {
    Table,
    ConversationId,
    TurnGeneration,
}
