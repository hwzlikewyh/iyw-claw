use serde::Serialize;

use crate::acp::memory_turn::MemoryCapabilityCalls;
use crate::user_memory::{
    CompanionHealthStatus, UserMemoryContextSnapshot, UserMemoryOrigin, MEMORY_RECALL_TOOL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityLifecycleState {
    Installed,
    Enabled,
    Exposed,
    Loaded,
    Called,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityObservation {
    Yes,
    No,
    Unknown,
}

impl CapabilityObservation {
    fn known(value: bool) -> Self {
        if value {
            Self::Yes
        } else {
            Self::No
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleStateReceipt {
    state: CapabilityLifecycleState,
    observation: CapabilityObservation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CapabilityLifecycleReceipt {
    capability_id: &'static str,
    states: Vec<LifecycleStateReceipt>,
}

pub(super) fn memory_lifecycle(
    memory: &UserMemoryContextSnapshot,
    _context_loaded: bool,
) -> Vec<CapabilityLifecycleReceipt> {
    let installed = memory.capability_inputs.service_available;
    vec![
        lifecycle("memory.policy", installed, installed, installed, false),
        lifecycle(
            "memory.read_context",
            installed,
            memory_read_enabled(memory),
            false,
            false,
        ),
        lifecycle(
            "memory.read_documents",
            installed,
            memory.capabilities.read_documents.available,
            memory.capabilities.read_documents.available,
            memory.capabilities.read_documents.available,
        ),
        lifecycle(
            "memory.append",
            installed,
            memory_append_enabled(memory),
            memory.capabilities.confirmed_append.available,
            memory.capabilities.confirmed_append.available,
        ),
        lifecycle(
            "memory.propose",
            installed,
            memory_proposal_enabled(memory),
            memory.capabilities.candidate_proposal.available,
            memory.capabilities.candidate_proposal.available,
        ),
        lifecycle(
            "memory.recall",
            installed,
            memory.recall_tool_enabled && memory_read_enabled(memory),
            memory_recall_exposed(memory),
            memory_recall_exposed(memory),
        ),
    ]
}

pub(super) fn apply_called_observations(
    lifecycle: &mut [CapabilityLifecycleReceipt],
    calls: MemoryCapabilityCalls,
) {
    for item in lifecycle {
        if item.capability_id == "memory.policy" {
            item.set_observation(
                CapabilityLifecycleState::Loaded,
                CapabilityObservation::known(calls.policy),
            );
        }
        let called = match item.capability_id {
            "memory.policy" => CapabilityObservation::known(calls.policy),
            "memory.append" => CapabilityObservation::known(calls.append),
            "memory.propose" => CapabilityObservation::known(calls.propose),
            "memory.recall" => CapabilityObservation::known(calls.recall),
            "memory.read_documents" => CapabilityObservation::known(calls.read_documents),
            _ => CapabilityObservation::Unknown,
        };
        item.set_observation(CapabilityLifecycleState::Called, called);
    }
}

impl CapabilityLifecycleReceipt {
    fn set_observation(
        &mut self,
        state: CapabilityLifecycleState,
        observation: CapabilityObservation,
    ) {
        if let Some(item) = self.states.iter_mut().find(|item| item.state == state) {
            item.observation = observation;
        }
    }
}

fn lifecycle(
    capability_id: &'static str,
    installed: bool,
    enabled: bool,
    exposed: bool,
    loaded: bool,
) -> CapabilityLifecycleReceipt {
    CapabilityLifecycleReceipt {
        capability_id,
        states: vec![
            state(CapabilityLifecycleState::Installed, installed),
            state(CapabilityLifecycleState::Enabled, enabled),
            state(CapabilityLifecycleState::Exposed, exposed),
            state(CapabilityLifecycleState::Loaded, loaded),
            LifecycleStateReceipt {
                state: CapabilityLifecycleState::Called,
                observation: CapabilityObservation::Unknown,
            },
        ],
    }
}

fn state(state: CapabilityLifecycleState, observed: bool) -> LifecycleStateReceipt {
    LifecycleStateReceipt {
        state,
        observation: CapabilityObservation::known(observed),
    }
}

fn common_memory_enabled(memory: &UserMemoryContextSnapshot) -> bool {
    let inputs = &memory.capability_inputs;
    inputs.service_available
        && inputs.policy.enabled
        && inputs.policy.agent_enabled
        && (inputs.origin != UserMemoryOrigin::Delegation || inputs.policy.inheritance_allowed)
        && inputs.origin != UserMemoryOrigin::Probe
}

fn memory_read_enabled(memory: &UserMemoryContextSnapshot) -> bool {
    let inputs = &memory.capability_inputs;
    common_memory_enabled(memory)
        && inputs.resources.storage_available
        && !inputs.policy.enabled_documents.is_empty()
        && inputs
            .policy
            .enabled_documents
            .intersection(&inputs.resources.readable_documents)
            .next()
            .is_some()
}

fn memory_recall_exposed(memory: &UserMemoryContextSnapshot) -> bool {
    let inputs = &memory.capability_inputs;
    memory.recall_tool_enabled
        && memory_read_enabled(memory)
        && inputs.host_bridge_available
        && inputs.companion_health.status == CompanionHealthStatus::Ready
        && crate::user_memory::companion_exposes_capability(
            &inputs.companion_health,
            MEMORY_RECALL_TOOL,
        )
}

fn memory_append_enabled(memory: &UserMemoryContextSnapshot) -> bool {
    use crate::user_memory::UserMemoryDocumentId::Memory;

    let inputs = &memory.capability_inputs;
    common_memory_enabled(memory)
        && inputs.resources.storage_available
        && inputs.policy.agent_write_enabled
        && inputs.policy.enabled_documents.contains(&Memory)
        && inputs.resources.readable_documents.contains(&Memory)
        && !inputs.resources.readonly_documents.contains(&Memory)
}

fn memory_proposal_enabled(memory: &UserMemoryContextSnapshot) -> bool {
    let inputs = &memory.capability_inputs;
    common_memory_enabled(memory)
        && inputs.resources.storage_available
        && inputs.policy.agent_write_enabled
        && inputs.resources.candidate_diagnostic.available
}
