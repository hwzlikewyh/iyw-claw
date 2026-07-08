use std::collections::HashMap;

use chrono::Utc;
use sea_orm::DatabaseConnection;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, Set, Statement,
};

use crate::db::entities::agent_setting;
use crate::db::error::DbError;
use crate::models::agent::AgentType;

#[derive(Debug, Clone)]
pub struct AgentDefaultInput {
    pub agent_type: AgentType,
    pub registry_id: String,
    pub default_sort_order: i32,
}

#[derive(Debug, Clone)]
pub struct AgentSettingsUpdate {
    pub enabled: bool,
    pub env_json: Option<String>,
    pub model_provider_id: Option<i32>,
}

pub(crate) fn default_enabled(agent_type: AgentType) -> bool {
    matches!(agent_type, AgentType::Codex)
}

pub async fn ensure_defaults(
    conn: &DatabaseConnection,
    defaults: &[AgentDefaultInput],
) -> Result<(), DbError> {
    for default in defaults {
        let agent_type = serde_json::to_string(&default.agent_type)
            .map_err(|e| DbError::Migration(format!("agent_type serialize failed: {e}")))?;
        let existing = agent_setting::Entity::find()
            .filter(agent_setting::Column::AgentType.eq(agent_type.clone()))
            .one(conn)
            .await?;

        if let Some(model) = existing {
            if model.registry_id != default.registry_id {
                let mut active = model.into_active_model();
                active.registry_id = Set(default.registry_id.clone());
                active.updated_at = Set(Utc::now());
                active.update(conn).await?;
            }
            continue;
        }

        let now = Utc::now();
        let active = agent_setting::ActiveModel {
            id: NotSet,
            agent_type: Set(agent_type),
            registry_id: Set(default.registry_id.clone()),
            enabled: Set(default_enabled(default.agent_type)),
            sort_order: Set(default.default_sort_order),
            installed_version: Set(None),
            env_json: Set(None),
            model_provider_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        match active.insert(conn).await {
            Ok(_) => {}
            Err(e) if e.to_string().contains("UNIQUE constraint failed") => {
                // Another concurrent call already inserted this row — safe to ignore.
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}

pub async fn list(conn: &DatabaseConnection) -> Result<Vec<agent_setting::Model>, DbError> {
    let rows = agent_setting::Entity::find()
        .order_by_asc(agent_setting::Column::SortOrder)
        .all(conn)
        .await?;
    Ok(rows)
}

pub async fn list_map_by_agent_type(
    conn: &DatabaseConnection,
) -> Result<HashMap<AgentType, agent_setting::Model>, DbError> {
    let rows = list(conn).await?;
    let mut map = HashMap::new();
    for row in rows {
        if let Ok(agent_type) = serde_json::from_str::<AgentType>(&row.agent_type) {
            map.insert(agent_type, row);
        }
    }
    Ok(map)
}

pub async fn get_by_agent_type(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<Option<agent_setting::Model>, DbError> {
    let agent_type_str = serde_json::to_string(&agent_type)
        .map_err(|e| DbError::Migration(format!("agent_type serialize failed: {e}")))?;
    let model = agent_setting::Entity::find()
        .filter(agent_setting::Column::AgentType.eq(agent_type_str))
        .one(conn)
        .await?;
    Ok(model)
}

pub async fn update(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    patch: AgentSettingsUpdate,
) -> Result<(), DbError> {
    let agent_type_str = serde_json::to_string(&agent_type)
        .map_err(|e| DbError::Migration(format!("agent_type serialize failed: {e}")))?;
    let model = agent_setting::Entity::find()
        .filter(agent_setting::Column::AgentType.eq(agent_type_str.clone()))
        .one(conn)
        .await?
        .ok_or_else(|| DbError::Migration(format!("agent setting not found: {agent_type_str}")))?;

    let mut active = model.into_active_model();
    active.enabled = Set(patch.enabled);
    active.env_json = Set(patch.env_json);
    active.model_provider_id = Set(patch.model_provider_id);
    active.updated_at = Set(Utc::now());
    active.update(conn).await?;
    Ok(())
}

pub async fn set_installed_version(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    installed_version: Option<String>,
) -> Result<(), DbError> {
    let agent_type_str = serde_json::to_string(&agent_type)
        .map_err(|e| DbError::Migration(format!("agent_type serialize failed: {e}")))?;
    if let Some(model) = agent_setting::Entity::find()
        .filter(agent_setting::Column::AgentType.eq(agent_type_str))
        .one(conn)
        .await?
    {
        let mut active = model.into_active_model();
        active.installed_version = Set(installed_version);
        active.updated_at = Set(Utc::now());
        active.update(conn).await?;
    }
    Ok(())
}

pub async fn reorder(conn: &DatabaseConnection, agent_types: &[AgentType]) -> Result<(), DbError> {
    if agent_types.is_empty() {
        return Ok(());
    }

    match reorder_once(conn, agent_types).await {
        Ok(()) => Ok(()),
        Err(err) if is_sqlite_full_error(&err) => {
            // Try truncating WAL once to reclaim space and retry.
            conn.execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA wal_checkpoint(TRUNCATE);".to_owned(),
            ))
            .await?;
            reorder_once(conn, agent_types).await
        }
        Err(err) => Err(err),
    }
}

pub async fn reorder_if_current_order_matches(
    conn: &DatabaseConnection,
    current_order: &[AgentType],
    next_order: &[AgentType],
) -> Result<bool, DbError> {
    if current_order == next_order || current_order.is_empty() || next_order.is_empty() {
        return Ok(false);
    }

    let rows = list(conn).await?;
    let mut sort_by_agent = HashMap::new();
    for row in rows {
        if let Ok(agent_type) = serde_json::from_str::<AgentType>(&row.agent_type) {
            sort_by_agent.insert(agent_type, row.sort_order);
        }
    }

    if current_order
        .iter()
        .any(|agent_type| !sort_by_agent.contains_key(agent_type))
    {
        return Ok(false);
    }

    let mut sorted_current = current_order.to_vec();
    sorted_current
        .sort_by_key(|agent_type| sort_by_agent.get(agent_type).copied().unwrap_or(i32::MAX));
    if sorted_current != current_order {
        return Ok(false);
    }

    reorder(conn, next_order).await?;
    Ok(true)
}

pub async fn reset_enabled_to_defaults(
    conn: &DatabaseConnection,
    agent_types: &[AgentType],
) -> Result<(), DbError> {
    let now = Utc::now();
    for agent_type in agent_types {
        let agent_type_str = serde_json::to_string(agent_type)
            .map_err(|e| DbError::Migration(format!("agent_type serialize failed: {e}")))?;
        let Some(model) = agent_setting::Entity::find()
            .filter(agent_setting::Column::AgentType.eq(agent_type_str))
            .one(conn)
            .await?
        else {
            continue;
        };
        let enabled = default_enabled(*agent_type);
        if model.enabled == enabled {
            continue;
        }
        let mut active = model.into_active_model();
        active.enabled = Set(enabled);
        active.updated_at = Set(now);
        active.update(conn).await?;
    }
    Ok(())
}

async fn reorder_once(conn: &DatabaseConnection, agent_types: &[AgentType]) -> Result<(), DbError> {
    let now = Utc::now();
    for (index, agent_type) in agent_types.iter().enumerate() {
        let agent_type_str = serde_json::to_string(agent_type)
            .map_err(|e| DbError::Migration(format!("agent_type serialize failed: {e}")))?;

        if let Some(model) = agent_setting::Entity::find()
            .filter(agent_setting::Column::AgentType.eq(agent_type_str))
            .one(conn)
            .await?
        {
            // Skip unchanged rows to reduce write pressure when repeatedly dragging.
            if model.sort_order == index as i32 {
                continue;
            }
            let mut active = model.into_active_model();
            active.sort_order = Set(index as i32);
            active.updated_at = Set(now);
            active.update(conn).await?;
        }
    }
    Ok(())
}

pub async fn find_by_model_provider_id(
    conn: &DatabaseConnection,
    model_provider_id: i32,
) -> Result<Vec<agent_setting::Model>, DbError> {
    let rows = agent_setting::Entity::find()
        .filter(agent_setting::Column::ModelProviderId.eq(Some(model_provider_id)))
        .all(conn)
        .await?;
    Ok(rows)
}

fn is_sqlite_full_error(err: &DbError) -> bool {
    let message = err.to_string();
    message.contains("database or disk is full") || message.contains("(code: 13)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::fresh_in_memory_db;

    fn defaults_for(order: &[AgentType]) -> Vec<AgentDefaultInput> {
        order
            .iter()
            .enumerate()
            .map(|(index, agent_type)| AgentDefaultInput {
                agent_type: *agent_type,
                registry_id: format!("{agent_type:?}"),
                default_sort_order: index as i32,
            })
            .collect()
    }

    async fn ordered_types(conn: &DatabaseConnection) -> Vec<AgentType> {
        list(conn)
            .await
            .expect("list agent settings")
            .into_iter()
            .map(|row| serde_json::from_str::<AgentType>(&row.agent_type).unwrap())
            .collect()
    }

    #[test]
    fn default_enabled_is_codex_only() {
        for agent_type in [
            AgentType::ClaudeCode,
            AgentType::Codex,
            AgentType::OpenCode,
            AgentType::Gemini,
            AgentType::OpenClaw,
            AgentType::Cline,
            AgentType::Hermes,
            AgentType::CodeBuddy,
            AgentType::KimiCode,
            AgentType::Pi,
        ] {
            assert_eq!(default_enabled(agent_type), agent_type == AgentType::Codex);
        }
    }

    #[tokio::test]
    async fn ensure_defaults_enables_only_codex_for_new_rows() {
        let db = fresh_in_memory_db().await;
        let order = [AgentType::Codex, AgentType::Hermes, AgentType::OpenCode];

        ensure_defaults(&db.conn, &defaults_for(&order))
            .await
            .expect("ensure defaults");

        let rows = list_map_by_agent_type(&db.conn).await.expect("list map");
        assert!(rows.get(&AgentType::Codex).unwrap().enabled);
        assert!(!rows.get(&AgentType::Hermes).unwrap().enabled);
        assert!(!rows.get(&AgentType::OpenCode).unwrap().enabled);
    }

    #[tokio::test]
    async fn reorder_if_current_order_matches_updates_legacy_defaults() {
        let db = fresh_in_memory_db().await;
        let legacy = [AgentType::ClaudeCode, AgentType::Codex, AgentType::Hermes];
        let next = [AgentType::Codex, AgentType::Hermes, AgentType::ClaudeCode];
        ensure_defaults(&db.conn, &defaults_for(&legacy))
            .await
            .expect("ensure legacy defaults");

        let migrated = reorder_if_current_order_matches(&db.conn, &legacy, &next)
            .await
            .expect("migrate legacy order");

        assert!(migrated);
        assert_eq!(ordered_types(&db.conn).await, next.to_vec());
    }

    #[tokio::test]
    async fn reorder_if_current_order_matches_preserves_manual_order() {
        let db = fresh_in_memory_db().await;
        let legacy = [AgentType::ClaudeCode, AgentType::Codex, AgentType::Hermes];
        let manual = [AgentType::Hermes, AgentType::Codex, AgentType::ClaudeCode];
        let next = [AgentType::Codex, AgentType::Hermes, AgentType::ClaudeCode];
        ensure_defaults(&db.conn, &defaults_for(&legacy))
            .await
            .expect("ensure legacy defaults");
        reorder(&db.conn, &manual).await.expect("manual reorder");

        let migrated = reorder_if_current_order_matches(&db.conn, &legacy, &next)
            .await
            .expect("skip manual order");

        assert!(!migrated);
        assert_eq!(ordered_types(&db.conn).await, manual.to_vec());
    }

    #[tokio::test]
    async fn reset_enabled_to_defaults_keeps_only_codex_enabled() {
        let db = fresh_in_memory_db().await;
        let order = [AgentType::Codex, AgentType::Hermes];
        ensure_defaults(&db.conn, &defaults_for(&order))
            .await
            .expect("ensure defaults");
        update(
            &db.conn,
            AgentType::Hermes,
            AgentSettingsUpdate {
                enabled: true,
                env_json: None,
                model_provider_id: None,
            },
        )
        .await
        .expect("enable hermes");

        reset_enabled_to_defaults(&db.conn, &order)
            .await
            .expect("reset enabled");

        let rows = list_map_by_agent_type(&db.conn).await.expect("list map");
        assert!(rows.get(&AgentType::Codex).unwrap().enabled);
        assert!(!rows.get(&AgentType::Hermes).unwrap().enabled);
    }
}
