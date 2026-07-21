use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::models::agent::AgentType;

use super::{
    UserMemoryCandidate, UserMemoryCandidateResolutionResult, UserMemoryCandidateSignal,
    UserMemoryCandidateStatus,
};

pub const USER_MEMORY_CANDIDATE_DEFAULT_LIMIT: u32 = 50;
pub const USER_MEMORY_CANDIDATE_MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryCandidateStatusFilter {
    Tentative,
    Emerging,
    PendingConfirmation,
    Terminal,
}

impl UserMemoryCandidateStatusFilter {
    pub(crate) fn matches(self, status: UserMemoryCandidateStatus) -> bool {
        match self {
            Self::Tentative => matches!(status, UserMemoryCandidateStatus::Tentative),
            Self::Emerging => matches!(status, UserMemoryCandidateStatus::Emerging),
            Self::PendingConfirmation => {
                matches!(status, UserMemoryCandidateStatus::PendingConfirmation)
            }
            Self::Terminal => status.is_terminal(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserMemoryCandidateListRequest {
    #[serde(default)]
    pub status: Option<UserMemoryCandidateStatusFilter>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_candidate_limit")]
    pub limit: u32,
}

impl Default for UserMemoryCandidateListRequest {
    fn default() -> Self {
        Self {
            status: None,
            offset: 0,
            limit: USER_MEMORY_CANDIDATE_DEFAULT_LIMIT,
        }
    }
}

const fn default_candidate_limit() -> u32 {
    USER_MEMORY_CANDIDATE_DEFAULT_LIMIT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidateSummary {
    pub id: String,
    pub content: String,
    pub signal: UserMemoryCandidateSignal,
    pub status: UserMemoryCandidateStatus,
    pub observation_count: u32,
    pub source_agents: Vec<AgentType>,
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

impl From<&UserMemoryCandidate> for UserMemoryCandidateSummary {
    fn from(candidate: &UserMemoryCandidate) -> Self {
        let source_agents = candidate
            .observations
            .iter()
            .map(|observation| observation.agent_type)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            id: candidate.id.clone(),
            content: candidate.content.clone(),
            signal: candidate.signal,
            status: candidate.status,
            observation_count: candidate.observation_count,
            source_agents,
            first_observed_at: candidate.first_observed_at.clone(),
            last_observed_at: candidate.last_observed_at.clone(),
            resolved_at: candidate.resolved_at.clone(),
            resolved_content: candidate.resolved_content.clone(),
            confirmed_memory_entry_id: candidate.confirmed_memory_entry_id.clone(),
            superseded_by_candidate_id: candidate.superseded_by_candidate_id.clone(),
            superseded_by_memory_entry_id: candidate.superseded_by_memory_entry_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidatePage {
    pub candidates: Vec<UserMemoryCandidateSummary>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidateResolutionResponse {
    pub candidate: UserMemoryCandidateSummary,
    pub revision: String,
}

impl From<UserMemoryCandidateResolutionResult> for UserMemoryCandidateResolutionResponse {
    fn from(result: UserMemoryCandidateResolutionResult) -> Self {
        Self {
            candidate: UserMemoryCandidateSummary::from(&result.candidate),
            revision: result.revision,
        }
    }
}
