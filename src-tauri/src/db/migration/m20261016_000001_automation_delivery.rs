use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in ["retry_of_run_id", "notification_channel_id"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("automation_run"))
                        .add_column(ColumnDef::new(Alias::new(column)).integer().null())
                        .to_owned(),
                )
                .await?;
        }
        for column in [
            "notification_channel_name",
            "notification_status",
            "notification_error",
            "notification_target_json",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("automation_run"))
                        .add_column(ColumnDef::new(Alias::new(column)).text().null())
                        .to_owned(),
                )
                .await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("automation_run"))
                    .add_column(
                        ColumnDef::new(Alias::new("notification_updated_at"))
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, column) in [
            ("idx_automation_run_retry", "retry_of_run_id"),
            ("idx_automation_run_notification", "notification_status"),
        ] {
            manager
                .create_index(
                    Index::create()
                        .name(name)
                        .table(Alias::new("automation_run"))
                        .col(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in [
            "idx_automation_run_retry",
            "idx_automation_run_notification",
        ] {
            manager
                .drop_index(Index::drop().name(name).to_owned())
                .await?;
        }
        for column in [
            "retry_of_run_id",
            "notification_channel_id",
            "notification_channel_name",
            "notification_status",
            "notification_error",
            "notification_target_json",
            "notification_updated_at",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("automation_run"))
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
