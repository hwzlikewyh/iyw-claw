use std::collections::BTreeMap;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::fs;
use super::helpers::{conflict, hash_parts, settings_revision, validate_document_update_content};
use super::journal;
use super::service::POLICY_KEY;
use super::{
    UserMemoryDocumentId, UserMemoryDocumentSnapshot, UserMemoryPolicy, UserMemoryService,
    UserMemorySettingsSnapshot, UserMemoryUpdateRequest, USER_MEMORY_AGENT_TYPES,
};

impl UserMemoryService {
    pub(super) async fn load_policy(&self) -> Result<UserMemoryPolicy, AppCommandError> {
        self.recover_pending_update().await?;
        self.load_policy_unrecovered().await
    }

    async fn load_policy_unrecovered(&self) -> Result<UserMemoryPolicy, AppCommandError> {
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

    async fn recover_pending_update(&self) -> Result<(), AppCommandError> {
        let Some(pending) = journal::read(&self.root)? else {
            return Ok(());
        };
        let current = self.load_policy_unrecovered().await?;
        if current == pending.next_policy && self.documents_match(&pending.next_documents)? {
            journal::remove(&self.root)?;
            return Ok(());
        }
        if current != pending.previous_policy {
            return Err(AppCommandError::configuration_invalid(
                "User memory transaction journal does not match stored policy",
            ));
        }
        let documents = pending
            .previous_documents
            .iter()
            .map(|(id, content)| (*id, content.as_str()))
            .collect::<Vec<_>>();
        self.write_documents_atomically(&documents)?;
        journal::remove(&self.root)
    }

    fn documents_match(
        &self,
        expected: &BTreeMap<UserMemoryDocumentId, String>,
    ) -> Result<bool, AppCommandError> {
        for (id, content) in expected {
            if self.read_document(*id)? != *content {
                return Ok(false);
            }
        }
        Ok(true)
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
        let mut documents = BTreeMap::new();
        for id in UserMemoryDocumentId::ALL {
            let content = self.read_document(id)?;
            documents.insert(
                id,
                UserMemoryDocumentSnapshot {
                    id,
                    file_name: id.file_name().to_string(),
                    path: self.root.join(id.file_name()),
                    etag: hash_parts(&[content.as_bytes()]),
                    content,
                    enabled: policy.documents.get(&id).copied().unwrap_or(true),
                    readonly: fs::is_document_readonly(&self.root, id),
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
        fs::read_document(&self.root, id)
    }

    pub(super) fn write_document(
        &self,
        id: UserMemoryDocumentId,
        content: &str,
    ) -> Result<(), AppCommandError> {
        self.write_documents_atomically(&[(id, content)])
    }

    pub(super) fn write_documents_atomically(
        &self,
        documents: &[(UserMemoryDocumentId, &str)],
    ) -> Result<(), AppCommandError> {
        fs::write_documents_atomically(&self.root, documents)
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
