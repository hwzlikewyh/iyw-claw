use super::types::{RecoveryItem, RecoveryPlan};
use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use sea_orm::DatabaseConnection;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

const MAX_RECOVERY_ITEMS: usize = 64;

pub(crate) async fn request(
    conn: &DatabaseConnection,
    record: &PluginInstallationRecord,
) -> Result<RecoveryPlan, AppCommandError> {
    let manifest: crate::commands::skill_market::SkillPluginManifest =
        serde_json::from_str(&record.installation.manifest_json)
            .map_err(|_| invalid("插件清单记录无效"))?;
    let builder=crate::commands::skill_market::client::request(conn,reqwest::Method::POST,"/skills/recovery-plan").await?
        .json(&json!({"id":record.installation.market_skill_id.to_string(),"version":record.installation.version,"contentSha256":record.installation.content_sha256,"manifestDigest":manifest.manifest_digest.as_deref().unwrap_or("")}));
    let raw = crate::commands::skill_market::client::send(builder).await?;
    let plan: RecoveryPlan =
        serde_json::from_value(raw).map_err(|_| invalid("插件恢复元数据格式无效"))?;
    validate(&plan, record, &manifest)?;
    Ok(plan)
}

fn validate(
    plan: &RecoveryPlan,
    record: &PluginInstallationRecord,
    manifest: &crate::commands::skill_market::SkillPluginManifest,
) -> Result<(), AppCommandError> {
    if plan.root_skill_id != record.installation.market_skill_id.to_string()
        || plan.root_version != record.installation.version
        || plan.items.is_empty()
        || plan.items.len() > MAX_RECOVERY_ITEMS
    {
        return Err(invalid("插件恢复计划身份或数量无效"));
    }
    let root = plan
        .items
        .last()
        .ok_or_else(|| invalid("插件恢复计划为空"))?;
    if root.skill_id != plan.root_skill_id
        || root.version != plan.root_version
        || root.slug != record.installation.slug
        || root.content_sha256 != record.installation.content_sha256
        || root.plugin.as_ref() != Some(manifest)
    {
        return Err(invalid("插件本地包与恢复版本声明不符"));
    }
    let seen = index(&plan.items)?;
    validate_closure(root, &seen)
}

fn index(items: &[RecoveryItem]) -> Result<BTreeMap<String, &RecoveryItem>, AppCommandError> {
    let mut seen = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    for item in items {
        if item
            .skill_id
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .is_none()
            || seen.contains_key(&item.skill_id)
            || semver::Version::parse(&item.version).is_err()
            || !matches!(item.package_type.as_str(), "skill" | "plugin")
            || !slugs.insert(&item.slug)
            || item.dependencies.len() > MAX_RECOVERY_ITEMS
            || item
                .dependencies
                .iter()
                .map(|value| &value.skill_id)
                .collect::<BTreeSet<_>>()
                .len()
                != item.dependencies.len()
        {
            return Err(invalid("插件恢复依赖身份无效或重复"));
        }
        for dependency in &item.dependencies {
            let previous: &&RecoveryItem = seen
                .get(&dependency.skill_id)
                .ok_or_else(|| invalid("插件恢复计划缺少依赖或存在循环"))?;
            if previous.slug != dependency.slug || previous.version != dependency.version {
                return Err(invalid("插件恢复依赖版本不符"));
            }
        }
        seen.insert(item.skill_id.clone(), item);
    }
    Ok(seen)
}

fn validate_closure(
    root: &RecoveryItem,
    seen: &BTreeMap<String, &RecoveryItem>,
) -> Result<(), AppCommandError> {
    let mut required = BTreeSet::from([root.skill_id.clone()]);
    let mut pending = vec![root];
    while let Some(item) = pending.pop() {
        for dependency in &item.dependencies {
            if required.insert(dependency.skill_id.clone()) {
                pending.push(
                    *seen
                        .get(&dependency.skill_id)
                        .ok_or_else(|| invalid("插件恢复计划缺少依赖"))?,
                );
            }
        }
    }
    if required.len() != seen.len() {
        return Err(invalid("插件恢复计划包含无关安装项"));
    }
    Ok(())
}

fn invalid(message: &str) -> AppCommandError {
    AppCommandError::configuration_invalid(message)
}
