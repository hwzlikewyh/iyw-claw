use super::UserMemoryDocumentId;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReconciliation {
    pub revision: String,
    pub mode: String,
    pub database_epoch: Option<i64>,
    pub required_epoch: Option<i64>,
    pub files: Vec<MemoryFileConflict>,
    pub recovery_sources: Vec<MemoryRecoverySource>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFileConflict {
    pub name: String,
    pub document: Option<UserMemoryDocumentId>,
    pub current: Option<String>,
    pub external: Option<String>,
    pub current_digest: Option<String>,
    pub external_digest: Option<String>,
    pub redacted: bool,
    pub import_allowed: bool,
    pub import_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecoverySource {
    pub id: String,
    pub label: String,
    pub epoch: i64,
    pub records: usize,
    pub revisions: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryConflictAction {
    KeepCurrent,
    ImportPaused,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveMemoryFileRequest {
    pub expected_revision: String,
    pub name: String,
    pub action: MemoryConflictAction,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreMemoryAuthorityRequest {
    pub expected_revision: String,
    pub source_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReconciliationResult {
    pub backup_path: String,
}
