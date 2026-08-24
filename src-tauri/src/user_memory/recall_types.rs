use serde::{Deserialize, Serialize};

const DEFAULT_RECALL_LIMIT: usize = 6;
pub(super) const MAX_RECALL_ITEM_CHARS: usize = 600;
pub(super) const MAX_RECALL_TOTAL_CHARS: usize = 4_000;
pub const USER_MEMORY_MAX_RECALL_LIMIT: usize = 8;
pub const USER_MEMORY_MAX_RECALL_QUERY_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryRecallState {
    Matched,
    NoEvidence,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryRecallRequest {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

impl UserMemoryRecallRequest {
    pub(crate) fn normalized(&self) -> Result<(String, usize), crate::app_error::AppCommandError> {
        let query = self.query.split_whitespace().collect::<Vec<_>>().join(" ");
        if query.is_empty() {
            return Err(crate::app_error::AppCommandError::invalid_input(
                "Memory recall query is empty",
            ));
        }
        if query.chars().count() > USER_MEMORY_MAX_RECALL_QUERY_CHARS {
            return Err(crate::app_error::AppCommandError::invalid_input(
                "Memory recall query is too long",
            ));
        }
        let limit = self
            .limit
            .unwrap_or(DEFAULT_RECALL_LIMIT)
            .clamp(1, USER_MEMORY_MAX_RECALL_LIMIT);
        Ok((query, limit))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryRecallItem {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub confidence: i64,
    pub importance: f64,
    pub source_revision: String,
    pub score: f64,
    pub lanes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryRecallResult {
    pub query: String,
    pub items: Vec<UserMemoryRecallItem>,
    pub index_generation: Option<i64>,
    pub source_digest: Option<String>,
    pub status: String,
    pub result_state: UserMemoryRecallState,
    pub abstained: bool,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryIndexStatus {
    pub source_key: String,
    pub source_digest: Option<String>,
    pub index_generation: Option<i64>,
    pub indexed_at: Option<String>,
    pub status: String,
    pub fts_unicode_status: String,
    pub fts_trigram_status: String,
    pub last_error: Option<String>,
}

pub(super) fn bounded_recall_content(content: &str, max_chars: usize) -> Option<String> {
    if content.chars().count() <= max_chars {
        return Some(content.to_string());
    }
    let marker = "\n[Memory result truncated]";
    let marker_chars = marker.chars().count();
    if max_chars <= marker_chars {
        return None;
    }
    let keep = max_chars - marker_chars;
    Some(format!(
        "{}{}",
        content.chars().take(keep).collect::<String>(),
        marker
    ))
}
