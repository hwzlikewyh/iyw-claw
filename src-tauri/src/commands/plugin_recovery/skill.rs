use super::{
    invalid,
    types::{DependencyView, RecoveryItem},
};
use crate::app_error::AppCommandError;

pub(super) async fn inspect(item: &RecoveryItem) -> Result<DependencyView, AppCommandError> {
    let item = item.clone();
    tokio::task::spawn_blocking(move || verify(&item))
        .await
        .map_err(|_| invalid("技能依赖验证任务未完成"))?
}

fn verify(item: &RecoveryItem) -> Result<DependencyView, AppCommandError> {
    if item.slug.is_empty()
        || item
            .slug
            .chars()
            .any(|value| !value.is_ascii_alphanumeric() && value != '-')
    {
        return Err(invalid("技能依赖路径无效"));
    }
    let source = crate::commands::acp::shared_skills_dir().join(&item.slug);
    let marker = crate::commands::acp::read_market_skill_marker(&source);
    let installed = marker.as_ref().map(|value| value.installed_version.clone());
    let matches = marker.is_some_and(|value| {
        value.source == "iyw_skill_market"
            && value.skill_id.to_string() == item.skill_id
            && value.slug == item.slug
            && value.installed_version == item.version
            && value.package_type == "skill"
            && value.plugin_slug.is_none()
            && value.content_sha256 == item.content_sha256
    });
    let ready = matches
        && crate::acp::skill_package::validate_skill_directory(&source, &item.content_sha256)
            .is_ok();
    Ok(DependencyView {
        id: item.skill_id.clone(),
        slug: item.slug.clone(),
        required_version: item.version.clone(),
        installed_version: installed,
        ready,
    })
}
