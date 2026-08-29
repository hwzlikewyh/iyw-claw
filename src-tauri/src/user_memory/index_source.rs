use std::collections::BTreeMap;

use crate::app_error::AppCommandError;

use super::helpers::settings_revision;
use super::settings_projection::{readable_document_snapshot, unreadable_document_snapshot};
use super::store::candidate_settings;
use super::{
    UserMemoryDocumentId, UserMemoryPolicy, UserMemoryService, UserMemorySettingsSnapshot,
};

pub(super) fn readonly_snapshot(
    service: &UserMemoryService,
    policy: &UserMemoryPolicy,
) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
    let resolution = service.root_resolution()?;
    let root = resolution.path.as_path();
    let mut documents = BTreeMap::new();
    for id in UserMemoryDocumentId::ALL {
        let snapshot = match service.read_document_optional(id) {
            Ok(content) => {
                readable_document_snapshot(root, policy, id, content.unwrap_or_default())
            }
            Err(error) => unreadable_document_snapshot(root, policy, id, error),
        };
        documents.insert(id, snapshot);
    }
    let revision = settings_revision(policy, &documents)?;
    let (candidate_diagnostic, candidate_counts) = candidate_settings(root);
    Ok(UserMemorySettingsSnapshot {
        enabled: policy.enabled,
        agent_write_enabled: policy.agent_write_enabled,
        inherit_to_subagents: policy.inherit_to_subagents,
        per_agent: policy.per_agent.clone(),
        documents,
        revision,
        stale_running_sessions: 0,
        resolved_root: Some(root.to_path_buf()),
        root_source: Some(resolution.source),
        availability: super::UserMemoryAvailabilityDiagnostic {
            available: true,
            reason: None,
            detail: None,
        },
        migration_report: service.migration_report(),
        candidate_diagnostic,
        candidate_counts,
        projected_capabilities: BTreeMap::new(),
        companion_health: Default::default(),
        recall_index_status: None,
    })
}
