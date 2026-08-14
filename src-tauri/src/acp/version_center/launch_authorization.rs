use sea_orm::DatabaseConnection;

use super::catalog::{platform_projection, PlatformAccess};
use super::client::AgentPlatformClient;
use super::types::ResolveAgentRequest;
use crate::acp::registry;
use crate::acp::version_center::capability;
use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

#[derive(Debug, Clone, Copy)]
enum LocalFallbackValidation {
    CallerVerified,
    ManagedInventory,
}

/// The spawn path must call `verify_agent_installed` before this gate.
pub async fn authorize_verified_agent_launch(
    conn: &DatabaseConnection,
    agent_type: AgentType,
) -> Result<(), AppCommandError> {
    let setting = agent_setting(conn, agent_type).await?;
    let version = setting
        .installed_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppCommandError::configuration_invalid("Agent is not installed"))?;
    authorize_agent_version(
        conn,
        agent_type,
        version,
        &setting,
        LocalFallbackValidation::CallerVerified,
    )
    .await
}

pub(crate) async fn authorize_agent_version_launch(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
) -> Result<(), AppCommandError> {
    let setting = agent_setting(conn, agent_type).await?;
    authorize_agent_version(
        conn,
        agent_type,
        version,
        &setting,
        LocalFallbackValidation::ManagedInventory,
    )
    .await
}

async fn authorize_agent_version(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    setting: &crate::db::entities::agent_setting::Model,
    fallback_validation: LocalFallbackValidation,
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
    validate_resolution(conn, agent_type, version, resolution, fallback_validation).await
}

async fn validate_resolution(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    resolution: Result<super::types::AgentOffer, AppCommandError>,
    fallback_validation: LocalFallbackValidation,
) -> Result<(), AppCommandError> {
    match resolution {
        Ok(offer) if offer.version == version => Ok(()),
        Ok(_) => Err(AppCommandError::configuration_invalid(
            "Agent launch version was rejected",
        )),
        Err(error) if super::fallback::launch_allowed(&error) => {
            if matches!(
                fallback_validation,
                LocalFallbackValidation::ManagedInventory
            ) {
                super::installer::validate_local_agent_runtime(conn, agent_type, version).await?;
            }
            tracing::warn!(
                agent_type = ?agent_type,
                version,
                reason = ?super::fallback::classify(&error),
                ?fallback_validation,
                "[agent-version-center] Fusion launch authorization unavailable; using verified local Agent"
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
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
