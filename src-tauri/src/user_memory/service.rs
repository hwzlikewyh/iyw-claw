use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;
use crate::paths::{ResolvedUserMemoryRoot, UserMemoryPathError, UserMemoryRootSource};

use super::fs;
use super::helpers::{
    apply_policy_patch, conflict, disabled_with_fingerprint, ensure_agent_write_allowed,
    hash_parts, normalize_append, policy_from_snapshot, supports_memory_tool,
    validate_document_content,
};
use super::journal::{self, PendingUpdate};
use super::{
    context::render_user_context, AgentMemoryAppend, UserMemoryAppendResult,
    UserMemoryContextSnapshot, UserMemoryDocumentId, UserMemoryOrigin, UserMemorySettingsSnapshot,
    UserMemoryUpdateRequest,
};

pub(super) const POLICY_KEY: &str = "user_memory.settings";

#[derive(Clone)]
pub struct UserMemoryService {
    pub(super) db: DatabaseConnection,
    pub(super) root: Result<ResolvedUserMemoryRoot, UserMemoryPathError>,
    pub(super) io_lock: Arc<tokio::sync::Mutex<()>>,
}

pub(crate) struct UserMemoryBackupGuard {
    _io_guard: tokio::sync::OwnedMutexGuard<()>,
    file_guard: File,
}

impl UserMemoryBackupGuard {
    pub(crate) fn file(&self) -> &File {
        &self.file_guard
    }
}

impl UserMemoryService {
    pub fn new(db: DatabaseConnection, root: PathBuf) -> Self {
        Self::from_resolution(
            db,
            Ok(ResolvedUserMemoryRoot {
                path: root,
                source: UserMemoryRootSource::Override,
            }),
        )
    }

    pub fn from_resolution(
        db: DatabaseConnection,
        root: Result<ResolvedUserMemoryRoot, UserMemoryPathError>,
    ) -> Self {
        Self {
            db,
            root,
            io_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Compatibility accessor for callers that construct an available service.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn root(&self) -> &Path {
        self.resolved_root()
            .expect("UserMemoryService::root requires an available root")
    }

    pub fn resolved_root(&self) -> Result<&Path, AppCommandError> {
        self.root
            .as_ref()
            .map(|root| root.path.as_path())
            .map_err(user_memory_root_unavailable)
    }

    pub fn root_resolution(&self) -> Result<&ResolvedUserMemoryRoot, AppCommandError> {
        self.root.as_ref().map_err(user_memory_root_unavailable)
    }

    pub(crate) async fn lock_for_backup_snapshot(
        &self,
    ) -> Result<UserMemoryBackupGuard, AppCommandError> {
        let (io_guard, file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        self.snapshot_locked(&policy)?;
        Ok(UserMemoryBackupGuard {
            _io_guard: io_guard,
            file_guard,
        })
    }

    pub async fn snapshot(&self) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        self.snapshot_locked(&policy)
    }

    pub async fn update(
        &self,
        request: UserMemoryUpdateRequest,
    ) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let mut policy = self.load_policy().await?;
        let previous_policy = policy.clone();
        let current = self.snapshot_locked(&policy)?;
        if request.expected_revision != current.revision {
            return Err(conflict(
                "User memory settings changed; reload before saving",
            ));
        }
        self.validate_patches(&request, &current)?;
        apply_policy_patch(&mut policy, &request);
        let writes = request
            .documents
            .iter()
            .filter_map(|(id, patch)| patch.content.as_deref().map(|content| (*id, content)))
            .collect::<Vec<_>>();
        let pending = (!writes.is_empty()).then(|| PendingUpdate {
            previous_policy: previous_policy.clone(),
            next_policy: policy.clone(),
            previous_documents: writes
                .iter()
                .map(|(id, _)| (*id, current.documents[id].content.clone()))
                .collect(),
            next_documents: writes
                .iter()
                .map(|(id, content)| (*id, (*content).to_string()))
                .collect(),
        });
        if let Some(pending) = pending.as_ref() {
            journal::write(self.resolved_root()?, pending)?;
        }
        self.write_documents_atomically(&writes)?;
        if let Err(error) = self.save_policy(&policy).await {
            let rollback = writes
                .iter()
                .map(|(id, _)| (*id, current.documents[id].content.as_str()))
                .collect::<Vec<_>>();
            if let Err(rollback_error) = self.write_documents_atomically(&rollback) {
                tracing::error!("[user-memory] document rollback failed: {rollback_error}");
            }
            if let Err(rollback_error) = self.save_policy(&previous_policy).await {
                tracing::error!("[user-memory] policy rollback failed: {rollback_error}");
            }
            return Err(error);
        }
        if pending.is_some() {
            if let Err(error) = journal::remove(self.resolved_root()?) {
                tracing::warn!("[user-memory] deferred transaction journal cleanup: {error}");
            }
        }
        self.snapshot_locked(&policy)
    }

    pub async fn context_for(
        &self,
        agent_type: AgentType,
        origin: UserMemoryOrigin,
    ) -> Result<UserMemoryContextSnapshot, AppCommandError> {
        if origin == UserMemoryOrigin::Probe {
            return Ok(UserMemoryContextSnapshot::disabled(origin));
        }
        let snapshot = self.snapshot().await?;
        let policy = policy_from_snapshot(&snapshot);
        let agent_enabled = policy.per_agent.get(&agent_type).copied().unwrap_or(true);
        let inherited = origin != UserMemoryOrigin::Delegation || policy.inherit_to_subagents;
        if !policy.enabled || !agent_enabled || !inherited {
            return Ok(disabled_with_fingerprint(origin, &snapshot.revision));
        }
        let write_enabled = policy.agent_write_enabled
            && policy
                .documents
                .get(&UserMemoryDocumentId::Memory)
                .copied()
                .unwrap_or(true)
            && supports_memory_tool(agent_type);
        let documents = snapshot
            .documents
            .iter()
            .map(|(id, document)| (*id, document.content.clone()))
            .collect::<BTreeMap<_, _>>();
        let rendered: Option<Arc<str>> =
            render_user_context(&policy, &documents, write_enabled).map(Arc::from);
        let effective_fingerprint = hash_parts(&[
            rendered.as_deref().unwrap_or_default().as_bytes(),
            &[write_enabled as u8],
        ]);
        Ok(UserMemoryContextSnapshot {
            revision: snapshot.revision,
            effective_fingerprint,
            rendered,
            memory_write_enabled: write_enabled,
            origin,
        })
    }

    pub async fn append_agent_memory(
        &self,
        input: AgentMemoryAppend,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        self.append_agent_memory_inner(input, true).await
    }

    /// Append for an authenticated connection whose launch token already
    /// captured write permission. Content and filesystem validation still run;
    /// only the current policy re-check is skipped so live sessions retain their
    /// documented launch snapshot until reconnect.
    pub async fn append_agent_memory_authorized(
        &self,
        input: AgentMemoryAppend,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        self.append_agent_memory_inner(input, false).await
    }

    async fn append_agent_memory_inner(
        &self,
        input: AgentMemoryAppend,
        enforce_current_policy: bool,
    ) -> Result<UserMemoryAppendResult, AppCommandError> {
        let content = normalize_append(&input.content)?;
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        if enforce_current_policy {
            ensure_agent_write_allowed(&policy, input.agent_type)?;
        }
        let mut current = self.read_document(UserMemoryDocumentId::Memory)?;
        let identity = content.to_lowercase();
        let digest = hash_parts(&[identity.as_bytes()]);
        let entry_id = format!("iyw-memory-{}", &digest[..20]);
        let now = chrono::Utc::now().to_rfc3339();
        if current.contains(&entry_id) {
            let revision = self.snapshot_locked(&policy)?.revision;
            return Ok(UserMemoryAppendResult {
                appended: false,
                entry_id,
                created_at: now,
                revision,
            });
        }
        let entry = format!(
            "- [{}] [{}] {} <!-- {} -->",
            now, input.agent_type, content, entry_id
        );
        if !current.is_empty() && !current.ends_with('\n') {
            current.push('\n');
        }
        current.push_str(&entry);
        current.push('\n');
        validate_document_content(&current)?;
        self.write_document(UserMemoryDocumentId::Memory, &current)?;
        let revision = self.snapshot_locked(&policy)?.revision;
        Ok(UserMemoryAppendResult {
            appended: true,
            entry_id,
            created_at: now,
            revision,
        })
    }

    async fn acquire_locks(
        &self,
    ) -> Result<(tokio::sync::OwnedMutexGuard<()>, File), AppCommandError> {
        let io_guard = self.io_lock.clone().lock_owned().await;
        let root = self.resolved_root()?.to_path_buf();
        let file_guard = tokio::task::spawn_blocking(move || fs::acquire_file_lock(&root))
            .await
            .map_err(|error| {
                AppCommandError::task_execution_failed("User memory lock task failed")
                    .with_detail(error.to_string())
            })??;
        Ok((io_guard, file_guard))
    }
}

fn user_memory_root_unavailable(error: &UserMemoryPathError) -> AppCommandError {
    AppCommandError::configuration_missing("user memory root is unavailable")
        .with_detail(error.to_string())
}
