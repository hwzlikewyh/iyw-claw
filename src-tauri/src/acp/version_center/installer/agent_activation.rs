use sea_orm::DatabaseConnection;

use crate::acp::error::AcpError;
use crate::acp::version_center::inventory;
use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::models::agent::AgentType;

use super::manifest::{push_pending_activation, read_pending_activations, PendingActivation};

pub(super) async fn activate_or_defer(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    policy: &str,
    revision: u64,
    defer_while_active: bool,
) -> Result<(), AcpError> {
    if !defer_while_active {
        return inventory::activate_agent(conn, agent_type, version, policy, revision).await;
    }
    let data_dir = crate::system_skills::data_dir_from_env();
    push_pending_activation(
        &data_dir,
        PendingActivation {
            component_id: serde_json::to_string(&agent_type)
                .map_err(|error| AcpError::protocol(error.to_string()))?,
            component_kind: "agent".to_string(),
            version: version.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            policy: Some(policy.to_string()),
            revision: Some(revision),
        },
    )
    .await
    .map_err(|error| AcpError::protocol(error.message))
}

pub(crate) async fn persisted_activation_revision(
    conn: &DatabaseConnection,
    agent_type: AgentType,
    version: &str,
    deferred: bool,
) -> Result<u64, AppCommandError> {
    if deferred {
        return pending_revision(agent_type, version).await;
    }
    let setting = agent_setting_service::get_by_agent_type(conn, agent_type)
        .await
        .map_err(AppCommandError::from)?
        .filter(|item| item.installed_version.as_deref() == Some(version))
        .ok_or_else(|| {
            AppCommandError::configuration_invalid("Installed Agent version was not activated")
        })?;
    u64::try_from(setting.catalog_revision).map_err(|error| {
        AppCommandError::configuration_invalid("Agent catalog revision is invalid")
            .with_detail(error.to_string())
    })
}

async fn pending_revision(agent_type: AgentType, version: &str) -> Result<u64, AppCommandError> {
    let component_id = serde_json::to_string(&agent_type)
        .map_err(|error| AppCommandError::invalid_input(error.to_string()))?;
    read_pending_activations(&crate::system_skills::data_dir_from_env())
        .await?
        .into_iter()
        .find(|item| {
            item.component_kind == "agent"
                && item.component_id == component_id
                && item.version == version
        })
        .and_then(|item| item.revision)
        .ok_or_else(|| {
            AppCommandError::configuration_invalid("Agent version activation was not queued")
        })
}
