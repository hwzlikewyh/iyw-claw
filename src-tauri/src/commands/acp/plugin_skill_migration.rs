use super::*;

/// 只从同市场 ID、同插件和同组件的旧投放继承状态。
pub(super) fn previous_source(source: &Path, marker: &MarketSkillMarker) -> PathBuf {
    if path_entry_exists(source) {
        return source.to_path_buf();
    }
    let (Some(plugin), Some(component)) = (&marker.plugin_slug, &marker.plugin_component_key)
    else {
        return source.to_path_buf();
    };
    let Ok(component) = validate_skill_id(component) else {
        return source.to_path_buf();
    };
    let legacy = shared_skill_path(&component);
    if is_real_skill_directory(&legacy)
        && read_market_skill_marker(&legacy).is_some_and(|old| {
            old.skill_id == marker.skill_id
                && old.plugin_slug.as_ref() == Some(plugin)
                && old.plugin_component_key == marker.plugin_component_key
        })
    {
        return legacy;
    }
    source.to_path_buf()
}

pub(super) fn stage(prepared: &PreparedMarketSkillInstall) -> Result<PathBuf, AcpError> {
    let staging = stage_market_skill_directory(&prepared.source, &prepared.files)?;
    if let Some(legacy) = &prepared.legacy_source {
        if let Err(error) = copy_runtime_envs(legacy, &staging, &prepared.skill_id) {
            cleanup_market_skill_staging(&staging, &prepared.skill_id);
            return Err(error);
        }
        tracing::info!(skill_id=%prepared.skill_id, legacy=%legacy.display(), "[skill-market] 已继承旧插件技能状态，提交成功后释放旧引用");
    }
    Ok(staging)
}
