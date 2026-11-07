use super::types::{Capability, InspectInput, PointerChange, Prepared, RemovalReview, Retained};
use crate::app_error::AppCommandError;
use crate::commands::acp::MarketSkillMarker;
use crate::db::service::plugin_installation_service::{self, PluginInstallationRecord};
use sea_orm::DatabaseConnection;
use std::collections::{BTreeMap, BTreeSet};

const MAX_PLUGINS: usize = 256;

pub async fn inspect(
    conn: &DatabaseConnection,
    input: InspectInput,
) -> Result<RemovalReview, AppCommandError> {
    let _storage = crate::acp::agent_storage_work::begin_agent_storage_read().await;
    let _operation = crate::plugin_runtime::registry::lock_mutation().await;
    if let Some(view) = super::apply::status(
        conn,
        InspectInput {
            market_skill_id: input.market_skill_id.clone(),
        },
    )
    .await?
    .filter(|view| matches!(view.state.as_str(), "cleanup_pending" | "recovery_required"))
    {
        return Ok(view);
    }
    Ok(prepare(conn, &input.market_skill_id).await?.view)
}

pub(super) async fn prepare(
    conn: &DatabaseConnection,
    id: &str,
) -> Result<Prepared, AppCommandError> {
    crate::commands::mcp::require_private_agent_storage_for_write()?;
    let id = super::super::types::parse_id(id)?;
    let scope = crate::commands::mcp_cli_config::account_scope(conn).await?;
    let records = plugin_installation_service::list_records(conn)
        .await
        .map_err(AppCommandError::db)?;
    if records.len() > MAX_PLUGINS {
        return Err(super::invalid("插件安装过多，无法完整检查移除影响"));
    }
    let root = records
        .iter()
        .find(|record| record.installation.market_skill_id == id)
        .cloned()
        .ok_or_else(|| AppCommandError::not_found("插件已移除，请刷新本地能力列表"))?;
    if let Some(journal) = super::journal::read(conn, &id.to_string(), &scope)
        .await?
        .filter(|journal| {
            matches!(
                journal.phase.as_str(),
                "prepared" | "restoring" | "recovery_required"
            )
        })
    {
        if journal.root_updated_at == root.installation.updated_at.to_rfc3339()
            || matches!(
                root.installation.reconcile_state.as_str(),
                "removing" | "repair_required"
            )
        {
            return Err(super::invalid("上次移除尚未恢复，请先从移除记录恢复原安装"));
        }
    }
    super::super::plugin_history::local::root(&root.installation.slug)?;
    let mut prepared = initial(root, scope, records);
    impacts(&mut prepared)?;
    prepared.cleanup = super::cleanup::plan(conn, &prepared).await?;
    prepared.view.clear_available = prepared.cleanup.available;
    prepared.view.parameter_records = prepared.cleanup.records.len();
    prepared.view.legacy_records_retained = prepared.cleanup.legacy_retained;
    prepared.view.review_token = Some(super::fingerprint::token(&prepared)?);
    Ok(prepared)
}

fn impacts(prepared: &mut Prepared) -> Result<(), AppCommandError> {
    let id = prepared.root.installation.market_skill_id;
    let mut markers = BTreeMap::new();
    let mut retained = Vec::new();
    for record in &prepared.records {
        if let Some(pointer) = super::super::plugin_history::local::pointer(record)
            .ok()
            .flatten()
        {
            if let Some(marker) = pointer.market.filter(|marker| valid_marker(record, marker)) {
                markers.insert(record.installation.market_skill_id, marker);
            }
        }
    }
    let affected = affected_ids(id, &markers);
    for record in &prepared.records {
        let key = record.installation.market_skill_id;
        if key != id && affected.contains(&key) {
            prepared.view.affected.push(Capability {
                id: key.to_string(),
                slug: record.installation.slug.clone(),
            });
        }
        if key != id
            && markers
                .get(&key)
                .is_some_and(|marker| marker.target_references.contains_key(&id))
        {
            retained.push(record.clone());
        }
    }
    for record in retained {
        retain(prepared, record)?;
    }
    if markers.len() != prepared.records.len() {
        prepared
            .view
            .warnings
            .push("部分旧安装缺少可靠依赖快照，移除后请检查这些能力的准备状态".into());
    }
    Ok(())
}

fn valid_marker(record: &PluginInstallationRecord, marker: &MarketSkillMarker) -> bool {
    let value = &record.installation;
    marker.source == "iyw_skill_market"
        && marker.skill_id == value.market_skill_id
        && marker.slug == value.slug
        && marker.installed_version == value.version
}

fn retain(
    prepared: &mut Prepared,
    record: PluginInstallationRecord,
) -> Result<(), AppCommandError> {
    let path =
        super::super::plugin_history::local::root(&record.installation.slug)?.join("current.json");
    let previous = super::super::plugin_storage_io::read_pointer_bytes(&path)?
        .ok_or_else(|| super::invalid("共用插件指针缺失"))?;
    let mut pointer: crate::plugin_runtime::recovery::CurrentPointer =
        serde_json::from_slice(&previous).map_err(|_| super::invalid("共用插件指针损坏"))?;
    let marker = pointer
        .market
        .as_mut()
        .ok_or_else(|| super::invalid("共用插件引用缺失"))?;
    marker
        .target_references
        .remove(&prepared.root.installation.market_skill_id);
    marker.agent_types = Some(
        marker
            .target_references
            .values()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    );
    let shared = !marker.target_references.is_empty();
    let next =
        serde_json::to_vec_pretty(&pointer).map_err(|_| super::invalid("共用插件引用无法编码"))?;
    prepared.pointers.push(PointerChange {
        path,
        previous,
        next,
    });
    prepared.view.retained.push(Retained {
        id: record.installation.market_skill_id.to_string(),
        slug: record.installation.slug,
        shared,
    });
    Ok(())
}

fn initial(
    root: PluginInstallationRecord,
    scope: String,
    records: Vec<PluginInstallationRecord>,
) -> Prepared {
    let id = root.installation.market_skill_id;
    Prepared {
        view: RemovalReview {
            id: id.to_string(),
            slug: root.installation.slug.clone(),
            version: root.installation.version.clone(),
            state: "ready".into(),
            review_token: None,
            affected: Vec::new(),
            retained: Vec::new(),
            warnings: Vec::new(),
            clear_available: false,
            parameter_records: 0,
            legacy_records_retained: false,
        },
        root,
        scope,
        records,
        pointers: Vec::new(),
        cleanup: super::cleanup::Plan::default(),
    }
}

fn affected_ids(id: i64, markers: &BTreeMap<i64, MarketSkillMarker>) -> BTreeSet<i64> {
    let mut affected = BTreeSet::from([id]);
    loop {
        let before = affected.len();
        for (key, marker) in markers {
            if marker
                .dependencies
                .iter()
                .any(|dependency| affected.contains(&dependency.skill_id))
            {
                affected.insert(*key);
            }
        }
        if before == affected.len() {
            break;
        }
    }
    affected
}
