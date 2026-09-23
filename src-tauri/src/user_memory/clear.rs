use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::authority_types::AuthoritySnapshot;
use super::{
    authority_sql as sql, ResourceGeneration, UserMemoryDocumentId, UserMemoryLearningState,
    UserMemoryService,
};
use crate::app_error::AppCommandError;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClearUserMemoryScope {
    Memory,
    All,
}

impl ClearUserMemoryScope {
    pub(super) fn documents(self) -> &'static [UserMemoryDocumentId] {
        match self {
            Self::Memory => &[UserMemoryDocumentId::Memory],
            Self::All => &UserMemoryDocumentId::ALL,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClearUserMemoryRequest {
    pub scope: ClearUserMemoryScope,
    pub expected_revision: String,
    pub purge_backups: bool,
    pub confirmation: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearUserMemoryResult {
    pub cleared_records: usize,
    pub purged_backup_paths: Vec<String>,
    pub residual_backup_paths: Vec<String>,
    pub residual_file_paths: Vec<String>,
}

impl UserMemoryService {
    pub async fn clear_user_memory(
        &self,
        request: ClearUserMemoryRequest,
    ) -> Result<ClearUserMemoryResult, AppCommandError> {
        if request.confirmation != "CLEAR" {
            return Err(AppCommandError::invalid_input(
                "Clearing memory requires confirmation CLEAR",
            ));
        }
        self.ensure_forget_authority(&request.expected_revision)
            .await?;
        self.semantic
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let _semantic = self
            .semantic
            .task
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppCommandError::task_execution_failed("Memory worker unavailable"))?;
        let _refresh = self.index_refresh_lock.clone().lock_owned().await;
        let (_guard, _file) = self.acquire_locks().await?;
        self.clear_memory_locked(request).await
    }

    async fn clear_memory_locked(
        &self,
        request: ClearUserMemoryRequest,
    ) -> Result<ClearUserMemoryResult, AppCommandError> {
        let current = self.prepare_memory_clear(&request).await?;
        let sources = super::clear_sources::collect(self, &current, request.scope).await?;
        let backups = super::forget_backups::prepare_clear(
            self,
            &sources.backup_needles,
            request.scope == ClearUserMemoryScope::All,
        )
        .await?;
        let next = cleared_snapshot(current, request.scope)?;
        self.clear_forgotten_vector_projection().await?;
        self.save_authority_forget_transaction(&next, &sources.purge)
            .await?;
        *self
            .authority
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Some(next.clone());
        let residual_file_paths = self.finish_memory_clear(&next).await;
        let backups = super::forget_backups::finish(backups, request.purge_backups);
        self.schedule_index_refresh();
        tracing::info!(scope=?request.scope, cleared=sources.purge.record_ids.len(),
            purge_backups=request.purge_backups, residual_backups=backups.residual.len(),
            residual_files=residual_file_paths.len(), "[memory-clear] memory authority cleared");
        Ok(ClearUserMemoryResult {
            cleared_records: sources.purge.record_ids.len(),
            purged_backup_paths: backups.purged,
            residual_backup_paths: backups.residual,
            residual_file_paths,
        })
    }

    async fn prepare_memory_clear(
        &self,
        request: &ClearUserMemoryRequest,
    ) -> Result<AuthoritySnapshot, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        let settings = self.snapshot_locked(&policy)?;
        let learning = self.read_learning_state()?;
        if super::entry_catalog::catalog_revision(&settings.revision, &learning)?
            != request.expected_revision
        {
            return Err(super::helpers::conflict(
                "Memory changed; refresh before clearing",
            ));
        }
        let current = self.active_authority().ok_or_else(|| {
            AppCommandError::configuration_invalid(
                "Clearing memory requires active SQLite memory authority",
            )
        })?;
        if !super::authority_export::external_changes(self.resolved_root()?, &current)?.is_empty() {
            return Err(super::helpers::conflict(
                "Memory files were edited externally; reconcile before clearing",
            ));
        }
        tracing::info!(scope=?request.scope, epoch=current.epoch, "[memory-clear] validated clear request");
        Ok(current)
    }

    async fn finish_memory_clear(&self, snapshot: &AuthoritySnapshot) -> Vec<String> {
        let root = self.resolved_root().expect("memory clear validated root");
        let mut residual = clear_pending_sources(root);
        if let Err(error) = self.export_authority_locked(snapshot).await {
            tracing::warn!(code=?error.code, "[memory-clear] compatibility export deferred");
            if let Ok(files) = super::authority_export::export_contents(snapshot) {
                residual.extend(
                    files
                        .into_keys()
                        .map(|name| root.join(name).to_string_lossy().into_owned()),
                );
            }
        }
        residual
    }

    pub(super) async fn ensure_harvest_not_cleared(
        &self,
        request: &super::MemoryHarvestRequest,
        content: &str,
    ) -> Result<(), AppCommandError> {
        let cancelled = sql::rows(&self.db,
            "SELECT 1 FROM memory_harvest_outbox WHERE dedup_key=? AND noop_reason='memory_cleared'",
            vec![request.dedup_key().into()]).await?;
        if !cancelled.is_empty() || self.is_forgotten_content(content).await? {
            return Err(AppCommandError::permission_denied(
                "Memory was cleared while learning was running",
            ));
        }
        Ok(())
    }
}

fn cleared_snapshot(
    mut snapshot: AuthoritySnapshot,
    scope: ClearUserMemoryScope,
) -> Result<AuthoritySnapshot, AppCommandError> {
    let mut learning = UserMemoryLearningState::default();
    if scope == ClearUserMemoryScope::Memory {
        let retained = retained_document_ids(&snapshot);
        if let ResourceGeneration::Present { value, .. } = &snapshot.data.learning {
            learning.retention = value
                .retention
                .iter()
                .filter(|(id, _)| retained.contains(*id))
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
        }
    }
    for document in scope.documents() {
        snapshot.data.documents.insert(
            *document,
            super::transaction::document_resource(String::new()),
        );
    }
    snapshot.data.learning = super::transaction::candidate_resource(learning)?;
    super::authority_validation::validate(&snapshot.data)?;
    snapshot.digest = super::authority_types::digest(&snapshot.data)?;
    snapshot.epoch += 1;
    Ok(snapshot)
}

fn retained_document_ids(snapshot: &AuthoritySnapshot) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for document in [UserMemoryDocumentId::Profile, UserMemoryDocumentId::Soul] {
        if let Some(ResourceGeneration::Present { value, .. }) =
            snapshot.data.documents.get(&document)
        {
            ids.extend(
                value
                    .split("\n\n")
                    .map(|text| super::retention_view::document_entry_id(document, text.trim())),
            );
        }
    }
    ids
}

pub(super) fn clear_pending_sources(root: &Path) -> Vec<String> {
    let mut residual = Vec::new();
    for name in [
        super::harvest_pending::PENDING_FILE,
        super::USER_MEMORY_HARVEST_FILE,
    ] {
        if let Err(error) = super::structured_file::remove_optional(root, name) {
            tracing::warn!(code=?error.code, "[memory-clear] old learning source cleanup deferred");
            residual.push(root.join(name).to_string_lossy().into_owned());
        }
    }
    residual
}
