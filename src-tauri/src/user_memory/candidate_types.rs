use serde::{Deserialize, Serialize};

use crate::models::agent::AgentType;

pub const USER_MEMORY_CANDIDATE_FILE: &str = ".user-memory-learning.json";
pub const USER_MEMORY_CANDIDATE_INVALID_REASON: &str = "user_memory_candidate_invalid";
pub const USER_MEMORY_CANDIDATE_SCHEMA_VERSION: u32 = 1;
pub const USER_MEMORY_MAX_CANDIDATE_CHARS: usize = 1_000;
pub const USER_MEMORY_MAX_CANDIDATES: usize = 500;
pub const USER_MEMORY_MAX_OBSERVATION_DETAILS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryCandidateSignal {
    Correction,
    Preference,
    Fact,
}

impl UserMemoryCandidateSignal {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Correction => "correction",
            Self::Preference => "preference",
            Self::Fact => "fact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryCandidateStatus {
    Tentative,
    Emerging,
    PendingConfirmation,
    Confirmed,
    Rejected,
    Superseded,
}

impl UserMemoryCandidateStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Confirmed | Self::Rejected | Self::Superseded)
    }

    pub(crate) fn from_observation_count(count: u32) -> Self {
        match count {
            1 => Self::Tentative,
            2 => Self::Emerging,
            _ => Self::PendingConfirmation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMemoryProposal {
    pub content: String,
    pub signal: UserMemoryCandidateSignal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateObservationSource {
    pub agent_type: AgentType,
    pub opaque_source_id: String,
    pub turn_nonce: u64,
}

impl CandidateObservationSource {
    pub(crate) fn validate(&self) -> Result<(), crate::app_error::AppCommandError> {
        if self.turn_nonce > 0 && is_valid_opaque_source_id(&self.opaque_source_id) {
            Ok(())
        } else {
            Err(crate::app_error::AppCommandError::invalid_input(
                "Candidate observation source is invalid",
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateObservation {
    pub agent_type: AgentType,
    pub opaque_source_id: String,
    pub turn_nonce: u64,
    pub observed_at: String,
}

impl CandidateObservation {
    pub(crate) fn from_source(source: CandidateObservationSource, observed_at: String) -> Self {
        Self {
            agent_type: source.agent_type,
            opaque_source_id: source.opaque_source_id,
            turn_nonce: source.turn_nonce,
            observed_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryCandidate {
    pub id: String,
    pub deduplication_digest: String,
    pub content: String,
    pub signal: UserMemoryCandidateSignal,
    pub status: UserMemoryCandidateStatus,
    pub observation_count: u32,
    pub observations: Vec<CandidateObservation>,
    pub observation_keys: Vec<String>,
    pub first_observed_at: String,
    pub last_observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_memory_entry_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by_candidate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by_memory_entry_id: Option<String>,
}

impl UserMemoryCandidate {
    pub(crate) fn mark_terminal(&mut self, status: UserMemoryCandidateStatus) {
        self.status = status;
        self.resolved_at = Some(chrono::Utc::now().to_rfc3339());
    }

    pub(crate) fn has_valid_resolution_time(&self) -> bool {
        let Some(resolved_at) = self.resolved_at.as_deref() else {
            return false;
        };
        let Ok(resolved_at) = chrono::DateTime::parse_from_rfc3339(resolved_at) else {
            return false;
        };
        chrono::DateTime::parse_from_rfc3339(&self.last_observed_at)
            .is_ok_and(|last_observed_at| resolved_at >= last_observed_at)
    }
}

pub(crate) fn is_valid_opaque_source_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn is_valid_candidate_id(value: &str) -> bool {
    value
        .strip_prefix("iyw-candidate-")
        .is_some_and(|suffix| is_lower_hex_string(suffix, 32))
}

pub(crate) fn is_valid_memory_entry_id(value: &str) -> bool {
    value
        .strip_prefix("iyw-memory-")
        .is_some_and(|suffix| is_lower_hex_string(suffix, 20))
}

pub(crate) fn is_lower_hex_string(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn new_candidate_id() -> String {
    format!("iyw-candidate-{}", uuid::Uuid::new_v4().simple())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryLearningState {
    pub schema_version: u32,
    pub candidates: Vec<UserMemoryCandidate>,
}

impl Default for UserMemoryLearningState {
    fn default() -> Self {
        Self {
            schema_version: USER_MEMORY_CANDIDATE_SCHEMA_VERSION,
            candidates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidateStateSnapshot {
    pub revision: String,
    pub candidates: Vec<UserMemoryCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryProposalResult {
    pub observation_added: bool,
    pub confirmation_recommended: bool,
    pub candidate: UserMemoryCandidate,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum UserMemoryCandidateResolution {
    Confirm { edited_content: Option<String> },
    Reject,
    SupersedeByCandidate { candidate_id: String },
    SupersedeByMemoryEntry { entry_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryCandidateResolveRequest {
    pub candidate_id: String,
    pub expected_revision: String,
    pub resolution: UserMemoryCandidateResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidateResolutionResult {
    pub candidate: UserMemoryCandidate,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryCandidateDeleteRequest {
    pub candidate_id: String,
    pub expected_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidateDeleteResult {
    pub deleted: bool,
    pub revision: String,
}
