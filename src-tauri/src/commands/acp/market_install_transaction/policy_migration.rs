use super::MarketSkillInstallTransaction;

use crate::app_error::AppCommandError;
use sea_orm::DatabaseConnection;

impl MarketSkillInstallTransaction {
    pub(crate) async fn migrate_plugin_skill_policies(
        &self,
        conn: &DatabaseConnection,
    ) -> Result<(), AppCommandError> {
        let migrations = self
            .replaced
            .iter()
            .filter_map(|value| {
                value.legacy_source.as_ref().map(|path| {
                    (
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .map(str::to_string),
                        value.skill_id.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        if migrations.iter().any(|(previous, _)| previous.is_none()) {
            return Err(AppCommandError::configuration_invalid("旧插件技能标识无效"));
        }
        let migrations = migrations
            .into_iter()
            .map(|(previous, current)| (previous.unwrap_or_default(), current))
            .collect::<Vec<_>>();
        crate::db::service::skill_activation_policy_service::migrate_skill_ids(conn, &migrations)
            .await
            .map_err(AppCommandError::db)
    }
}
