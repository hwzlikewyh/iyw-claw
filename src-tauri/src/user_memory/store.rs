use std::collections::BTreeMap;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::paths::UserMemoryPathError;

use super::helpers::{conflict, hash_parts, settings_revision, validate_document_update_content};
use super::service::POLICY_KEY;
use super::settings_projection::{readable_document_snapshot, unreadable_document_snapshot};
use super::{candidate_store, fs};
use super::{
    UserMemoryAvailabilityDiagnostic, UserMemoryAvailabilityReason, UserMemoryCandidateDiagnostic,
    UserMemoryCandidateDiagnosticReason, UserMemoryCandidateStatus, UserMemoryDocumentId,
    UserMemoryPolicy, UserMemoryService, UserMemorySettingsSnapshot, UserMemoryUpdateRequest,
    USER_MEMORY_AGENT_TYPES,
};

impl UserMemoryService {
    pub(super) async fn load_policy(&self) -> Result<UserMemoryPolicy, AppCommandError> {
        self.recover_pending_transaction().await?;
        self.load_policy_unrecovered().await
    }

    pub(super) async fn load_policy_unrecovered(
        &self,
    ) -> Result<UserMemoryPolicy, AppCommandError> {
        let raw = app_metadata_service::get_value(&self.db, POLICY_KEY)
            .await
            .map_err(AppCommandError::from)?;
        let mut policy = match raw {
            Some(value) => serde_json::from_str::<UserMemoryPolicy>(&value).map_err(|error| {
                AppCommandError::configuration_invalid("Stored user memory policy is invalid")
                    .with_detail(error.to_string())
            })?,
            None => UserMemoryPolicy::default(),
        };
        for agent in USER_MEMORY_AGENT_TYPES {
            policy.per_agent.entry(agent).or_insert(true);
        }
        for document in UserMemoryDocumentId::ALL {
            policy.documents.entry(document).or_insert(true);
        }
        Ok(policy)
    }

    pub(super) async fn save_policy(
        &self,
        policy: &UserMemoryPolicy,
    ) -> Result<(), AppCommandError> {
        let value = serde_json::to_string(policy)
            .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
        app_metadata_service::upsert_value(&self.db, POLICY_KEY, &value)
            .await
            .map_err(AppCommandError::from)
    }

    pub(super) fn snapshot_locked(
        &self,
        policy: &UserMemoryPolicy,
    ) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
        let resolution = self.root_resolution()?;
        let root = resolution.path.as_path();
        let mut documents = BTreeMap::new();
        for id in UserMemoryDocumentId::ALL {
            let snapshot = match self.read_document(id) {
                Ok(content) => readable_document_snapshot(root, policy, id, content),
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
            availability: available_diagnostic(),
            migration_report: self.migration_report(),
            candidate_diagnostic,
            candidate_counts,
            projected_capabilities: BTreeMap::new(),
            companion_health: Default::default(),
        })
    }

    pub(super) fn unavailable_settings_snapshot(
        &self,
        policy: &UserMemoryPolicy,
        error: &UserMemoryPathError,
    ) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
        let documents = BTreeMap::new();
        let revision = unavailable_settings_revision(policy)?;
        Ok(UserMemorySettingsSnapshot {
            enabled: policy.enabled,
            agent_write_enabled: policy.agent_write_enabled,
            inherit_to_subagents: policy.inherit_to_subagents,
            per_agent: policy.per_agent.clone(),
            documents,
            revision,
            stale_running_sessions: 0,
            resolved_root: None,
            root_source: None,
            availability: UserMemoryAvailabilityDiagnostic {
                available: false,
                reason: Some(UserMemoryAvailabilityReason::RootUnavailable),
                detail: Some(error.to_string()),
            },
            migration_report: self.migration_report(),
            candidate_diagnostic: UserMemoryCandidateDiagnostic {
                available: false,
                reason: Some(UserMemoryCandidateDiagnosticReason::RootUnavailable),
                detail: Some(error.to_string()),
            },
            candidate_counts: empty_candidate_counts(),
            projected_capabilities: BTreeMap::new(),
            companion_health: Default::default(),
        })
    }

    pub(super) fn read_document(
        &self,
        id: UserMemoryDocumentId,
    ) -> Result<String, AppCommandError> {
        if let Some(content) = self.read_document_optional(id)? {
            return Ok(content);
        }
        fs::read_document(self.resolved_root()?, id)
    }

    pub(super) fn read_document_resource(
        &self,
        id: UserMemoryDocumentId,
    ) -> Result<super::ResourceGeneration<String>, AppCommandError> {
        Ok(match self.read_document_optional(id)? {
            Some(content) => super::transaction::document_resource(content),
            None => super::ResourceGeneration::Absent,
        })
    }

    fn read_document_optional(
        &self,
        id: UserMemoryDocumentId,
    ) -> Result<Option<String>, AppCommandError> {
        let root = self.resolved_root()?;
        if self.migration_blocks_document(id)
            && std::fs::symlink_metadata(root.join(id.file_name()))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Err(AppCommandError::configuration_invalid(
                "User memory document is waiting for legacy migration retry",
            ));
        }
        fs::read_document_optional(root, id)
    }

    pub(super) fn validate_patches(
        &self,
        request: &UserMemoryUpdateRequest,
        current: &UserMemorySettingsSnapshot,
    ) -> Result<(), AppCommandError> {
        for (id, patch) in &request.documents {
            if let Some(content) = patch.content.as_deref() {
                if !current.documents[id].readable {
                    return Err(AppCommandError::permission_denied(
                        "Unreadable user memory documents cannot be overwritten",
                    ));
                }
                validate_document_update_content(content)?;
                let expected = patch.expected_etag.as_deref().ok_or_else(|| {
                    AppCommandError::invalid_input("expectedEtag is required for content updates")
                })?;
                if current.documents[id].etag != expected {
                    return Err(conflict(
                        "User memory document changed; reload before saving",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn available_diagnostic() -> UserMemoryAvailabilityDiagnostic {
    UserMemoryAvailabilityDiagnostic {
        available: true,
        reason: None,
        detail: None,
    }
}

pub(super) fn candidate_settings(
    root: &std::path::Path,
) -> (
    UserMemoryCandidateDiagnostic,
    BTreeMap<UserMemoryCandidateStatus, u32>,
) {
    let mut counts = empty_candidate_counts();
    match candidate_store::read_state(root) {
        Ok(state) => {
            for candidate in state.candidates {
                *counts.entry(candidate.status).or_insert(0) += 1;
            }
            if candidate_state_readonly(root) {
                return (
                    UserMemoryCandidateDiagnostic {
                        available: false,
                        reason: Some(UserMemoryCandidateDiagnosticReason::ReadOnly),
                        detail: Some("Candidate state is read-only".into()),
                    },
                    counts,
                );
            }
            (
                UserMemoryCandidateDiagnostic {
                    available: true,
                    reason: None,
                    detail: None,
                },
                counts,
            )
        }
        Err(error) => (
            UserMemoryCandidateDiagnostic {
                available: false,
                reason: Some(UserMemoryCandidateDiagnosticReason::InvalidState),
                detail: Some(error.detail.unwrap_or(error.message)),
            },
            counts,
        ),
    }
}

fn candidate_state_readonly(root: &std::path::Path) -> bool {
    std::fs::symlink_metadata(root.join(super::USER_MEMORY_CANDIDATE_FILE))
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

fn empty_candidate_counts() -> BTreeMap<UserMemoryCandidateStatus, u32> {
    [
        UserMemoryCandidateStatus::Tentative,
        UserMemoryCandidateStatus::Emerging,
        UserMemoryCandidateStatus::PendingConfirmation,
        UserMemoryCandidateStatus::Confirmed,
        UserMemoryCandidateStatus::Rejected,
        UserMemoryCandidateStatus::Superseded,
    ]
    .into_iter()
    .map(|status| (status, 0))
    .collect()
}

fn unavailable_settings_revision(policy: &UserMemoryPolicy) -> Result<String, AppCommandError> {
    let policy = serde_json::to_vec(policy)
        .map_err(|error| AppCommandError::configuration_invalid(error.to_string()))?;
    Ok(hash_parts(&[&policy]))
}
