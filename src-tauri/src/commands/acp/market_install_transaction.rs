use super::*;

/// 插件外层事务确认前保留原技能目录与发布状态；不跨await持有同步锁。
pub(crate) struct MarketSkillInstallTransaction {
    root_id: i64,
    retained: BTreeSet<(i64, String)>,
    replaced: Vec<PreparedMarketSkillInstall>,
    expected: Vec<String>,
    resolved: bool,
}

impl MarketSkillInstallTransaction {
    pub(crate) fn begin(
        target: (&[AgentType], i64),
        installs: Vec<MarketSkillInstall>,
    ) -> Result<Self, AcpError> {
        let _paths = require_private_agent_storage_for_write()?;
        if target.0.is_empty()
            || !installs.is_empty() && market_install_root_id(&installs)? != target.1
        {
            return Err(AcpError::protocol(
                "Skill publication targets or install root are invalid",
            ));
        }
        let retained = installs
            .iter()
            .flat_map(|value| {
                std::iter::once((value.marker.skill_id, value.marker.slug.clone())).chain(
                    value
                        .marker
                        .dependencies
                        .iter()
                        .filter(|dependency| {
                            crate::commands::office_tools::is_officecli_skill_id(&dependency.slug)
                        })
                        .map(|dependency| (dependency.skill_id, dependency.slug.clone())),
                )
            })
            .collect();
        let _guard = shared_skill_mutation_guard();
        let prepared = prepare_market_skill_installs(installs)?;
        let mut replaced = replace_market_skill_directories(prepared)?;
        let result = publish_market_skill_installs(&replaced).and_then(|_| stamps(&replaced));
        let expected = match result {
            Ok(value) => value,
            Err(error) => {
                rollback_market_skill_directories(&mut replaced).map_err(|rollback| {
                    AcpError::protocol(format!("{error}; rollback incomplete: {rollback}"))
                })?;
                return Err(error);
            }
        };
        Ok(Self {
            root_id: target.1,
            retained,
            replaced,
            expected,
            resolved: false,
        })
    }

    pub(crate) fn finish(mut self) {
        let _guard = shared_skill_mutation_guard();
        self.resolved = true;
        if let Err(error) = release_market_root_references(self.root_id, &self.retained) {
            tracing::warn!(root_skill_id=self.root_id,error=%error,"[skill-market] 已提交，旧依赖引用清理未完成");
        }
        finish_market_skill_directories(&mut self.replaced);
        for value in &self.replaced {
            let Some(legacy) = &value.legacy_source else {
                continue;
            };
            if !path_entry_exists(legacy) {
                continue;
            }
            let Some(legacy_id) = legacy.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if let Err(error) = remove_shared_skill_publications_locked(legacy_id).and_then(|_| {
                remove_skill_entry(legacy).map_err(|error| AcpError::protocol(error.to_string()))
            }) {
                tracing::warn!(
                    skill_id = %value.skill_id,
                    legacy = %legacy.display(),
                    error = %error,
                    "[skill-market] superseded plugin Skill cleanup failed; legacy directory retained"
                );
            }
        }
    }

    pub(crate) fn rollback(mut self) -> Result<(), AcpError> {
        self.resolved = true;
        let _guard = shared_skill_mutation_guard();
        self.restore()
    }

    fn restore(&mut self) -> Result<(), AcpError> {
        if stamps(&self.replaced)? != self.expected {
            return Err(AcpError::protocol(
                "Skill state changed during plugin installation; backup retained for recovery",
            ));
        }
        rollback_market_skill_directories(&mut self.replaced)
    }
}

impl Drop for MarketSkillInstallTransaction {
    fn drop(&mut self) {
        if !self.resolved {
            let _guard = shared_skill_mutation_guard();
            if let Err(error) = self.restore() {
                tracing::error!(root_skill_id=self.root_id,error=%error,"[skill-market] 中断安装的技能回退未完成，保留备份");
            }
        }
    }
}

impl MarketSkillInstallTransaction {
    pub(crate) async fn migrate_plugin_skill_policies(
        &self,
        conn: &sea_orm::DatabaseConnection,
    ) -> Result<(), crate::app_error::AppCommandError> {
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
            return Err(crate::app_error::AppCommandError::configuration_invalid(
                "Legacy plugin Skill identity is invalid",
            ));
        }
        let migrations = migrations
            .into_iter()
            .map(|(previous, current)| (previous.unwrap_or_default(), current))
            .collect::<Vec<_>>();
        crate::db::service::skill_activation_policy_service::migrate_skill_ids(conn, &migrations)
            .await
            .map_err(crate::app_error::AppCommandError::db)
    }
}

fn stamps(replaced: &[PreparedMarketSkillInstall]) -> Result<Vec<String>, AcpError> {
    replaced
        .iter()
        .map(|value| {
            let marker = fs::read(value.source.join(MARKET_SKILL_MARKER))
                .map_err(|error| AcpError::protocol(error.to_string()))?;
            let enabled = shared_skill_publish_enabled(&value.source, &value.skill_id)?;
            let publications = existing_shared_skill_publications(&value.source, &value.skill_id)?;
            Ok(format!("{:?}|{enabled}|{publications:?}", marker))
        })
        .collect()
}
