use std::collections::BTreeMap;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::fs;
use super::helpers::{conflict, hash_parts, settings_revision, validate_document_update_content};
use super::service::POLICY_KEY;
use super::{
    UserMemoryDocumentId, UserMemoryDocumentSnapshot, UserMemoryPolicy, UserMemoryService,
    UserMemorySettingsSnapshot, UserMemoryUpdateRequest, USER_MEMORY_AGENT_TYPES,
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
        let root = self.resolved_root()?;
        let mut documents = BTreeMap::new();
        for id in UserMemoryDocumentId::ALL {
            let content = self.read_document(id)?;
            documents.insert(
                id,
                UserMemoryDocumentSnapshot {
                    id,
                    file_name: id.file_name().to_string(),
                    path: root.join(id.file_name()),
                    etag: hash_parts(&[content.as_bytes()]),
                    content,
                    enabled: policy.documents.get(&id).copied().unwrap_or(true),
                    readonly: fs::is_document_readonly(root, id),
                },
            );
        }
        let revision = settings_revision(policy, &documents)?;
        Ok(UserMemorySettingsSnapshot {
            enabled: policy.enabled,
            agent_write_enabled: policy.agent_write_enabled,
            inherit_to_subagents: policy.inherit_to_subagents,
            per_agent: policy.per_agent.clone(),
            documents,
            revision,
            stale_running_sessions: 0,
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
