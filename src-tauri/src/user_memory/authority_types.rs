use super::{ResourceGeneration, UserMemoryDocumentId, UserMemoryLearningState, UserMemoryPolicy};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AuthorityData {
    pub documents: BTreeMap<UserMemoryDocumentId, ResourceGeneration<String>>,
    pub learning: ResourceGeneration<UserMemoryLearningState>,
    pub policy: UserMemoryPolicy,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct AuthoritySnapshot {
    pub store_id: String,
    pub mode: String,
    pub epoch: i64,
    pub digest: String,
    pub backup_path: String,
    pub data: AuthorityData,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AuthorityMarker {
    pub store_id: String,
    pub epoch: i64,
    pub digest: String,
    pub exports: BTreeMap<String, Option<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAuthorityStatus {
    pub mode: String,
    pub epoch: Option<i64>,
    pub revision: Option<String>,
    pub records: usize,
    pub revisions: usize,
    pub pending_projections: usize,
    pub backup_path: Option<String>,
    pub external_changes: Vec<String>,
    pub counts: BTreeMap<String, usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivateMemoryAuthorityRequest {
    pub expected_revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRevisionEntry {
    pub record_id: String,
    pub source_id: String,
    pub revision: i64,
    pub state: String,
    pub item: serde_json::Value,
    pub reason: String,
    pub recorded_at: String,
    pub sources: Vec<MemoryHistorySource>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHistorySource {
    pub source_id: String,
    pub turn_nonce: i64,
    pub conversation: Option<MemorySourceConversation>,
    pub availability: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySourceConversation {
    pub id: i32,
    pub folder_id: i32,
    pub agent_type: crate::models::agent::AgentType,
    pub title: Option<String>,
}

pub(super) fn digest(data: &AuthorityData) -> Result<String, crate::app_error::AppCommandError> {
    Ok(super::helpers::hash_parts(&[&serde_json::to_vec(data)
        .map_err(|error| {
            crate::app_error::AppCommandError::configuration_invalid(
                "Cannot encode memory authority",
            )
            .with_detail(error.to_string())
        })?]))
}
