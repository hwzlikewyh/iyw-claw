use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::helpers::hash_parts;
use super::{
    compose_user_memory_capabilities, fs, CompanionHealthSnapshot, UserMemoryCapabilityInputs,
    UserMemoryDocumentId, UserMemoryDocumentSnapshot, UserMemoryOrigin, UserMemoryPolicy,
    UserMemoryPolicyAccess, UserMemoryResourceAccess, UserMemorySettingsSnapshot,
    USER_MEMORY_AGENT_TYPES,
};

pub(crate) fn project_settings_capabilities(
    snapshot: &mut UserMemorySettingsSnapshot,
    health: CompanionHealthSnapshot,
    host_bridge_available: bool,
) {
    snapshot.companion_health = health.clone();
    snapshot.projected_capabilities = USER_MEMORY_AGENT_TYPES
        .into_iter()
        .map(|agent| {
            let inputs =
                settings_capability_inputs(snapshot, agent, health.clone(), host_bridge_available);
            (agent, compose_user_memory_capabilities(&inputs))
        })
        .collect();
}

pub(super) fn readable_document_snapshot(
    root: &std::path::Path,
    policy: &UserMemoryPolicy,
    id: UserMemoryDocumentId,
    content: String,
) -> UserMemoryDocumentSnapshot {
    UserMemoryDocumentSnapshot {
        id,
        file_name: id.file_name().to_string(),
        path: root.join(id.file_name()),
        etag: hash_parts(&[content.as_bytes()]),
        content,
        enabled: policy.documents.get(&id).copied().unwrap_or(true),
        readonly: fs::is_document_readonly(root, id),
        readable: true,
        diagnostic: None,
    }
}

pub(super) fn unreadable_document_snapshot(
    root: &std::path::Path,
    policy: &UserMemoryPolicy,
    id: UserMemoryDocumentId,
    error: AppCommandError,
) -> UserMemoryDocumentSnapshot {
    let detail = error.detail.unwrap_or(error.message);
    UserMemoryDocumentSnapshot {
        id,
        file_name: id.file_name().to_string(),
        path: root.join(id.file_name()),
        etag: hash_parts(&[b"unreadable", id.file_name().as_bytes(), detail.as_bytes()]),
        content: String::new(),
        enabled: policy.documents.get(&id).copied().unwrap_or(true),
        readonly: true,
        readable: false,
        diagnostic: Some(detail),
    }
}

fn settings_capability_inputs(
    snapshot: &UserMemorySettingsSnapshot,
    agent_type: AgentType,
    health: CompanionHealthSnapshot,
    host_bridge_available: bool,
) -> UserMemoryCapabilityInputs {
    let enabled_documents = snapshot
        .documents
        .iter()
        .filter_map(|(id, document)| document.enabled.then_some(*id))
        .collect();
    let readable_documents = snapshot
        .documents
        .iter()
        .filter_map(|(id, document)| document.readable.then_some(*id))
        .collect();
    let readonly_documents = snapshot
        .documents
        .iter()
        .filter_map(|(id, document)| document.readonly.then_some(*id))
        .collect();
    UserMemoryCapabilityInputs {
        agent_type,
        origin: UserMemoryOrigin::Root,
        service_available: true,
        policy: UserMemoryPolicyAccess {
            enabled: snapshot.enabled,
            agent_enabled: snapshot.per_agent.get(&agent_type).copied().unwrap_or(true),
            inheritance_allowed: snapshot.inherit_to_subagents,
            agent_write_enabled: snapshot.agent_write_enabled,
            enabled_documents,
        },
        resources: UserMemoryResourceAccess {
            storage_available: snapshot.availability.available,
            readable_documents,
            readonly_documents,
            candidate_diagnostic: snapshot.candidate_diagnostic.clone(),
        },
        companion_health: health,
        host_bridge_available,
    }
}
