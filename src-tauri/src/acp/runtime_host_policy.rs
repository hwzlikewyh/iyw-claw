use crate::acp::capability_policy::{evaluate, runtime_enforcer, Capability, CapabilityEnforcer};
use crate::models::agent::AgentType;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct RuntimeHostCapabilities(u8);

impl RuntimeHostCapabilities {
    const HOST_EXECUTION: u8 = 1 << 0;
    const HOST_READ: u8 = 1 << 1;
    const HOST_WRITE: u8 = 1 << 2;
    const TERMINAL: u8 = 1 << 3;

    pub(crate) fn none() -> Self {
        Self(0)
    }

    pub(crate) fn bits(self) -> u8 {
        self.0
    }

    pub(crate) fn contains(self, capability: Capability) -> bool {
        self.0 & Self::bit(capability) != 0
    }

    pub(crate) fn runtime_verified(self) -> bool {
        self.contains(Capability::HostExecution)
    }

    fn enable(&mut self, capability: Capability) {
        self.0 |= Self::bit(capability);
    }

    fn bit(capability: Capability) -> u8 {
        match capability {
            Capability::HostExecution => Self::HOST_EXECUTION,
            Capability::HostRead => Self::HOST_READ,
            Capability::HostWrite => Self::HOST_WRITE,
            Capability::Terminal => Self::TERMINAL,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RuntimeHostPolicy {
    pub(crate) revision: Option<u64>,
    pub(crate) capabilities: RuntimeHostCapabilities,
}

impl RuntimeHostPolicy {
    pub(crate) fn deny_all() -> Self {
        Self {
            revision: None,
            capabilities: RuntimeHostCapabilities::none(),
        }
    }
}

pub(crate) async fn resolve(agent_type: AgentType) -> RuntimeHostPolicy {
    let Ok(enforcer) = runtime_enforcer() else {
        tracing::warn!(agent = %agent_type, "[capability-policy] Host policy is unavailable");
        return RuntimeHostPolicy::deny_all();
    };
    match enforcer.runtime_host_policy(agent_type).await {
        Ok(policy) => policy,
        Err(error) => {
            tracing::warn!(agent = %agent_type, error = %error, "[capability-policy] Host policy resolution failed closed");
            RuntimeHostPolicy::deny_all()
        }
    }
}

pub(super) async fn resolve_with_enforcer(
    enforcer: &CapabilityEnforcer,
    agent_type: AgentType,
) -> Result<RuntimeHostPolicy, crate::app_error::AppCommandError> {
    let view = enforcer.policy_view().await;
    let revision = view.snapshot.as_ref().map(|snapshot| snapshot.revision);
    let mut capabilities = RuntimeHostCapabilities::none();
    for request in enforcer.host_requests(agent_type).await? {
        if request.capability.requires_host_execution()
            && !capabilities.contains(Capability::HostExecution)
        {
            continue;
        }
        if evaluate(&request, &view, chrono::Utc::now()).enabled {
            capabilities.enable(request.capability);
        }
    }
    Ok(RuntimeHostPolicy {
        revision,
        capabilities,
    })
}
