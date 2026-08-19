use sea_orm::DatabaseConnection;

use super::catalog::{platform_projection, PlatformAccess};
use super::client::AgentPlatformClient;
use super::types::ResolveAgentRequest;
use crate::acp::registry;
use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

pub(crate) async fn authorize_agent_version_launch(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AppCommandError> {
    crate::acp::deepseek_config::validate_tool_version(agent_type, version)
        .map_err(AppCommandError::configuration_invalid)?;
    let setting = agent_setting(conn, agent_type).await?;
    authorize_agent_version(conn, agent_type, version, &setting).await
}

async fn authorize_agent_version(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    setting: &crate::db::entities::agent_setting::Model,
) -> Result<(), AppCommandError> {
    let platform = platform_projection(conn, agent_type).await;
    if platform.access == PlatformAccess::Disabled {
        return Err(AppCommandError::configuration_invalid(
            "Agent is disabled by the cached platform policy",
        ));
    }
    let resolution = AgentPlatformClient::resolve_agent(
        conn,
        ResolveAgentRequest {
            registry_id: registry::registry_id_for(agent_type),
            current_version: version,
            requested_version: Some(version),
            pinned_version: setting.pinned_version.as_deref(),
            client_version: env!("CARGO_PKG_VERSION"),
            runtime: capability::RUNTIME,
            target: capability::current_target(),
            arch: capability::current_arch(),
            channel: &setting.update_channel,
            reason: "manual",
        },
    )
    .await;
    validate_resolution(
        conn,
        agent_type,
        version,
        setting.pinned_version.as_deref(),
        resolution,
    )
    .await
}

async fn validate_resolution(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    pinned_version: Option<&str>,
    resolution: Result<super::types::AgentOffer, AppCommandError>,
) -> Result<(), AppCommandError> {
    match resolution {
        Ok(offer) if offer.version == version => Ok(()),
        Ok(offer) if is_trusted_deepseek_fallback(agent_type, version) => {
            if offer.required || !pin_allows_fallback(pinned_version, version) {
                return Err(AppCommandError::configuration_invalid(
                    "Agent launch version was rejected",
                ));
            }
            validate_official_deepseek_fallback(conn, agent_type, version).await?;
            tracing::warn!(
                agent_type = ?agent_type,
                version,
                offered_version = %offer.version,
                "[agent-version-center] activating verified DeepSeek fallback instead of newer Fusion offer"
            );
            Ok(())
        }
        Ok(_) => Err(AppCommandError::configuration_invalid(
            "Agent launch version was rejected",
        )),
        Err(error) if super::fallback::launch_allowed(&error) => {
            super::installer::validate_local_agent_runtime(conn, agent_type, version).await?;
            tracing::warn!(
                agent_type = ?agent_type,
                version,
                reason = ?super::fallback::classify(&error),
                "[agent-version-center] Fusion launch authorization unavailable; using verified local Agent"
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn validate_official_deepseek_fallback(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AppCommandError> {
    let verified = super::inventory::list_agent_installations(conn, agent_type)
        .await
        .map_err(|error| AppCommandError::task_execution_failed(error.to_string()))?
        .into_iter()
        .any(|installation| {
            installation.version == version
                && installation.verified
                && installation.platform == crate::acp::registry::current_platform()
                && installation.source_key.as_deref() == Some("official-npm-registry")
        });
    if !verified {
        return Err(AppCommandError::configuration_invalid(
            "DeepSeek fallback installation is not verified",
        ));
    }
    super::installer::validate_local_agent_runtime(conn, agent_type, version).await
}

fn is_trusted_deepseek_fallback(agent_type: AgentType, version: &str) -> bool {
    crate::acp::deepseek_config::fallback_tool_version(agent_type)
        .is_some_and(|fallback| version.trim() == fallback)
}

fn pin_allows_fallback(pinned_version: Option<&str>, version: &str) -> bool {
    pinned_version
        .map(str::trim)
        .filter(|pinned| !pinned.is_empty())
        .is_none_or(|pinned| pinned == version)
}

async fn agent_setting(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<crate::db::entities::agent_setting::Model, AppCommandError> {
    crate::db::service::agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(AppCommandError::from)?
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent setting is unavailable"))
}
