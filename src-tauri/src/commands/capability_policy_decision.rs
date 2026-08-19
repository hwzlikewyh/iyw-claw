use chrono::Utc;
use sea_orm::DatabaseConnection;

use super::{validate_request_scope, CapabilityDecisionRequest, CapabilitySubjectKind};
use crate::acp::capability_policy::{
    evaluate, AgentSubject, Capability, CapabilityDecision, CapabilityPolicyStore,
    CapabilityRequest, DenialCode, PolicySubject,
};
use crate::acp::registry;
use crate::acp::version_center::{platform_id, CatalogStore};
use crate::app_error::AppCommandError;
use crate::db::service::{agent_setting_service, capability_preference_service};

pub async fn decision_core(
    conn: &DatabaseConnection,
    catalog: &CatalogStore,
    store: &CapabilityPolicyStore,
    request: CapabilityDecisionRequest,
) -> Result<CapabilityDecision, AppCommandError> {
    validate_request_scope(
        request.subject_kind,
        &request.subject_id,
        request.capability,
    )?;
    let policy_request = build_policy_request(conn, catalog, &request).await?;
    Ok(evaluate(&policy_request, &store.view().await, Utc::now()))
}

pub async fn require_existing_agent_capability_core(
    conn: &DatabaseConnection,
    store: &CapabilityPolicyStore,
    agent_type: crate::models::agent::AgentType,
    capability: Capability,
    runtime_verified: bool,
) -> Result<(), AppCommandError> {
    let platform_id = platform_id(conn, agent_type).await?;
    let local_enabled =
        existing_agent_local_enabled(conn, agent_type, &platform_id, capability).await?;
    let request = CapabilityRequest {
        subject: PolicySubject::Agent(AgentSubject {
            platform_id,
            is_existing_agent: agent_type.is_legacy_builtin(),
        }),
        capability,
        compiled_support: capability.compiled_support(),
        local_enabled,
        runtime_verified,
    };
    require_enabled(evaluate(&request, &store.view().await, Utc::now()))
}

async fn build_policy_request(
    conn: &DatabaseConnection,
    catalog: &CatalogStore,
    request: &CapabilityDecisionRequest,
) -> Result<CapabilityRequest, AppCommandError> {
    match request.subject_kind {
        CapabilitySubjectKind::Client => build_client_request(conn, request).await,
        CapabilitySubjectKind::Agent => build_agent_request(conn, catalog, request).await,
    }
}

async fn build_client_request(
    conn: &DatabaseConnection,
    request: &CapabilityDecisionRequest,
) -> Result<CapabilityRequest, AppCommandError> {
    Ok(CapabilityRequest {
        subject: PolicySubject::Client,
        capability: request.capability,
        compiled_support: request.capability.compiled_support(),
        local_enabled: local_preference(conn, request).await?,
        runtime_verified: true,
    })
}

async fn build_agent_request(
    conn: &DatabaseConnection,
    catalog: &CatalogStore,
    request: &CapabilityDecisionRequest,
) -> Result<CapabilityRequest, AppCommandError> {
    let agent_type = catalog
        .view()
        .await
        .snapshot
        .platforms
        .into_iter()
        .find(|platform| platform.id == request.subject_id)
        .and_then(|platform| registry::from_registry_id(&platform.registry_id));
    let is_known = agent_type.is_some();
    let is_existing = agent_type.is_some_and(|value| value.is_legacy_builtin());
    let setting = match agent_type {
        Some(agent_type) => agent_setting_service::get_by_agent_type(conn, agent_type)
            .await
            .map_err(AppCommandError::from)?,
        None => None,
    };
    let local_enabled = if request.capability == Capability::AgentLaunch && is_known {
        setting
            .as_ref()
            .map(|value| value.enabled)
            .unwrap_or_else(|| agent_type.is_some_and(agent_setting_service::default_enabled))
    } else {
        let enabled = local_preference(conn, request).await?;
        if request.capability.requires_host_execution() {
            enabled
                && capability_preference_service::get_enabled(
                    conn,
                    CapabilitySubjectKind::Agent.key(),
                    &request.subject_id,
                    Capability::HostExecution.key(),
                )
                .await
                .map_err(AppCommandError::from)?
        } else {
            enabled
        }
    };
    let runtime_verified = setting
        .as_ref()
        .and_then(|value| value.installed_version.as_deref())
        .is_some_and(|value| !value.trim().is_empty());
    Ok(CapabilityRequest {
        subject: PolicySubject::Agent(AgentSubject {
            platform_id: request.subject_id.clone(),
            is_existing_agent: is_existing,
        }),
        capability: request.capability,
        compiled_support: request.capability.compiled_support(),
        local_enabled,
        runtime_verified,
    })
}

async fn existing_agent_local_enabled(
    conn: &DatabaseConnection,
    agent_type: crate::models::agent::AgentType,
    platform_id: &str,
    capability: Capability,
) -> Result<bool, AppCommandError> {
    if capability == Capability::AgentLaunch {
        let setting = agent_setting_service::get_by_agent_type(conn, agent_type)
            .await
            .map_err(AppCommandError::from)?;
        return Ok(setting
            .as_ref()
            .map(|value| value.enabled)
            .unwrap_or_else(|| agent_setting_service::default_enabled(agent_type)));
    }
    let enabled = capability_preference_service::get_enabled(
        conn,
        CapabilitySubjectKind::Agent.key(),
        platform_id,
        capability.key(),
    )
    .await
    .map_err(AppCommandError::from)?;
    if capability.requires_host_execution() {
        return Ok(enabled
            && capability_preference_service::get_enabled(
                conn,
                CapabilitySubjectKind::Agent.key(),
                platform_id,
                Capability::HostExecution.key(),
            )
            .await
            .map_err(AppCommandError::from)?);
    }
    Ok(enabled)
}

async fn local_preference(
    conn: &DatabaseConnection,
    request: &CapabilityDecisionRequest,
) -> Result<bool, AppCommandError> {
    capability_preference_service::get_enabled(
        conn,
        request.subject_kind.key(),
        &request.subject_id,
        request.capability.key(),
    )
    .await
    .map_err(AppCommandError::from)
}

fn require_enabled(decision: CapabilityDecision) -> Result<(), AppCommandError> {
    if decision.enabled {
        return Ok(());
    }
    let code = decision
        .denial_code
        .unwrap_or(DenialCode::RemotePolicyDenied)
        .key();
    Err(AppCommandError::permission_denied("Agent capability is disabled").with_detail(code))
}
