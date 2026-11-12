use super::{
    invalid, local, prepare, state,
    types::{Inspection, RecoveryInput, RecoveryReview, RecoveryTarget},
};
use crate::app_error::AppCommandError;
use crate::commands::acp::{MarketSkillDependencyMarker, MarketSkillMarker};
use crate::plugin_runtime::{recovery::CurrentPointer, registry};
use std::collections::BTreeMap;

pub async fn apply(
    conn: &sea_orm::DatabaseConnection,
    input: RecoveryInput,
    emitter: crate::web::event_bridge::EventEmitter,
) -> Result<RecoveryReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let inspection = prepare(conn, &input.market_skill_id).await?;
    if inspection.view.state != "ready"
        || inspection.view.review_token.as_deref() != Some(input.review_token.as_str())
    {
        return Err(invalid(
            "插件、包文件、账号或恢复计划已变化，请重新检查后确认",
        ));
    }
    let mut applied = Vec::new();
    for target in &inspection.targets {
        if let Err(error) = commit_target(conn, target, (&inspection, &emitter, &mut applied)).await
        {
            tracing::warn!(plugin=%target.record.installation.slug,code=?error.code,"[plugin-recovery] 提交失败，开始回退已修改记录");
            rollback(conn, &inspection, &applied).await?;
            return Err(error);
        }
    }
    if let Err(error) = verify_commit(conn, &inspection).await {
        rollback(conn, &inspection, &applied).await?;
        return Err(error);
    }
    tracing::info!(plugin=%inspection.record.installation.slug,scope_changed=inspection.view.scope_changed,restored_plugins=applied.len(),"[plugin-recovery] 本地版本安装记录已恢复，未下载或覆盖执行包");
    let mut view = inspection.view;
    view.state = "recovered".into();
    view.review_token = None;
    Ok(view)
}

async fn commit_target<'a>(
    conn: &sea_orm::DatabaseConnection,
    target: &'a RecoveryTarget,
    context: (
        &Inspection,
        &crate::web::event_bridge::EventEmitter,
        &mut Vec<&'a RecoveryTarget>,
    ),
) -> Result<(), AppCommandError> {
    let (inspection, emitter, applied) = context;
    if crate::commands::mcp_cli_config::account_scope(conn).await? != inspection.account_scope {
        return Err(invalid("恢复期间账号或企业空间发生变化，操作已取消"));
    }
    registry::reconcile_global(conn).await?;
    stop_target(conn, target, emitter).await;
    if local::read_pointer(&target.pointer_path)? != target.previous_pointer {
        return Err(invalid("插件指针在恢复期间被其他操作修改"));
    }
    let plan = inspection
        .plan
        .as_ref()
        .ok_or_else(|| invalid("插件恢复元数据缺失"))?;
    let pointer = pointer(target, (&inspection.account_scope, plan))?;
    let bytes = serde_json::to_vec_pretty(&pointer).map_err(|_| invalid("插件恢复指针无法编码"))?;
    applied.push(target);
    state::apply(conn, &target.record, target.scope_changed).await?;
    // 先持久化停用和权限重置，再发布新账号指针，进程中断也不会沿用原权限。
    local::write_pointer(&target.pointer_path, &bytes)?;
    state::finish(conn, &target.record).await
}

async fn stop_target(
    conn: &sea_orm::DatabaseConnection,
    target: &RecoveryTarget,
    emitter: &crate::web::event_bridge::EventEmitter,
) {
    if let Some(plugin) = registry::global_snapshot().and_then(|snapshot| {
        snapshot
            .plugins
            .get(&target.record.installation.slug)
            .cloned()
    }) {
        crate::commands::plugin_preparation::mutation::stop_affected_workspaces(
            conn,
            (&plugin, ""),
            emitter,
        )
        .await;
    }
    registry::suspend_global(&target.record.installation.slug);
}

async fn verify_commit(
    conn: &sea_orm::DatabaseConnection,
    inspection: &Inspection,
) -> Result<(), AppCommandError> {
    if crate::commands::mcp_cli_config::account_scope(conn).await? != inspection.account_scope {
        return Err(invalid("提交期间账号或企业空间变化，请重新检查"));
    }
    registry::reconcile_global(conn).await?;
    for target in &inspection.targets {
        let available = registry::global_snapshot()
            .and_then(|snapshot| {
                snapshot
                    .plugins
                    .get(&target.record.installation.slug)
                    .map(|plugin| plugin.available)
            })
            .unwrap_or(false);
        if !available {
            return Err(invalid(
                "插件恢复后的目录或依赖仍未通过检查，已尝试恢复原记录",
            ));
        }
    }
    Ok(())
}

async fn rollback(
    conn: &sea_orm::DatabaseConnection,
    inspection: &Inspection,
    applied: &[&RecoveryTarget],
) -> Result<(), AppCommandError> {
    let mut failed = false;
    for target in applied.iter().rev() {
        if let Err(error) = restore_target(conn, target).await {
            tracing::error!(plugin=%target.record.installation.slug,code=?error.code,"[plugin-recovery] 原安装状态回退失败");
            failed = true;
        }
    }
    failed |= registry::reconcile_global(conn).await.is_err();
    if failed {
        for target in &inspection.targets {
            registry::suspend_global(&target.record.installation.slug);
        }
        tracing::error!(plugin=%inspection.record.installation.slug,"[plugin-recovery] 恢复失败且原记录回退未完成，保留包和凭据供继续恢复");
        return Err(invalid(
            "恢复未完成且部分旧记录回退失败。包文件和凭据保留，请重新检查安装记录后恢复",
        ));
    }
    Ok(())
}

async fn restore_target(
    conn: &sea_orm::DatabaseConnection,
    target: &RecoveryTarget,
) -> Result<(), AppCommandError> {
    // 指针和权限必须在停用期间恢复，失败时保留停用状态供下次检查。
    state::suspend(conn, &target.record).await?;
    restore_pointer(target)?;
    state::restore(conn, &target.record).await
}

fn restore_pointer(target: &RecoveryTarget) -> Result<(), AppCommandError> {
    match &target.previous_pointer {
        Some(bytes) => local::write_pointer(&target.pointer_path, bytes),
        None => std::fs::remove_file(&target.pointer_path)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|_| invalid("原插件指针无法恢复")),
    }
}

fn pointer(
    target: &RecoveryTarget,
    context: (&str, &super::types::RecoveryPlan),
) -> Result<CurrentPointer, AppCommandError> {
    let (scope, plan) = context;
    let value = &target.record.installation;
    let item = &target.metadata;
    let marker = marker(target)?;
    let plugin_ids = plan
        .items
        .iter()
        .filter(|item| item.package_type == "plugin")
        .map(|item| item.skill_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let dependencies = item
        .dependencies
        .iter()
        .filter(|dependency| plugin_ids.contains(dependency.skill_id.as_str()))
        .map(|dependency| {
            dependency
                .skill_id
                .parse::<i64>()
                .map_err(|_| invalid("插件依赖标识无效"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CurrentPointer {
        version: value.version.clone(),
        content_sha256: value.content_sha256.clone(),
        object_sha256: value.object_sha256.clone(),
        market: Some(marker),
        plugin_dependencies: dependencies,
        account_scope: scope.into(),
    })
}

fn marker(target: &RecoveryTarget) -> Result<MarketSkillMarker, AppCommandError> {
    let value = &target.record.installation;
    let item = &target.metadata;
    let agents: Vec<crate::models::AgentType> = serde_json::from_str(&value.agent_types_json)
        .map_err(|_| invalid("插件目标智能体记录无效"))?;
    Ok(MarketSkillMarker {
        schema_version: 3,
        source: "iyw_skill_market".into(),
        skill_id: value.market_skill_id,
        slug: value.slug.clone(),
        installed_version: value.version.clone(),
        content_sha256: value.content_sha256.clone(),
        object_sha256: value.object_sha256.clone(),
        visibility: item.visibility.clone(),
        publisher_type: item.publisher_type.clone(),
        package_type: "plugin".into(),
        agent_types: Some(agents.clone()),
        target_references: previous_references(target)
            .unwrap_or_else(|| BTreeMap::from([(value.market_skill_id, agents)])),
        dependencies: marker_dependencies(item)?,
        plugin_slug: None,
        plugin_component_key: None,
        installed_at: value.created_at.to_rfc3339(),
    })
}

fn previous_references(
    target: &RecoveryTarget,
) -> Option<BTreeMap<i64, Vec<crate::models::AgentType>>> {
    let pointer: CurrentPointer =
        serde_json::from_slice(target.previous_pointer.as_deref()?).ok()?;
    let marker = pointer.market?;
    let value = &target.record.installation;
    (marker.source == "iyw_skill_market"
        && marker.skill_id == value.market_skill_id
        && marker.slug == value.slug
        && marker.installed_version == value.version
        && !marker.target_references.is_empty())
    .then_some(marker.target_references)
}

fn marker_dependencies(
    item: &super::types::RecoveryItem,
) -> Result<Vec<MarketSkillDependencyMarker>, AppCommandError> {
    item.dependencies
        .iter()
        .map(|dependency| {
            Ok(MarketSkillDependencyMarker {
                skill_id: dependency
                    .skill_id
                    .parse()
                    .map_err(|_| invalid("插件依赖标识无效"))?,
                slug: dependency.slug.clone(),
                version: dependency.version.clone(),
            })
        })
        .collect()
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn plugin_recovery_apply(
    input: RecoveryInput,
    db: tauri::State<'_, crate::db::AppDatabase>,
    app: tauri::AppHandle,
) -> Result<RecoveryReview, AppCommandError> {
    apply(
        &db.conn,
        input,
        crate::web::event_bridge::EventEmitter::Tauri(app),
    )
    .await
}
