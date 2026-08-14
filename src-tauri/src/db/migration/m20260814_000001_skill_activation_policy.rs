use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SkillActivationPolicy::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SkillActivationPolicy::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SkillActivationPolicy::SkillId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SkillActivationPolicy::Scope)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SkillActivationPolicy::WorkspaceKey)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SkillActivationPolicy::AgentType)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SkillActivationPolicy::RequestedEnabled)
                            .boolean()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SkillActivationPolicy::PolicySource)
                            .string()
                            .not_null(),
                    )
                    .col(timestamp(SkillActivationPolicy::CreatedAt))
                    .col(timestamp(SkillActivationPolicy::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_skill_activation_policy_target")
                    .table(SkillActivationPolicy::Table)
                    .col(SkillActivationPolicy::SkillId)
                    .col(SkillActivationPolicy::Scope)
                    .col(SkillActivationPolicy::WorkspaceKey)
                    .col(SkillActivationPolicy::AgentType)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SkillActivationPolicy::Table).to_owned())
            .await
    }
}

fn timestamp(name: impl IntoIden) -> ColumnDef {
    ColumnDef::new(name)
        .timestamp_with_time_zone()
        .not_null()
        .to_owned()
}

#[derive(DeriveIden)]
enum SkillActivationPolicy {
    Table,
    Id,
    SkillId,
    Scope,
    WorkspaceKey,
    AgentType,
    RequestedEnabled,
    PolicySource,
    CreatedAt,
    UpdatedAt,
}
