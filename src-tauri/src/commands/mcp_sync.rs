//! Projection of managed MCP catalog entries into agent-native configuration.

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::models::agent::AgentType;

use super::mcp_catalog::ManagedMcpCatalog;

pub fn is_managed_mcp_target(agent_type: AgentType) -> bool {
    matches!(
        agent_type,
        AgentType::ClaudeCode
            | AgentType::Codex
            | AgentType::OpenCode
            | AgentType::Gemini
            | AgentType::Cline
            | AgentType::Hermes
            | AgentType::CodeBuddy
            | AgentType::KimiCode
    )
}

pub async fn managed_target_agents(
    conn: &DatabaseConnection,
) -> Result<Vec<AgentType>, AppCommandError> {
    let settings = agent_setting_service::list_map_by_agent_type(conn)
        .await
        .map_err(AppCommandError::db)?;
    Ok(crate::acp::registry::all_acp_agents()
        .into_iter()
        .filter(|agent_type| is_managed_mcp_target(*agent_type))
        .filter(|agent_type| {
            settings
                .get(agent_type)
                .map(|setting| setting.enabled)
                .unwrap_or_else(|| agent_setting_service::default_enabled(*agent_type))
        })
        .collect())
}

pub fn reconcile_catalog_for_agent_with<U, R>(
    catalog: &ManagedMcpCatalog,
    agent_enabled: bool,
    mut upsert: U,
    mut remove: R,
) -> Result<(), AppCommandError>
where
    U: FnMut(&str, &serde_json::Value) -> Result<(), AppCommandError>,
    R: FnMut(&str) -> Result<bool, AppCommandError>,
{
    if !agent_enabled {
        return Ok(());
    }
    let mut failures = Vec::new();
    for (server_id, entry) in &catalog.servers {
        if !entry.managed {
            continue;
        }
        if entry.enabled {
            if let Err(error) = upsert(server_id, &entry.spec) {
                failures.push(format_reconcile_failure("upsert", server_id, &error));
            }
        } else {
            if let Err(error) = remove(server_id) {
                failures.push(format_reconcile_failure("remove", server_id, &error));
            }
        }
    }
    for server_id in &catalog.tombstones {
        if let Err(error) = remove(server_id) {
            failures.push(format_reconcile_failure("remove", server_id, &error));
        }
    }
    finish_reconcile("Managed MCP agent projection failed", failures)
}

fn format_reconcile_failure(operation: &str, server_id: &str, error: &AppCommandError) -> String {
    let detail = error
        .detail
        .as_deref()
        .map(|detail| format!("; {detail}"))
        .unwrap_or_default();
    format!("{operation} '{server_id}': {}{detail}", error.message)
}

fn finish_reconcile(message: &str, failures: Vec<String>) -> Result<(), AppCommandError> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppCommandError::task_execution_failed(message).with_detail(failures.join("\n")))
    }
}

pub async fn reconcile_managed_mcp_for_agent(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    agent_enabled: bool,
) -> Result<(), AppCommandError> {
    if !is_managed_mcp_target(agent_type) || !agent_enabled {
        return Ok(());
    }
    let _guard = super::mcp_catalog::lock_operation().await;
    reconcile_managed_mcp_for_agent_unlocked(conn, agent_type, agent_enabled).await
}

pub async fn reconcile_all_managed_mcp(conn: &DatabaseConnection) -> Result<(), AppCommandError> {
    let _guard = super::mcp_catalog::lock_operation().await;
    reconcile_all_managed_mcp_unlocked(conn).await
}

pub(crate) async fn reconcile_managed_mcp_for_agent_unlocked(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    agent_enabled: bool,
) -> Result<(), AppCommandError> {
    if !is_managed_mcp_target(agent_type) || !agent_enabled {
        return Ok(());
    }
    let catalog =
        super::mcp_catalog::load_or_import_unlocked(conn, super::mcp::scan_legacy_server_specs)
            .await?;
    reconcile_agent_with_catalog(&catalog, agent_type, true)
}

pub(crate) async fn reconcile_all_managed_mcp_unlocked(
    conn: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    let catalog =
        super::mcp_catalog::load_or_import_unlocked(conn, super::mcp::scan_legacy_server_specs)
            .await?;
    let settings = agent_setting_service::list_map_by_agent_type(conn)
        .await
        .map_err(AppCommandError::db)?;
    let mut failures = Vec::new();
    for agent_type in crate::acp::registry::all_acp_agents()
        .into_iter()
        .filter(|agent_type| is_managed_mcp_target(*agent_type))
    {
        let enabled = settings
            .get(&agent_type)
            .map(|setting| setting.enabled)
            .unwrap_or_else(|| agent_setting_service::default_enabled(agent_type));
        if !enabled {
            continue;
        }
        if let Err(error) = reconcile_agent_with_catalog(&catalog, agent_type, true) {
            failures.push(format_agent_failure(agent_type, &error));
        }
    }
    finish_reconcile("Managed MCP projection failed", failures)
}

fn format_agent_failure(agent_type: AgentType, error: &AppCommandError) -> String {
    let detail = error
        .detail
        .as_deref()
        .map(|detail| format!("\n{detail}"))
        .unwrap_or_default();
    format!("{agent_type:?}: {}{detail}", error.message)
}

fn reconcile_agent_with_catalog(
    catalog: &ManagedMcpCatalog,
    agent_type: AgentType,
    agent_enabled: bool,
) -> Result<(), AppCommandError> {
    reconcile_catalog_for_agent_with(
        catalog,
        agent_enabled,
        |server_id, spec| super::mcp::upsert_server_for_agent_type(agent_type, server_id, spec),
        |server_id| super::mcp::remove_server_for_agent_type(agent_type, server_id),
    )
}
