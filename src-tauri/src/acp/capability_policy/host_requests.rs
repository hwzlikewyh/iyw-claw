use super::{AgentSubject, Capability, CapabilityEnforcer, CapabilityRequest, PolicySubject};
use crate::app_error::AppCommandError;
use crate::db::service::capability_preference_service;
use crate::models::agent::AgentType;

const HOST_CAPABILITIES: [Capability; 4] = [
    Capability::HostExecution,
    Capability::HostRead,
    Capability::HostWrite,
    Capability::Terminal,
];

impl CapabilityEnforcer {
    pub(in crate::acp) async fn host_requests(
        &self,
        agent_type: AgentType,
    ) -> Result<Vec<CapabilityRequest>, AppCommandError> {
        let platform_id = crate::acp::version_center::platform_id(&self.conn, agent_type)
            .await
            .ok();
        let preferences = match platform_id.as_deref() {
            Some(id) => capability_preference_service::list_for_subject(&self.conn, "agent", id)
                .await
                .map_err(AppCommandError::from)?,
            None => Vec::new(),
        };
        let enabled = |capability: Capability| {
            preferences
                .iter()
                .find(|row| row.capability == capability.key())
                .is_none_or(|row| row.enabled)
        };
        let subject = PolicySubject::Agent(AgentSubject {
            platform_id: platform_id
                .unwrap_or_else(|| crate::acp::registry::registry_id_for(agent_type).into()),
            is_existing_agent: agent_type.is_legacy_builtin(),
        });
        // 一次读取本地权限快照，子能力仍要求 HostExecution 同时开启。
        Ok(HOST_CAPABILITIES
            .into_iter()
            .map(|capability| CapabilityRequest {
                subject: subject.clone(),
                capability,
                compiled_support: capability.compiled_support(),
                local_enabled: enabled(capability)
                    && (!capability.requires_host_execution()
                        || enabled(Capability::HostExecution)),
                runtime_verified: true,
            })
            .collect())
    }
}
