use super::{
    prepare,
    types::{ApplyInput, InspectInput, Prepared, VersionReview},
};
use crate::app_error::AppCommandError;
use crate::plugin_runtime::{global, registry};
use crate::web::event_bridge::EventEmitter;
use sea_orm::DatabaseConnection;

pub async fn apply(
    conn: &DatabaseConnection,
    input: ApplyInput,
    emitter: EventEmitter,
) -> Result<VersionReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let prepared = prepare::prepare(
        conn,
        &InspectInput {
            market_skill_id: input.market_skill_id,
            version: input.version,
        },
    )
    .await?;
    if prepared.view.state != "ready"
        || prepared.view.review_token.as_deref() != Some(input.review_token.as_str())
    {
        return Err(super::invalid(
            "当前安装、账号、包或依赖已变化，请重新检查后确认",
        ));
    }
    let paused = pause(conn, &prepared, &emitter).await?;
    let result = super::super::plugin_install::install_market_plan_locked(
        super::super::plugin_install_context::MarketInstallPlanExecution {
            conn,
            account_scope: prepared.scope,
            agent_types: &prepared.agents,
            root_skill_id: prepared.current.installation.market_skill_id,
            skill_installs: prepared.skills,
            plugins: prepared.plugins,
        },
    )
    .await;
    if let Err(error) = result {
        if let Err(reconcile) = registry::reconcile_global(conn).await {
            for slug in &paused {
                registry::suspend_global(slug);
            }
            tracing::error!(error=%reconcile,"[plugin-history] 切换失败后目录恢复未完成");
        }
        return Err(error);
    }
    tracing::info!(plugin=%prepared.view.slug,from=%prepared.view.current_version,to=%prepared.view.version,"[plugin-history] 本地版本已切换，仍需重新检查准备状态");
    let mut view = prepared.view;
    view.state = "applied".into();
    view.review_token = None;
    Ok(view)
}

async fn pause(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    emitter: &EventEmitter,
) -> Result<Vec<String>, AppCommandError> {
    registry::reconcile_global(conn).await?;
    let Some(snapshot) = registry::global_snapshot() else {
        return Err(super::invalid("插件运行目录未初始化"));
    };
    let ids = prepared
        .plugins
        .iter()
        .map(|plugin| plugin.market_skill_id.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let affected = snapshot
        .plugins
        .values()
        .filter(|plugin| {
            ids.contains(&plugin.market_skill_id.to_string())
                || crate::plugin_runtime::dependencies::inspect(plugin, &snapshot.plugins)
                    .dependencies
                    .iter()
                    .any(|dependency| ids.contains(&dependency.id))
        })
        .map(|plugin| plugin.slug.clone())
        .collect::<Vec<_>>();
    for slug in &affected {
        if global::plugin_version_has_active_leases(slug, None).await {
            return Err(super::invalid(&format!(
                "插件 {slug} 仍在使用，请关闭相关插件页面或等待工具完成后重新检查"
            )));
        }
    }
    for slug in &affected {
        registry::suspend_global(slug);
    }
    crate::commands::plugin_preparation::mutation::stop_plugins_for_update(
        conn, &affected, emitter,
    )
    .await;
    Ok(affected)
}
