//! 独立的单会话子 Agent 并发设置。

use serde::{Deserialize, Serialize};

use crate::acp::delegation::broker::DelegationBroker;
use crate::acp::registry::SubagentConcurrencyEnforcement;
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::models::AgentType;

pub const KEY_MAX_CONCURRENT_SUBAGENTS: &str = "agent_runtime.max_concurrent_subagents";
pub const DEFAULT_MAX_CONCURRENT_SUBAGENTS: u32 = 40;
pub const MIN_MAX_CONCURRENT_SUBAGENTS: u32 = 1;
pub const MAX_MAX_CONCURRENT_SUBAGENTS: u32 = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConcurrencyInfo {
    pub agent_type: AgentType,
    pub name: String,
    pub enforcement: SubagentConcurrencyEnforcement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConcurrencySettings {
    pub max_concurrent_subagents: u32,
    #[serde(default)]
    pub agents: Vec<AgentConcurrencyInfo>,
}

impl AgentConcurrencySettings {
    fn with_limit(max_concurrent_subagents: u32) -> Self {
        Self {
            max_concurrent_subagents,
            agents: registered_agents(),
        }
    }
}

fn registered_agents() -> Vec<AgentConcurrencyInfo> {
    crate::acp::registry::all_identity_agents()
        .into_iter()
        .map(|agent_type| AgentConcurrencyInfo {
            name: crate::acp::registry::get_agent_meta(agent_type)
                .name
                .to_string(),
            enforcement: crate::acp::registry::subagent_concurrency_enforcement(agent_type),
            agent_type,
        })
        .collect()
}

pub fn clamp_limit(value: u32) -> u32 {
    value.clamp(MIN_MAX_CONCURRENT_SUBAGENTS, MAX_MAX_CONCURRENT_SUBAGENTS)
}

pub async fn load_limit(db: &sea_orm::DatabaseConnection) -> u32 {
    let raw = match app_metadata_service::get_value(db, KEY_MAX_CONCURRENT_SUBAGENTS).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(error = %error, "failed to read Agent concurrency setting; using default");
            return DEFAULT_MAX_CONCURRENT_SUBAGENTS;
        }
    };
    match raw.as_deref().and_then(|value| value.parse::<u32>().ok()) {
        Some(value) if value >= MIN_MAX_CONCURRENT_SUBAGENTS => clamp_limit(value),
        None => {
            if raw.is_some() {
                tracing::warn!("invalid Agent concurrency setting; using default");
            }
            DEFAULT_MAX_CONCURRENT_SUBAGENTS
        }
        Some(_) => {
            tracing::warn!("invalid Agent concurrency setting; using default");
            DEFAULT_MAX_CONCURRENT_SUBAGENTS
        }
    }
}

async fn apply_limit(broker: &DelegationBroker, limit: u32) {
    broker.set_concurrency_limit(limit).await;
    crate::acp::codex_multi_agent::set_max_concurrent_threads(limit);
    crate::acp::connection::set_claude_max_concurrent_subagents(limit);
}

pub async fn apply_persisted_config(
    db: &sea_orm::DatabaseConnection,
    broker: &DelegationBroker,
) -> u32 {
    let limit = load_limit(db).await;
    apply_limit(broker, limit).await;
    limit
}

pub async fn get_agent_concurrency_settings_core(
    db: &sea_orm::DatabaseConnection,
) -> Result<AgentConcurrencySettings, AppCommandError> {
    Ok(AgentConcurrencySettings::with_limit(load_limit(db).await))
}

pub async fn set_agent_concurrency_settings_core(
    db: &sea_orm::DatabaseConnection,
    broker: &DelegationBroker,
    settings: AgentConcurrencySettings,
) -> Result<AgentConcurrencySettings, AppCommandError> {
    let limit = clamp_limit(settings.max_concurrent_subagents);
    app_metadata_service::upsert_value(db, KEY_MAX_CONCURRENT_SUBAGENTS, &limit.to_string())
        .await
        .map_err(AppCommandError::from)?;
    apply_limit(broker, limit).await;
    tracing::info!(
        max_concurrent_subagents = limit,
        "Agent concurrency setting updated"
    );
    Ok(AgentConcurrencySettings::with_limit(limit))
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_agent_concurrency_settings(
    db: tauri::State<'_, crate::db::AppDatabase>,
) -> Result<AgentConcurrencySettings, AppCommandError> {
    get_agent_concurrency_settings_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn set_agent_concurrency_settings(
    settings: AgentConcurrencySettings,
    db: tauri::State<'_, crate::db::AppDatabase>,
    broker: tauri::State<'_, std::sync::Arc<DelegationBroker>>,
) -> Result<AgentConcurrencySettings, AppCommandError> {
    set_agent_concurrency_settings_core(&db.conn, broker.inner(), settings).await
}
