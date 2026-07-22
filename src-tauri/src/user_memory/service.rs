use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use sea_orm::DatabaseConnection;

use crate::app_error::AppCommandError;
use crate::paths::{ResolvedUserMemoryRoot, UserMemoryPathError, UserMemoryRootSource};

use super::fs;
use super::helpers::{apply_policy_patch, conflict};
use super::transaction::document_resource;
use super::{
    project_settings_capabilities, UserMemoryDocumentId, UserMemoryGeneration,
    UserMemoryMigrationReport, UserMemorySettingsSnapshot, UserMemoryUpdateRequest,
};

pub(super) const POLICY_KEY: &str = "user_memory.settings";

#[derive(Clone)]
pub struct UserMemoryService {
    pub(super) db: DatabaseConnection,
    pub(super) root: Result<ResolvedUserMemoryRoot, UserMemoryPathError>,
    pub(super) io_lock: Arc<tokio::sync::Mutex<()>>,
    pub(super) migration_blocked_documents: Arc<RwLock<BTreeSet<UserMemoryDocumentId>>>,
    pub(super) migration_report: Arc<RwLock<Option<UserMemoryMigrationReport>>>,
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
            migration_blocked_documents: Arc::new(RwLock::new(BTreeSet::new())),
            migration_report: Arc::new(RwLock::new(None)),
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

    pub(super) fn set_migration_blocked_documents(
        &self,
        documents: BTreeSet<UserMemoryDocumentId>,
    ) {
        let mut blocked = self
            .migration_blocked_documents
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *blocked = documents;
    }

    pub(super) fn migration_blocks_document(&self, id: UserMemoryDocumentId) -> bool {
        self.migration_blocked_documents
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&id)
    }

    pub(super) fn set_migration_report(&self, report: Option<UserMemoryMigrationReport>) {
        let mut current = self
            .migration_report
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = report;
    }

    pub(super) fn migration_report(&self) -> Option<UserMemoryMigrationReport> {
        self.migration_report
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
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

    pub(crate) async fn lock_for_restore_staging(
        &self,
    ) -> Result<UserMemoryBackupGuard, AppCommandError> {
        let (io_guard, file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
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

    pub async fn settings_snapshot(&self) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
        let snapshot = match self.root.as_ref() {
            Ok(_) => self.snapshot().await,
            Err(error) => {
                let policy = self.load_policy_unrecovered().await?;
                self.unavailable_settings_snapshot(&policy, error)
            }
        }?;
        self.enrich_settings_snapshot(snapshot).await
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
            .filter_map(|(id, patch)| patch.content.clone().map(|content| (*id, content)))
            .collect::<BTreeMap<_, _>>();
        let previous_documents = writes
            .keys()
            .map(|id| {
                (
                    *id,
                    document_resource(current.documents[id].content.clone()),
                )
            })
            .collect();
        let next_documents = writes
            .into_iter()
            .map(|(id, content)| (id, document_resource(content)))
            .collect();
        self.execute_transaction(
            UserMemoryGeneration {
                policy: Some(previous_policy),
                documents: previous_documents,
                candidate_state: None,
            },
            UserMemoryGeneration {
                policy: Some(policy.clone()),
                documents: next_documents,
                candidate_state: None,
            },
        )
        .await?;
        let snapshot = self.snapshot_locked(&policy)?;
        drop(_file_guard);
        drop(_guard);
        self.enrich_settings_snapshot(snapshot).await
    }

    async fn enrich_settings_snapshot(
        &self,
        mut snapshot: UserMemorySettingsSnapshot,
    ) -> Result<UserMemorySettingsSnapshot, AppCommandError> {
        let health = crate::acp::companion_health::locate_healthy_companion().await;
        project_settings_capabilities(&mut snapshot, health, false);
        Ok(snapshot)
    }

    pub(super) async fn acquire_locks(
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
