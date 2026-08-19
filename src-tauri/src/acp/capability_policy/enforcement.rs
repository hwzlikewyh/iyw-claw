use std::sync::{OnceLock, RwLock as StdRwLock};

use chrono::Utc;
use sea_orm::DatabaseConnection;
use tokio_util::sync::CancellationToken;

use super::revocation::CapabilityRevocationMonitor;
use super::{
    evaluate, AgentSubject, Capability, CapabilityDecision, CapabilityPolicyStore,
    CapabilityRequest, DenialCode, PolicySubject,
};
use crate::acp::runtime_host_policy;
use crate::app_error::AppCommandError;
use crate::db::service::agent_setting_service;
use crate::models::agent::AgentType;

#[derive(Clone)]
pub struct CapabilityEnforcer {
    pub(super) conn: DatabaseConnection,
    pub(super) store: CapabilityPolicyStore,
}

impl CapabilityEnforcer {
    pub fn new(conn: DatabaseConnection, store: CapabilityPolicyStore) -> Self {
        Self { conn, store }
    }

    pub async fn require_client(
        &self,
        capability: Capability,
        runtime_verified: bool,
    ) -> Result<(), AppCommandError> {
        require_enabled(
            self.client_decision(capability, runtime_verified).await?,
            capability,
        )
    }

    pub async fn require_existing_agent(
        &self,
        agent_type: AgentType,
        capability: Capability,
        runtime_verified: bool,
    ) -> Result<(), AppCommandError> {
        require_enabled(
            self.existing_agent_decision(agent_type, capability, runtime_verified)
                .await?,
            capability,
        )
    }

    pub async fn monitor_client(
        &self,
        capability: Capability,
        runtime_verified: bool,
        cancel_target: Option<CancellationToken>,
    ) -> Result<CapabilityRevocationMonitor, AppCommandError> {
        self.require_client(capability, runtime_verified).await?;
        Ok(CapabilityRevocationMonitor::spawn(
            self.clone(),
            None,
            capability,
            runtime_verified,
            cancel_target,
        ))
    }

    pub async fn monitor_existing_agent(
        &self,
        agent_type: AgentType,
        capability: Capability,
        runtime_verified: bool,
        cancel_target: Option<CancellationToken>,
    ) -> Result<CapabilityRevocationMonitor, AppCommandError> {
        self.require_existing_agent(agent_type, capability, runtime_verified)
            .await?;
        Ok(CapabilityRevocationMonitor::spawn(
            self.clone(),
            Some(agent_type),
            capability,
            runtime_verified,
            cancel_target,
        ))
    }

    async fn client_decision(
        &self,
        capability: Capability,
        runtime_verified: bool,
    ) -> Result<CapabilityDecision, AppCommandError> {
        let request = CapabilityRequest {
            subject: PolicySubject::Client,
            capability,
            compiled_support: capability.compiled_support(),
            // Capability preferences are policy metadata only; local execution
            // is always enabled and the remote policy remains authoritative.
            local_enabled: true,
            runtime_verified,
        };
        Ok(evaluate(&request, &self.store.view().await, Utc::now()))
    }

    async fn existing_agent_decision(
        &self,
        agent_type: AgentType,
        capability: Capability,
        runtime_verified: bool,
    ) -> Result<CapabilityDecision, AppCommandError> {
        let request = self
            .agent_request(agent_type, capability, runtime_verified)
            .await?;
        Ok(evaluate(&request, &self.store.view().await, Utc::now()))
    }

    pub(crate) async fn runtime_host_policy(
        &self,
        agent_type: AgentType,
    ) -> Result<runtime_host_policy::RuntimeHostPolicy, AppCommandError> {
        runtime_host_policy::resolve_with_enforcer(self, agent_type).await
    }

    pub(in crate::acp) async fn policy_view(&self) -> super::PolicySnapshotView {
        self.store.view().await
    }

    pub(in crate::acp) async fn agent_request(
        &self,
        agent_type: AgentType,
        capability: Capability,
        runtime_verified: bool,
    ) -> Result<CapabilityRequest, AppCommandError> {
        let platform_id = self.platform_id(agent_type).await?;
        let local_enabled = self.agent_local_enabled(agent_type, capability).await?;
        Ok(CapabilityRequest {
            subject: PolicySubject::Agent(AgentSubject {
                platform_id,
                is_existing_agent: agent_type.is_legacy_builtin(),
            }),
            capability,
            compiled_support: capability.compiled_support(),
            local_enabled,
            runtime_verified,
        })
    }

    async fn platform_id(&self, agent_type: AgentType) -> Result<String, AppCommandError> {
        crate::acp::version_center::platform_id(&self.conn, agent_type)
            .await
            .map_err(|error| {
                tracing::warn!(
                    agent = %agent_type,
                    error = %error,
                    "[capability-policy] Agent platform identity is unavailable"
                );
                denied(DenialCode::RemotePolicyUnknownAgent)
            })
    }

    async fn agent_local_enabled(
        &self,
        agent_type: AgentType,
        capability: Capability,
    ) -> Result<bool, AppCommandError> {
        if capability == Capability::AgentLaunch {
            let setting = agent_setting_service::get_by_agent_type(&self.conn, agent_type)
                .await
                .map_err(AppCommandError::from)?;
            return Ok(setting
                .as_ref()
                .map(|value| value.enabled)
                .unwrap_or_else(|| agent_setting_service::default_enabled(agent_type)));
        }
        // Capability preferences are policy metadata only; local execution
        // is always enabled and the remote policy remains authoritative.
        Ok(true)
    }
}

fn require_enabled(
    decision: CapabilityDecision,
    capability: Capability,
) -> Result<(), AppCommandError> {
    if decision.enabled {
        return Ok(());
    }
    let code = decision
        .denial_code
        .unwrap_or(DenialCode::RemotePolicyDenied);
    tracing::warn!(
        capability = capability.key(),
        denial_code = code.key(),
        revision = decision.revision,
        "[capability-policy] Execution denied"
    );
    Err(denied(code))
}

pub(super) fn denied(code: DenialCode) -> AppCommandError {
    AppCommandError::permission_denied("Capability is disabled").with_detail(code.key())
}

fn runtime_slot() -> &'static StdRwLock<Option<CapabilityEnforcer>> {
    static SLOT: OnceLock<StdRwLock<Option<CapabilityEnforcer>>> = OnceLock::new();
    SLOT.get_or_init(|| StdRwLock::new(None))
}

pub fn install_runtime_enforcer(enforcer: CapabilityEnforcer) {
    *runtime_slot()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(enforcer);
}

pub fn notify_runtime_policy_change() {
    if let Ok(enforcer) = runtime_enforcer() {
        enforcer.store.notify_change();
    }
}

pub fn runtime_enforcer() -> Result<CapabilityEnforcer, AppCommandError> {
    runtime_slot()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| denied(DenialCode::RemotePolicyMissing))
}

pub async fn require_runtime_agent(
    agent_type: AgentType,
    capability: Capability,
    runtime_verified: bool,
) -> Result<(), AppCommandError> {
    runtime_enforcer()?
        .require_existing_agent(agent_type, capability, runtime_verified)
        .await
}

pub async fn require_runtime_client(
    capability: Capability,
    runtime_verified: bool,
) -> Result<(), AppCommandError> {
    runtime_enforcer()?
        .require_client(capability, runtime_verified)
        .await
}
