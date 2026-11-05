use std::collections::{BTreeMap, BTreeSet};

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app_error::AppCommandError;
use crate::db::entities::conversation;
use crate::db::service::app_metadata_service;
use crate::models::AgentType;

const PREFIX: &str = "mcp.conversation.v1.";
const MAX_SELECTION: usize = 64;

#[derive(Default, Serialize, Deserialize)]
pub struct Selection {
    pub revision: u64,
    pub ids: BTreeSet<String>,
}

#[derive(Serialize)]
pub struct ConnectorChoice {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub compatible: bool,
    pub reason: Option<String>,
    pub requires_migration: bool,
}

#[derive(Serialize)]
pub struct ConversationConnectors {
    pub revision: u64,
    pub selected: BTreeSet<String>,
    pub connectors: Vec<ConnectorChoice>,
    pub reconnect_required: bool,
    pub migrated: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConnectors {
    pub conversation_id: i32,
    pub revision: u64,
    pub selected: BTreeSet<String>,
}

pub async fn read(
    conn: &DatabaseConnection,
    id: i32,
) -> Result<ConversationConnectors, AppCommandError> {
    let row = require_conversation(conn, id).await?;
    let agent = AgentType::from_wire(&row.agent_type)
        .ok_or_else(|| AppCommandError::invalid_input("对话智能体类型无效"))?;
    choices(conn, (Some(id), agent)).await
}

pub async fn query(
    conn: &DatabaseConnection,
    context: (Option<i32>, Option<AgentType>),
) -> Result<ConversationConnectors, AppCommandError> {
    if let Some(id) = context.0 {
        return read(conn, id).await;
    }
    let agent = context
        .1
        .ok_or_else(|| AppCommandError::invalid_input("请选择对话智能体"))?;
    choices(conn, (None, agent)).await
}

async fn choices(
    conn: &DatabaseConnection,
    context: (Option<i32>, AgentType),
) -> Result<ConversationConnectors, AppCommandError> {
    let selection = load(conn, context.0).await?;
    let catalog =
        super::mcp_catalog::load_or_import(conn, super::mcp::scan_legacy_server_specs).await?;
    let mut connectors: Vec<ConnectorChoice> = catalog
        .servers
        .iter()
        .filter(|(id, entry)| {
            entry.spec.get("iyw_builtin").is_none() && id.as_str() != "iyw-capability-gateway"
        })
        .map(|(key, entry)| {
            let reason =
                super::mcp_conversation_compatibility::incompatibility(context.1, &entry.spec)
                    .or_else(|| {
                        (!entry.missing_config.is_empty()).then(|| "连接器仍需补充参数".to_string())
                    });
            ConnectorChoice {
                id: key.clone(),
                name: entry
                    .display_name
                    .clone()
                    .or_else(|| entry.spec["iyw_market"]["name"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| key.clone()),
                protocol: entry.spec["type"].as_str().unwrap_or("stdio").to_string(),
                compatible: reason.is_none(),
                reason,
                requires_migration: !entry.managed
                    || (entry.enabled && entry.spec.get("iyw_market").is_none()),
            }
        })
        .collect();
    for id in selection
        .ids
        .iter()
        .filter(|id| !catalog.servers.contains_key(*id))
    {
        connectors.push(ConnectorChoice {
            id: id.clone(),
            name: id.clone(),
            protocol: String::new(),
            compatible: false,
            reason: Some("此连接器已卸载，请重新添加或关闭本对话选择".into()),
            requires_migration: false,
        });
    }
    Ok(ConversationConnectors {
        revision: selection.revision,
        selected: selection.ids,
        connectors,
        reconnect_required: super::mcp_conversation_applied::needs_reconnect(
            context.0,
            selection.revision,
        ),
        migrated: Vec::new(),
    })
}

pub async fn update(
    conn: &DatabaseConnection,
    input: UpdateConnectors,
) -> Result<ConversationConnectors, AppCommandError> {
    if input.selected.len() > MAX_SELECTION {
        return Err(AppCommandError::invalid_input("本对话连接器数量超过限制"));
    }
    let current = read(conn, input.conversation_id).await?;
    for id in input.selected.difference(&current.selected) {
        let choice = current
            .connectors
            .iter()
            .find(|item| &item.id == id)
            .ok_or_else(|| AppCommandError::not_found("连接器已卸载"))?;
        if !choice.compatible {
            return Err(AppCommandError::invalid_input(
                choice.reason.clone().unwrap_or_default(),
            ));
        }
    }
    let _guard = super::mcp_catalog::lock_operation().await;
    let saved = load(conn, Some(input.conversation_id)).await?;
    if saved.revision != input.revision {
        return Err(AppCommandError::invalid_input(
            "对话连接器选择已变更，请刷新后重试",
        ));
    }
    let next = Selection {
        revision: saved.revision + 1,
        ids: input.selected.clone(),
    };
    let newly_selected = next
        .ids
        .difference(&current.selected)
        .cloned()
        .collect::<BTreeSet<_>>();
    let migrated =
        super::mcp_conversation_migration::migrate_selected(conn, &newly_selected, &next.ids)
            .await?;
    app_metadata_service::upsert_value(
        conn,
        &format!("{PREFIX}{}", input.conversation_id),
        &serde_json::to_string(&next)
            .map_err(|_| AppCommandError::invalid_input("连接器选择无法序列化"))?,
    )
    .await
    .map_err(AppCommandError::db)?;
    tracing::info!(
        conversation_id = input.conversation_id,
        selected_count = next.ids.len(),
        revision = next.revision,
        "[MCP] 对话连接器选择已保存"
    );
    Ok(ConversationConnectors {
        revision: next.revision,
        selected: next.ids,
        reconnect_required: super::mcp_conversation_applied::needs_reconnect(
            Some(input.conversation_id),
            next.revision,
        ),
        migrated,
        ..current
    })
}

pub(crate) async fn load(
    conn: &DatabaseConnection,
    id: Option<i32>,
) -> Result<Selection, AppCommandError> {
    let Some(id) = id.filter(|id| *id > 0) else {
        return Ok(Selection::default());
    };
    let raw = app_metadata_service::get_value(conn, &format!("{PREFIX}{id}"))
        .await
        .map_err(AppCommandError::db)?;
    raw.map(|raw| {
        serde_json::from_str(&raw)
            .map_err(|_| AppCommandError::configuration_invalid("对话连接器选择记录损坏"))
    })
    .transpose()
    .map(Option::unwrap_or_default)
}

pub(crate) async fn specs(
    conn: &DatabaseConnection,
    context: (Option<i32>, AgentType),
) -> Result<BTreeMap<String, Value>, AppCommandError> {
    let selected = load(conn, context.0).await?;
    let catalog =
        super::mcp_catalog::load_or_import(conn, super::mcp::scan_legacy_server_specs).await?;
    let mut specs = BTreeMap::new();
    for id in selected.ids {
        let Some(entry) = catalog.servers.get(&id) else {
            tracing::warn!(connector=%id, "[MCP] 对话所选连接器已卸载");
            continue;
        };
        if let Some(reason) =
            super::mcp_conversation_compatibility::incompatibility(context.1, &entry.spec)
        {
            return Err(AppCommandError::configuration_invalid(reason));
        }
        let spec = super::mcp::hydrate_market_spec(&entry.spec)?;
        specs.insert(id, spec);
    }
    super::mcp_conversation_applied::record(context.0, selected.revision);
    Ok(specs)
}

async fn require_conversation(
    conn: &DatabaseConnection,
    id: i32,
) -> Result<conversation::Model, AppCommandError> {
    conversation::Entity::find_by_id(id)
        .filter(conversation::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .map_err(|error| AppCommandError::db(error.into()))?
        .ok_or_else(|| AppCommandError::not_found("对话不存在"))
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_conversation_get(
    conversation_id: Option<i32>,
    agent_type: Option<AgentType>,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<ConversationConnectors, AppCommandError> {
    query(&db.conn, (conversation_id, agent_type)).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mcp_conversation_set(
    input: UpdateConnectors,
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<ConversationConnectors, AppCommandError> {
    update(&db.conn, input).await
}
