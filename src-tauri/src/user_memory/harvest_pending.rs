use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;

use super::harvest::{
    validate_harvest_request, MemoryHarvestRequest, UserMemoryHarvestSubmitResult,
    USER_MEMORY_HARVEST_MAX_QUEUED, USER_MEMORY_HARVEST_MAX_STATE_CHARS,
};
use super::{harvest_store, structured_file, UserMemoryService};

const PENDING_FILE: &str = ".user-memory-harvest-pending.json";
const PENDING_SCHEMA_VERSION: u32 = 1;
const REPLAY_BATCH_SIZE: usize = 32;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingHarvest {
    schema_version: u32,
    requests: Vec<MemoryHarvestRequest>,
}

impl Default for PendingHarvest {
    fn default() -> Self {
        Self {
            schema_version: PENDING_SCHEMA_VERSION,
            requests: Vec::new(),
        }
    }
}

impl UserMemoryService {
    pub(super) async fn persist_harvest_submission(
        &self,
        request: &MemoryHarvestRequest,
    ) -> Result<UserMemoryHarvestSubmitResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?;
        let mut pending = read_pending(root)?;
        stage(&mut pending, request)?;
        save_pending(root, &pending)?;
        let staged = pending
            .requests
            .iter()
            .find(|item| item.dedup_key() == request.dedup_key())
            .expect("submission was staged");
        let result = harvest_store::submit(&self.db, staged).await?;
        pending
            .requests
            .retain(|item| item.dedup_key() != request.dedup_key());
        if let Err(error) = save_pending(root, &pending) {
            tracing::warn!(code = ?error.code, "[memory-harvest] submission cleanup deferred");
        }
        Ok(result)
    }

    pub(super) async fn replay_pending_harvest(&self) -> Result<(), AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?;
        let mut pending = read_pending(root)?;
        let batch = pending
            .requests
            .iter()
            .take(REPLAY_BATCH_SIZE)
            .cloned()
            .collect::<Vec<_>>();
        for request in &batch {
            harvest_store::submit(&self.db, request).await?;
            pending
                .requests
                .retain(|item| item.dedup_key() != request.dedup_key());
            save_pending(root, &pending)?;
        }
        if !batch.is_empty() {
            tracing::info!(
                recovered = batch.len(),
                "[memory-harvest] pending submissions recovered"
            );
        }
        Ok(())
    }

    pub(super) async fn pending_harvest_submissions(&self) -> Result<u32, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        Ok(read_pending(self.resolved_root()?)?.requests.len() as u32)
    }
}

fn read_pending(root: &std::path::Path) -> Result<PendingHarvest, AppCommandError> {
    let pending: PendingHarvest = structured_file::read_json_optional(
        root,
        PENDING_FILE,
        USER_MEMORY_HARVEST_MAX_STATE_CHARS,
    )?
    .unwrap_or_default();
    if pending.schema_version != PENDING_SCHEMA_VERSION
        || pending.requests.len() > USER_MEMORY_HARVEST_MAX_QUEUED
    {
        return Err(AppCommandError::configuration_invalid(
            "Invalid pending memory submissions",
        ));
    }
    for request in &pending.requests {
        validate_harvest_request(request)?;
    }
    Ok(pending)
}

fn stage(
    pending: &mut PendingHarvest,
    request: &MemoryHarvestRequest,
) -> Result<(), AppCommandError> {
    if pending
        .requests
        .iter()
        .any(|item| item.dedup_key() == request.dedup_key())
    {
        return Ok(());
    }
    if pending.requests.len() >= USER_MEMORY_HARVEST_MAX_QUEUED {
        return Err(AppCommandError::invalid_input(
            "Pending memory submission queue is full",
        ));
    }
    pending.requests.push(request.clone());
    Ok(())
}

fn save_pending(root: &std::path::Path, pending: &PendingHarvest) -> Result<(), AppCommandError> {
    if pending.requests.is_empty() {
        return structured_file::remove_optional(root, PENDING_FILE);
    }
    structured_file::write_json_atomic(root, PENDING_FILE, pending)
}
