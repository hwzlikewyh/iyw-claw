use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;
use std::collections::{BTreeMap, BTreeSet};

use super::store::candidate_settings;
use super::{
    CompanionHealthReason, CompanionHealthSnapshot, CompanionHealthStatus,
    UserMemoryCandidateDiagnostic, UserMemoryCandidateDiagnosticReason, UserMemoryCapabilityInputs,
    UserMemoryContextSnapshot, UserMemoryDocumentId, UserMemoryOrigin, UserMemoryPolicy,
    UserMemoryPolicyAccess, UserMemoryResourceAccess, UserMemoryService, APPEND_USER_MEMORY_TOOL,
    PROPOSE_USER_MEMORY_TOOL,
};

impl UserMemoryService {
    pub async fn context_for(
        &self,
        agent_type: AgentType,
        origin: UserMemoryOrigin,
    ) -> Result<UserMemoryContextSnapshot, AppCommandError> {
        let mut snapshot = self.launch_context_for(agent_type, origin).await?;
        snapshot.finalize_runtime(compatibility_runtime_environment());
        Ok(snapshot)
    }

    pub(crate) async fn launch_context_for(
        &self,
        agent_type: AgentType,
        origin: UserMemoryOrigin,
    ) -> Result<UserMemoryContextSnapshot, AppCommandError> {
        if origin == UserMemoryOrigin::Probe {
            return Ok(UserMemoryContextSnapshot::pending(origin));
        }
        if self.root_resolution().is_err() {
            let policy = self.load_policy_unrecovered().await?;
            return Ok(build_context(
                policy,
                agent_type,
                origin,
                unavailable_resources(),
                BTreeMap::new(),
            ));
        }
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        let (resources, documents) = self.context_resources(&policy);
        Ok(build_context(
            policy, agent_type, origin, resources, documents,
        ))
    }

    fn context_resources(
        &self,
        policy: &UserMemoryPolicy,
    ) -> (
        UserMemoryResourceAccess,
        BTreeMap<UserMemoryDocumentId, String>,
    ) {
        let mut readable_documents = BTreeSet::new();
        let mut readonly_documents = BTreeSet::new();
        let mut documents = BTreeMap::new();
        for id in enabled_documents(policy) {
            match self.read_document(id) {
                Ok(content) => {
                    readable_documents.insert(id);
                    documents.insert(id, content);
                }
                Err(error) => tracing::warn!(
                    "[user-memory] omitting unreadable {} from launch context: {error}",
                    id.file_name()
                ),
            }
            if super::fs::is_document_readonly(self.resolved_root().unwrap(), id) {
                readonly_documents.insert(id);
            }
        }
        let (candidate_diagnostic, _) = candidate_settings(self.resolved_root().unwrap());
        (
            UserMemoryResourceAccess {
                storage_available: true,
                readable_documents,
                readonly_documents,
                candidate_diagnostic,
            },
            documents,
        )
    }
}

fn build_context(
    policy: UserMemoryPolicy,
    agent_type: AgentType,
    origin: UserMemoryOrigin,
    resources: UserMemoryResourceAccess,
    documents: BTreeMap<UserMemoryDocumentId, String>,
) -> UserMemoryContextSnapshot {
    let capability_inputs = UserMemoryCapabilityInputs {
        agent_type,
        origin,
        service_available: true,
        policy: policy_access(&policy, agent_type),
        resources,
        companion_health: CompanionHealthSnapshot::default(),
        host_bridge_available: false,
    };
    UserMemoryContextSnapshot {
        revision: context_revision(&policy, &documents, &capability_inputs),
        effective_fingerprint: String::new(),
        rendered: None,
        memory_write_enabled: false,
        capabilities: Default::default(),
        origin,
        capability_inputs,
        policy,
        documents,
    }
}

fn policy_access(policy: &UserMemoryPolicy, agent_type: AgentType) -> UserMemoryPolicyAccess {
    UserMemoryPolicyAccess {
        enabled: policy.enabled,
        agent_enabled: policy.per_agent.get(&agent_type).copied().unwrap_or(true),
        inheritance_allowed: policy.inherit_to_subagents,
        agent_write_enabled: policy.agent_write_enabled,
        enabled_documents: enabled_documents(policy),
    }
}

fn enabled_documents(policy: &UserMemoryPolicy) -> BTreeSet<UserMemoryDocumentId> {
    UserMemoryDocumentId::ALL
        .into_iter()
        .filter(|id| policy.documents.get(id).copied().unwrap_or(true))
        .collect()
}

fn unavailable_resources() -> UserMemoryResourceAccess {
    UserMemoryResourceAccess {
        storage_available: false,
        readable_documents: BTreeSet::new(),
        readonly_documents: BTreeSet::new(),
        candidate_diagnostic: UserMemoryCandidateDiagnostic {
            available: false,
            reason: Some(UserMemoryCandidateDiagnosticReason::RootUnavailable),
            detail: None,
        },
    }
}

fn compatibility_runtime_environment() -> super::UserMemoryRuntimeEnvironment {
    super::UserMemoryRuntimeEnvironment {
        companion_health: CompanionHealthSnapshot {
            status: CompanionHealthStatus::Ready,
            reason: CompanionHealthReason::Ready,
            expected_version: env!("CARGO_PKG_VERSION").to_string(),
            detected_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            selected_path: None,
            advertised_tools: vec![
                APPEND_USER_MEMORY_TOOL.to_string(),
                PROPOSE_USER_MEMORY_TOOL.to_string(),
            ],
            detail: None,
        },
        host_bridge_available: true,
    }
}

fn context_revision(
    policy: &UserMemoryPolicy,
    documents: &BTreeMap<UserMemoryDocumentId, String>,
    inputs: &UserMemoryCapabilityInputs,
) -> String {
    let mut generations = vec![serde_json::to_vec(policy).unwrap_or_default()];
    for id in UserMemoryDocumentId::ALL {
        let marker = if inputs.resources.readable_documents.contains(&id) {
            documents.get(&id).cloned().unwrap_or_default()
        } else {
            format!("unreadable:{id:?}")
        };
        generations.push(marker.into_bytes());
    }
    let parts = generations.iter().map(Vec::as_slice).collect::<Vec<_>>();
    super::helpers::hash_parts(&parts)
}
