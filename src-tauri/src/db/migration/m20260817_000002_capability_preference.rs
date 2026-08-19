use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CapabilityPreference::Table)
                    .if_not_exists()
                    .col(primary_key(CapabilityPreference::Id))
                    .col(
                        ColumnDef::new(CapabilityPreference::SubjectKind)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapabilityPreference::SubjectId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapabilityPreference::Capability)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapabilityPreference::Enabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(timestamp(CapabilityPreference::CreatedAt))
                    .col(timestamp(CapabilityPreference::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_capability_preference_target")
                    .table(CapabilityPreference::Table)
                    .col(CapabilityPreference::SubjectKind)
                    .col(CapabilityPreference::SubjectId)
                    .col(CapabilityPreference::Capability)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_capability_preference_subject")
                    .table(CapabilityPreference::Table)
                    .col(CapabilityPreference::SubjectKind)
                    .col(CapabilityPreference::SubjectId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CapabilityPreference::Table).to_owned())
            .await
    }
}

fn primary_key(name: impl IntoIden) -> ColumnDef {
    ColumnDef::new(name)
        .integer()
        .not_null()
        .auto_increment()
        .primary_key()
        .to_owned()
}

fn timestamp(name: impl IntoIden) -> ColumnDef {
    ColumnDef::new(name)
        .timestamp_with_time_zone()
        .not_null()
        .to_owned()
}

#[derive(DeriveIden)]
enum CapabilityPreference {
    Table,
    Id,
    SubjectKind,
    SubjectId,
    Capability,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
