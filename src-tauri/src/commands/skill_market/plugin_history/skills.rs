use crate::app_error::AppCommandError;
use crate::commands::{
    acp::{MarketSkillDependencyMarker, MarketSkillInstall},
    plugin_recovery::ReleaseRecoveryItem,
};
use crate::models::AgentType;
use std::collections::BTreeMap;

pub(super) fn load(
    item: &ReleaseRecoveryItem,
    context: (i64, &[AgentType]),
) -> Result<MarketSkillInstall, AppCommandError> {
    crate::commands::acp::validate_skill_id(&item.slug)
        .map_err(|_| super::invalid("技能依赖路径无效"))?;
    let source = crate::commands::acp::shared_skills_dir().join(&item.slug);
    let mut marker = super::local::read_marker(&source)?
        .ok_or_else(|| super::invalid("技能依赖缺少本地安装记录"))?;
    if marker.skill_id.to_string() != item.skill_id
        || marker.slug != item.slug
        || marker.installed_version != item.version
        || marker.content_sha256 != item.content_sha256
        || marker.plugin_slug.is_some()
        || marker.package_type != "skill"
        || marker.source != "iyw_skill_market"
    {
        return Err(super::invalid("技能依赖需要先安装完整的精确版本"));
    }
    let package =
        crate::acp::skill_package::validate_skill_directory(&source, &item.content_sha256)?;
    marker.target_references = BTreeMap::from([(context.0, context.1.to_vec())]);
    marker.agent_types = Some(context.1.to_vec());
    marker.dependencies = item
        .dependencies
        .iter()
        .map(|value| {
            Ok(MarketSkillDependencyMarker {
                skill_id: value
                    .skill_id
                    .parse()
                    .map_err(|_| super::invalid("技能依赖标识无效"))?,
                slug: value.slug.clone(),
                version: value.version.clone(),
            })
        })
        .collect::<Result<_, AppCommandError>>()?;
    Ok(MarketSkillInstall { marker, package })
}

pub(super) async fn managed_office_ready(slug: &str) -> bool {
    let info = crate::commands::office_tools::officecli_detect().await;
    info.installed
        && info.compatible
        && info.runtime_error.is_none()
        && crate::commands::office_tools::managed_office_dependency_available(slug)
}
