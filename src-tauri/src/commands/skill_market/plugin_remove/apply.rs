use super::{
    journal::Journal,
    types::{ApplyInput, InspectInput, Prepared, RemovalReview},
};
use crate::app_error::AppCommandError;
use crate::db::service::plugin_installation_service;
use crate::plugin_runtime::{global, registry};
use crate::web::event_bridge::EventEmitter;
use sea_orm::DatabaseConnection;

pub async fn apply(
    conn: &DatabaseConnection,
    input: ApplyInput,
    emitter: EventEmitter,
) -> Result<RemovalReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let prepared = super::inspect::prepare(conn, &input.market_skill_id).await?;
    if prepared.view.review_token.as_deref() != Some(input.review_token.as_str()) {
        return Err(super::invalid(
            "账号、安装或依赖已变化，请重新检查移除影响后确认",
        ));
    }
    if input.clear_account_data && !prepared.cleanup.available {
        return Err(super::invalid("无法确认当前账号数据归属，不能清理此范围"));
    }
    if crate::commands::mcp_cli_config::account_scope(conn).await? != prepared.scope {
        return Err(super::invalid("账号范围已变化，请重新检查移除影响"));
    }
    let mut journal = make_journal(&prepared, input.clear_account_data);
    pause(conn, &prepared, &emitter).await?;
    super::commit::commit(conn, &prepared, &mut journal).await?;
    let mut view = prepared.view;
    view.review_token = None;
    view.state = if super::journal::finish(conn, &mut journal)
        .await
        .unwrap_or(false)
    {
        "removed"
    } else {
        "cleanup_pending"
    }
    .into();
    tracing::info!(plugin=%view.slug,clear_account_data=input.clear_account_data,state=%view.state,"[plugin-remove] 插件逻辑移除已提交");
    Ok(view)
}

async fn pause(
    conn: &DatabaseConnection,
    prepared: &Prepared,
    emitter: &EventEmitter,
) -> Result<(), AppCommandError> {
    registry::reconcile_global(conn).await?;
    let mut slugs = vec![prepared.root.installation.slug.clone()];
    slugs.extend(
        prepared
            .view
            .affected
            .iter()
            .map(|value| value.slug.clone()),
    );
    slugs.extend(
        prepared
            .view
            .retained
            .iter()
            .filter(|value| !value.shared)
            .map(|value| value.slug.clone()),
    );
    slugs.sort();
    slugs.dedup();
    for slug in &slugs {
        registry::suspend_global(slug);
    }
    for slug in &slugs {
        if global::plugin_version_has_active_leases(slug, None).await {
            let _ = registry::reconcile_global(conn).await;
            return Err(super::invalid(&format!(
                "插件 {slug} 仍在使用，请关闭插件页面或等待工具调用完成后重试"
            )));
        }
    }
    crate::commands::plugin_preparation::mutation::stop_plugins_for_update(conn, &slugs, emitter)
        .await;
    Ok(())
}

pub async fn status(
    conn: &DatabaseConnection,
    input: InspectInput,
) -> Result<Option<RemovalReview>, AppCommandError> {
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    super::super::types::parse_id(&input.market_skill_id)?;
    let journal = super::journal::read(conn, &input.market_skill_id, &scope)
        .await?
        .filter(|journal| journal.phase != "cancelled");
    let current = plugin_installation_service::find_by_market_skill_id(
        conn,
        super::super::types::parse_id(&input.market_skill_id)?,
    )
    .await
    .map_err(AppCommandError::db)?;
    Ok(journal
        .filter(|journal| {
            current.as_ref().is_none_or(|record| {
                record.installation.updated_at.to_rfc3339() == journal.root_updated_at
                    || matches!(
                        record.installation.reconcile_state.as_str(),
                        "removing" | "repair_required"
                    )
                    || journal.phase == "restoring"
            })
        })
        .map(review))
}

pub async fn retry_cleanup(
    conn: &DatabaseConnection,
    input: InspectInput,
) -> Result<RemovalReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = registry::lock_mutation().await;
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let id = super::super::types::parse_id(&input.market_skill_id)?;
    if plugin_installation_service::find_by_market_skill_id(conn, id)
        .await
        .map_err(AppCommandError::db)?
        .is_some()
    {
        return Err(super::invalid(
            "插件已重新安装，不能沿用旧清理记录；请从当前安装重新检查",
        ));
    }
    let mut journal = super::journal::read(conn, &input.market_skill_id, &scope)
        .await?
        .ok_or_else(|| super::invalid("没有待继续的移除记录"))?;
    if journal.phase == "complete" {
        return Ok(review(journal));
    }
    let _ = super::journal::finish(conn, &mut journal).await?;
    Ok(review(journal))
}

fn review(journal: Journal) -> RemovalReview {
    RemovalReview {
        id: journal.plan.id,
        slug: journal.plan.slug,
        version: String::new(),
        state: match journal.phase.as_str() {
            "complete" => "removed",
            "committed" => "cleanup_pending",
            _ => "recovery_required",
        }
        .into(),
        review_token: None,
        affected: Vec::new(),
        retained: Vec::new(),
        warnings: Vec::new(),
        clear_available: journal.plan.available,
        parameter_records: journal.plan.records.len(),
        legacy_records_retained: journal.plan.legacy_retained,
    }
}

fn make_journal(prepared: &Prepared, clear: bool) -> Journal {
    Journal {
        plan: prepared.cleanup.clone(),
        clear: clear,
        phase: "prepared".into(),
        operation: uuid::Uuid::new_v4().to_string(),
        backups: prepared
            .records
            .iter()
            .filter(|record| {
                record.installation.market_skill_id == prepared.root.installation.market_skill_id
                    || prepared
                        .view
                        .affected
                        .iter()
                        .any(|value| value.id == record.installation.market_skill_id.to_string())
                    || prepared.view.retained.iter().any(|value| {
                        value.id == record.installation.market_skill_id.to_string() && !value.shared
                    })
            })
            .map(super::super::plugin_install_data::record_input)
            .collect(),
        pointers: prepared
            .pointers
            .iter()
            .map(|change| super::journal::PointerBackup {
                slug: change
                    .path
                    .parent()
                    .and_then(|path| path.file_name())
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                previous: change.previous.clone(),
                next: change.next.clone(),
            })
            .collect(),
        skills: None,
        catalog_ref: None,
        root_updated_at: prepared.root.installation.updated_at.to_rfc3339(),
    }
}
