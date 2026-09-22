use std::time::Duration;

use super::{check_updates_core, install_core, SkillUpdateCheckItem, SkillUpdateCheckResult};
use crate::commands::system_settings::load_skill_auto_update_settings;
use crate::db::AppDatabase;

const INITIAL_DELAY: Duration = Duration::from_secs(300);
const INTERVAL: Duration = Duration::from_secs(1800);
const BATCH_SIZE: usize = 256;

pub async fn task(db: AppDatabase) {
    tokio::time::sleep(INITIAL_DELAY).await;
    loop {
        run_pass(&db).await;
        tokio::time::sleep(INTERVAL).await;
    }
}

async fn run_pass(db: &AppDatabase) {
    let Ok(settings) = load_skill_auto_update_settings(&db.conn).await else {
        tracing::warn!("[skill-market] failed to load automatic update settings");
        return;
    };
    if !settings.enabled {
        tracing::debug!("[skill-market] automatic Skill updates disabled");
        return;
    }

    let mut installed = crate::commands::acp::installed_market_skill_versions();
    let plugin_installs =
        match crate::db::service::plugin_installation_service::list_installations(&db.conn).await {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(error = %error, "[skill-market] automatic plugin inventory failed");
                return;
            }
        };
    for plugin in plugin_installs {
        installed.insert(plugin.market_skill_id, plugin.version);
    }
    if installed.is_empty() {
        return;
    }
    let items = installed
        .iter()
        .map(|(id, version)| SkillUpdateCheckItem {
            id: id.to_string(),
            installed_version: version.clone(),
        })
        .collect::<Vec<_>>();
    let mut updated = 0usize;
    let mut skipped_plugins = 0usize;
    let mut failed = 0usize;
    for batch in items.chunks(BATCH_SIZE) {
        let result = match check_updates_core(&db.conn, batch.to_vec()).await {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(error = %error, "[skill-market] automatic update check failed");
                return;
            }
        };
        let counts = apply_candidates(db, result).await;
        updated += counts.updated;
        skipped_plugins += counts.skipped_plugins;
        failed += counts.failed;
    }
    tracing::info!(
        updated,
        skipped_plugins,
        failed,
        "[skill-market] automatic update pass completed"
    );
}

#[derive(Default)]
struct PassCounts {
    updated: usize,
    skipped_plugins: usize,
    failed: usize,
}

async fn apply_candidates(db: &AppDatabase, result: SkillUpdateCheckResult) -> PassCounts {
    let mut counts = PassCounts::default();
    for candidate in result.items {
        if candidate.status != "update_available" {
            continue;
        }
        if candidate.package_type == Some(super::SkillPackageType::Plugin) {
            counts.skipped_plugins += 1;
            tracing::info!(skill_id = %candidate.id, version = ?candidate.current_version, "[skill-market] plugin update left for manual confirmation");
            continue;
        }
        let Ok(skill_id) = candidate.id.parse::<i64>() else {
            counts.failed += 1;
            continue;
        };
        let targets = crate::commands::acp::installed_market_skill_targets(skill_id);
        if targets.is_empty() {
            counts.failed += 1;
            tracing::warn!(
                skill_id,
                "[skill-market] automatic update skipped because targets are missing"
            );
            continue;
        }
        let Some(version) = candidate.current_version.clone() else {
            counts.failed += 1;
            continue;
        };
        match install_core(
            &db.conn,
            candidate.id.clone(),
            version.clone(),
            targets.clone(),
        )
        .await
        {
            Ok(()) => counts.updated += 1,
            Err(error) => {
                counts.failed += 1;
                tracing::warn!(skill_id = %candidate.id, old_version = ?candidate.installed_version, new_version = %version, agent_types = ?targets, error = %error, "[skill-market] automatic Skill update failed");
            }
        }
    }
    counts
}
