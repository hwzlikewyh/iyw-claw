use super::super::{
    plugin_install_context::PreparedPluginInstall, plugin_receipt::PluginVersionReceipt,
};
use super::{local, skills, types::DependencyChange};
use crate::app_error::AppCommandError;
use crate::commands::{
    acp::{MarketSkillDependencyMarker, MarketSkillInstall},
    plugin_recovery::{ReleaseRecoveryItem, ReleaseRecoveryPlan},
};
use crate::db::service::plugin_installation_service::PluginInstallationRecord;
use crate::models::AgentType;
use std::collections::BTreeMap;

const MAX_LOCAL_BYTES: usize = 256 * 1024 * 1024;

pub(super) struct Context {
    pub plan: ReleaseRecoveryPlan,
    pub root: PluginVersionReceipt,
    pub records: Vec<PluginInstallationRecord>,
    pub agents: Vec<AgentType>,
    pub scope: String,
    pub root_id: i64,
}

#[derive(Default)]
pub(super) struct Built {
    pub plugins: Vec<PreparedPluginInstall>,
    pub skills: Vec<MarketSkillInstall>,
    pub dependencies: Vec<DependencyChange>,
    pub issues: Vec<String>,
    pub scope_changed: bool,
    bytes: usize,
}

pub(super) async fn build(context: &Context) -> Result<Built, AppCommandError> {
    let mut built = Built::default();
    for item in &context.plan.items {
        let installed = context
            .records
            .iter()
            .find(|record| record.installation.market_skill_id.to_string() == item.skill_id);
        let version = installed.map(|record| record.installation.version.clone());
        let result = load_item(context, item, &mut built).await;
        let ready = result.is_ok();
        if let Err(error) = result {
            built
                .issues
                .push(format!("{}@{}：{}", item.slug, item.version, error.message));
        }
        if item.skill_id != context.plan.root_skill_id {
            built.dependencies.push(DependencyChange {
                id: item.skill_id.clone(),
                slug: item.slug.clone(),
                version: item.version.clone(),
                changing: !crate::commands::office_tools::is_officecli_skill_id(&item.slug)
                    && version.as_deref() != Some(&item.version),
                managed: crate::commands::office_tools::is_officecli_skill_id(&item.slug),
                installed_version: version,
                ready,
            });
        }
        if built.bytes > MAX_LOCAL_BYTES {
            return Err(super::invalid("本地版本依赖包总量超出安装预算"));
        }
    }
    Ok(built)
}

fn selected_scope(context: &Context, item: &ReleaseRecoveryItem) -> Option<String> {
    if item.skill_id == context.plan.root_skill_id {
        return Some(context.root.account_scope.clone());
    }
    local::receipt(&item.slug, &item.version)
        .or_else(|_| {
            context
                .records
                .iter()
                .find(|record| {
                    record.installation.market_skill_id.to_string() == item.skill_id
                        && record.installation.version == item.version
                })
                .ok_or_else(|| super::invalid("依赖本地版本缺失"))
                .and_then(local::current_receipt)
        })
        .ok()
        .map(|receipt| receipt.account_scope)
}

async fn plugin(
    context: &Context,
    item: &ReleaseRecoveryItem,
) -> Result<PreparedPluginInstall, AppCommandError> {
    let receipt = if item.skill_id == context.plan.root_skill_id {
        context.root.clone()
    } else {
        local::receipt(&item.slug, &item.version).or_else(|_| {
            context
                .records
                .iter()
                .find(|record| {
                    record.installation.market_skill_id.to_string() == item.skill_id
                        && record.installation.version == item.version
                })
                .ok_or_else(|| super::invalid("依赖本地版本缺失，请先安装精确版本"))
                .and_then(local::current_receipt)
        })?
    };
    let item = item.clone();
    let agents = context.agents.clone();
    let root = context.root_id;
    tokio::task::spawn_blocking(move || prepare_plugin(receipt, &item, (root, &agents)))
        .await
        .map_err(|_| super::invalid("本地插件验证未完成"))?
}

fn prepare_plugin(
    receipt: PluginVersionReceipt,
    item: &ReleaseRecoveryItem,
    target: (i64, &[AgentType]),
) -> Result<PreparedPluginInstall, AppCommandError> {
    if receipt.market.skill_id.to_string() != item.skill_id
        || receipt.market.content_sha256 != item.content_sha256
        || item.plugin.as_ref() != Some(&receipt.manifest)
    {
        return Err(super::invalid("本地插件与原发布记录不符"));
    }
    super::super::plugin_platforms::require_current(&receipt.manifest.platforms)?;
    super::super::plugin_client_version::require_current(&receipt.manifest.min_client_version)?;
    let package = local::package(&receipt)?;
    let plugin = super::super::plugin_manifest::validate_plugin_package(
        &package,
        &receipt.manifest,
        &item.slug,
        &item.version,
    )?;
    let mut marker = receipt.market;
    marker.agent_types = Some(target.1.to_vec());
    marker.target_references = BTreeMap::from([(target.0, target.1.to_vec())]);
    marker.visibility = item.visibility.clone();
    marker.publisher_type = item.publisher_type.clone();
    marker.dependencies = item
        .dependencies
        .iter()
        .map(|value| {
            Ok(MarketSkillDependencyMarker {
                skill_id: value
                    .skill_id
                    .parse()
                    .map_err(|_| super::invalid("插件依赖标识无效"))?,
                slug: value.slug.clone(),
                version: value.version.clone(),
            })
        })
        .collect::<Result<_, AppCommandError>>()?;
    Ok(PreparedPluginInstall {
        market_skill_id: marker.skill_id,
        slug: item.slug.clone(),
        version: item.version.clone(),
        object_sha256: marker.object_sha256.clone(),
        publisher_id: receipt.publisher_id,
        signature_key_id: receipt.signature_key_id,
        package,
        plugin,
        marker,
    })
}

fn size(package: &crate::acp::skill_package::ValidatedSkillPackage) -> usize {
    package.files.iter().map(|file| file.bytes.len()).sum()
}

async fn load_item(
    context: &Context,
    item: &ReleaseRecoveryItem,
    built: &mut Built,
) -> Result<(), AppCommandError> {
    if item.package_type == "plugin" {
        plugin(context, item).await.map(|plugin| {
            built.scope_changed |=
                selected_scope(context, item).as_deref() != Some(context.scope.as_str());
            built.bytes += size(&plugin.package);
            built.plugins.push(plugin);
        })
    } else if crate::commands::office_tools::is_officecli_skill_id(&item.slug) {
        if skills::managed_office_ready(&item.slug).await {
            Ok(())
        } else {
            Err(super::invalid(
                "OfficeCLI依赖未准备完成，请通过办公工具设置更新或修复后重试",
            ))
        }
    } else {
        let item = item.clone();
        let agents = context.agents.clone();
        let id = context.root_id;
        tokio::task::spawn_blocking(move || skills::load(&item, (id, &agents)))
            .await
            .map_err(|_| super::invalid("技能依赖读取未完成"))?
            .map(|skill| {
                built.bytes += size(&skill.package);
                built.skills.push(skill);
            })
    }
}
