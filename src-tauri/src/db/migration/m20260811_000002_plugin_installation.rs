use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_plugin_installation(manager).await?;
        create_plugin_component_ownership(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PluginComponentOwnership::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(PluginInstallation::Table).to_owned())
            .await
    }
}

async fn create_plugin_installation(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(PluginInstallation::Table)
                .if_not_exists()
                .col(primary_key(PluginInstallation::Id))
                .col(
                    ColumnDef::new(PluginInstallation::MarketSkillId)
                        .big_integer()
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::Slug)
                        .string()
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::Version)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::InstallRoot)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::Status)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::ContentSha256)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::ObjectSha256)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::AgentTypesJson)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginInstallation::ManifestJson)
                        .text()
                        .not_null(),
                )
                .col(timestamp(PluginInstallation::CreatedAt))
                .col(timestamp(PluginInstallation::UpdatedAt))
                .to_owned(),
        )
        .await
}

async fn create_plugin_component_ownership(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(PluginComponentOwnership::Table)
                .if_not_exists()
                .col(primary_key(PluginComponentOwnership::Id))
                .col(
                    ColumnDef::new(PluginComponentOwnership::PluginInstallationId)
                        .integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginComponentOwnership::ComponentType)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginComponentOwnership::ComponentKey)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginComponentOwnership::ManagedResourceKey)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(PluginComponentOwnership::RelativePath)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(PluginComponentOwnership::ServerKey)
                        .string()
                        .null(),
                )
                .col(timestamp(PluginComponentOwnership::CreatedAt))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_plugin_component_installation")
                        .from(
                            PluginComponentOwnership::Table,
                            PluginComponentOwnership::PluginInstallationId,
                        )
                        .to(PluginInstallation::Table, PluginInstallation::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_plugin_component_owner_key")
                .table(PluginComponentOwnership::Table)
                .col(PluginComponentOwnership::PluginInstallationId)
                .col(PluginComponentOwnership::ComponentType)
                .col(PluginComponentOwnership::ComponentKey)
                .unique()
                .to_owned(),
        )
        .await
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
enum PluginInstallation {
    Table,
    Id,
    MarketSkillId,
    Slug,
    Version,
    InstallRoot,
    Status,
    ContentSha256,
    ObjectSha256,
    AgentTypesJson,
    ManifestJson,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum PluginComponentOwnership {
    Table,
    Id,
    PluginInstallationId,
    ComponentType,
    ComponentKey,
    ManagedResourceKey,
    RelativePath,
    ServerKey,
    CreatedAt,
}
