use std::collections::BTreeSet;
use std::sync::Arc;

use crate::models::agent::AgentType;

use super::context::render_user_context;
use super::helpers::hash_parts;
use super::{
    CompanionHealthSnapshot, CompanionHealthStatus, UserMemoryCandidateDiagnostic,
    UserMemoryCandidateDiagnosticReason, UserMemoryCapabilities, UserMemoryCapabilityReason,
    UserMemoryCapabilityResult, UserMemoryContextSnapshot, UserMemoryDegradedReason,
    UserMemoryDocumentId, UserMemoryOrigin,
};

pub const APPEND_USER_MEMORY_TOOL: &str = "append_user_memory";
pub const PROPOSE_USER_MEMORY_TOOL: &str = "propose_user_memory";

#[derive(Debug, Clone)]
pub struct UserMemoryPolicyAccess {
    pub enabled: bool,
    pub agent_enabled: bool,
    pub inheritance_allowed: bool,
    pub agent_write_enabled: bool,
    pub enabled_documents: BTreeSet<UserMemoryDocumentId>,
}

#[derive(Debug, Clone)]
pub struct UserMemoryResourceAccess {
    pub storage_available: bool,
    pub readable_documents: BTreeSet<UserMemoryDocumentId>,
    pub readonly_documents: BTreeSet<UserMemoryDocumentId>,
    pub candidate_diagnostic: UserMemoryCandidateDiagnostic,
}

#[derive(Debug, Clone)]
pub struct UserMemoryCapabilityInputs {
    pub agent_type: AgentType,
    pub origin: UserMemoryOrigin,
    pub service_available: bool,
    pub policy: UserMemoryPolicyAccess,
    pub resources: UserMemoryResourceAccess,
    pub companion_health: CompanionHealthSnapshot,
    pub host_bridge_available: bool,
}

impl UserMemoryCapabilityInputs {
    pub fn unavailable(origin: UserMemoryOrigin) -> Self {
        Self {
            agent_type: AgentType::Codex,
            origin,
            service_available: false,
            policy: UserMemoryPolicyAccess {
                enabled: true,
                agent_enabled: true,
                inheritance_allowed: true,
                agent_write_enabled: true,
                enabled_documents: BTreeSet::new(),
            },
            resources: UserMemoryResourceAccess {
                storage_available: false,
                readable_documents: BTreeSet::new(),
                readonly_documents: BTreeSet::new(),
                candidate_diagnostic: UserMemoryCandidateDiagnostic {
                    available: false,
                    reason: Some(UserMemoryCandidateDiagnosticReason::RootUnavailable),
                    detail: None,
                },
            },
            companion_health: CompanionHealthSnapshot::default(),
            host_bridge_available: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserMemoryRuntimeEnvironment {
    pub companion_health: CompanionHealthSnapshot,
    pub host_bridge_available: bool,
}

pub fn compose_user_memory_capabilities(
    inputs: &UserMemoryCapabilityInputs,
) -> UserMemoryCapabilities {
    UserMemoryCapabilities {
        read_context: read_capability(inputs),
        confirmed_append: tool_capability(inputs, APPEND_USER_MEMORY_TOOL),
        candidate_proposal: tool_capability(inputs, PROPOSE_USER_MEMORY_TOOL),
    }
}

impl UserMemoryContextSnapshot {
    pub fn finalize_runtime(&mut self, runtime: UserMemoryRuntimeEnvironment) {
        self.capability_inputs.companion_health = runtime.companion_health;
        self.capability_inputs.host_bridge_available = runtime.host_bridge_available;
        self.capabilities = compose_user_memory_capabilities(&self.capability_inputs);
        self.memory_write_enabled = self.capabilities.confirmed_append.available;
        self.rendered =
            render_user_context(&self.policy, &self.documents, &self.capabilities).map(Arc::from);
        self.effective_fingerprint =
            context_fingerprint(self.rendered.as_deref(), &self.capabilities);
    }

    pub fn finalize_resumed_runtime(&mut self, runtime: UserMemoryRuntimeEnvironment) {
        self.finalize_runtime(runtime);
        self.capabilities.read_context = UserMemoryCapabilityResult::default();
        self.memory_write_enabled = self.capabilities.confirmed_append.available;
        self.rendered = None;
        let encoded = serde_json::to_vec(&self.capabilities).unwrap_or_default();
        self.effective_fingerprint = hash_parts(&[b"resumed-user-context-unknown", &encoded]);
    }
}

pub(crate) fn context_fingerprint(
    rendered: Option<&str>,
    capabilities: &UserMemoryCapabilities,
) -> String {
    let encoded = serde_json::to_vec(capabilities).unwrap_or_default();
    hash_parts(&[rendered.unwrap_or_default().as_bytes(), &encoded])
}

fn read_capability(inputs: &UserMemoryCapabilityInputs) -> UserMemoryCapabilityResult {
    if let Some(reason) = common_reason(inputs) {
        return unavailable(reason);
    }
    if !inputs.resources.storage_available {
        return unavailable(UserMemoryCapabilityReason::RootUnavailable);
    }
    if inputs.policy.enabled_documents.is_empty() {
        return unavailable(UserMemoryCapabilityReason::NoEnabledDocuments);
    }
    let readable = inputs
        .policy
        .enabled_documents
        .intersection(&inputs.resources.readable_documents)
        .next()
        .is_some();
    if !readable {
        return unavailable(UserMemoryCapabilityReason::NoReadableDocuments);
    }
    available(unreadable_reasons(inputs))
}

fn tool_capability(
    inputs: &UserMemoryCapabilityInputs,
    tool: &'static str,
) -> UserMemoryCapabilityResult {
    if let Some(reason) = common_reason(inputs) {
        return unavailable(reason);
    }
    if !inputs.resources.storage_available {
        return unavailable(UserMemoryCapabilityReason::RootUnavailable);
    }
    if !inputs.policy.agent_write_enabled {
        return unavailable(UserMemoryCapabilityReason::AgentWritesDisabled);
    }
    if let Some(reason) = resource_reason(inputs, tool) {
        return unavailable(reason);
    }
    if let Some(reason) = transport_reason(inputs) {
        return unavailable(reason);
    }
    if !inputs.host_bridge_available {
        return unavailable(UserMemoryCapabilityReason::HostBridgeUnavailable);
    }
    if let Some(reason) = health_reason(inputs.companion_health.status) {
        return unavailable(reason);
    }
    if !inputs
        .companion_health
        .advertised_tools
        .iter()
        .any(|advertised| advertised == tool)
    {
        return unavailable(UserMemoryCapabilityReason::CompanionToolMissing);
    }
    available(Vec::new())
}

fn common_reason(inputs: &UserMemoryCapabilityInputs) -> Option<UserMemoryCapabilityReason> {
    if !inputs.service_available {
        Some(UserMemoryCapabilityReason::ServiceUnavailable)
    } else if !inputs.policy.enabled {
        Some(UserMemoryCapabilityReason::PolicyDisabled)
    } else if !inputs.policy.agent_enabled {
        Some(UserMemoryCapabilityReason::AgentDisabled)
    } else if inputs.origin == UserMemoryOrigin::Delegation && !inputs.policy.inheritance_allowed {
        Some(UserMemoryCapabilityReason::DelegationDisabled)
    } else if inputs.origin == UserMemoryOrigin::Probe {
        Some(UserMemoryCapabilityReason::ProbeOrigin)
    } else {
        None
    }
}

fn resource_reason(
    inputs: &UserMemoryCapabilityInputs,
    tool: &str,
) -> Option<UserMemoryCapabilityReason> {
    if tool == APPEND_USER_MEMORY_TOOL {
        return memory_resource_reason(inputs);
    }
    candidate_resource_reason(&inputs.resources.candidate_diagnostic)
}

fn memory_resource_reason(
    inputs: &UserMemoryCapabilityInputs,
) -> Option<UserMemoryCapabilityReason> {
    if !inputs
        .policy
        .enabled_documents
        .contains(&UserMemoryDocumentId::Memory)
    {
        Some(UserMemoryCapabilityReason::MemoryDocumentDisabled)
    } else if !inputs
        .resources
        .readable_documents
        .contains(&UserMemoryDocumentId::Memory)
    {
        Some(UserMemoryCapabilityReason::MemoryDocumentUnreadable)
    } else if inputs
        .resources
        .readonly_documents
        .contains(&UserMemoryDocumentId::Memory)
    {
        Some(UserMemoryCapabilityReason::MemoryDocumentReadOnly)
    } else {
        None
    }
}

fn candidate_resource_reason(
    diagnostic: &UserMemoryCandidateDiagnostic,
) -> Option<UserMemoryCapabilityReason> {
    if diagnostic.available {
        return None;
    }
    Some(match diagnostic.reason {
        Some(UserMemoryCandidateDiagnosticReason::RootUnavailable) => {
            UserMemoryCapabilityReason::RootUnavailable
        }
        Some(UserMemoryCandidateDiagnosticReason::InvalidState) => {
            UserMemoryCapabilityReason::CandidateStateInvalid
        }
        Some(UserMemoryCandidateDiagnosticReason::ReadOnly) => {
            UserMemoryCapabilityReason::CandidateStateReadOnly
        }
        None => UserMemoryCapabilityReason::CandidateStateUnavailable,
    })
}

fn transport_reason(inputs: &UserMemoryCapabilityInputs) -> Option<UserMemoryCapabilityReason> {
    match inputs.agent_type {
        AgentType::OpenClaw => Some(UserMemoryCapabilityReason::AdapterRejectsMcp),
        AgentType::Pi => Some(UserMemoryCapabilityReason::AdapterDropsMcp),
        _ => None,
    }
}

fn health_reason(status: CompanionHealthStatus) -> Option<UserMemoryCapabilityReason> {
    match status {
        CompanionHealthStatus::Ready => None,
        CompanionHealthStatus::Missing => Some(UserMemoryCapabilityReason::CompanionMissing),
        CompanionHealthStatus::Incompatible => {
            Some(UserMemoryCapabilityReason::CompanionIncompatible)
        }
        CompanionHealthStatus::ProbeFailed => {
            Some(UserMemoryCapabilityReason::CompanionProbeFailed)
        }
        CompanionHealthStatus::Timeout => Some(UserMemoryCapabilityReason::CompanionTimeout),
    }
}

fn unreadable_reasons(inputs: &UserMemoryCapabilityInputs) -> Vec<UserMemoryDegradedReason> {
    UserMemoryDocumentId::ALL
        .into_iter()
        .filter(|id| inputs.policy.enabled_documents.contains(id))
        .filter(|id| !inputs.resources.readable_documents.contains(id))
        .map(|id| match id {
            UserMemoryDocumentId::Memory => UserMemoryDegradedReason::MemoryDocumentUnreadable,
            UserMemoryDocumentId::Profile => UserMemoryDegradedReason::ProfileDocumentUnreadable,
            UserMemoryDocumentId::Soul => UserMemoryDegradedReason::SoulDocumentUnreadable,
        })
        .collect()
}

fn available(degraded_reasons: Vec<UserMemoryDegradedReason>) -> UserMemoryCapabilityResult {
    UserMemoryCapabilityResult {
        available: true,
        reason: UserMemoryCapabilityReason::Available,
        degraded_reasons,
    }
}

fn unavailable(reason: UserMemoryCapabilityReason) -> UserMemoryCapabilityResult {
    UserMemoryCapabilityResult {
        available: false,
        reason,
        degraded_reasons: Vec::new(),
    }
}
