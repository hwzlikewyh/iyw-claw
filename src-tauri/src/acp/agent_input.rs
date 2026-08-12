use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::AgentType;

use super::types::PromptInputBlock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentInputStatus {
    Waiting,
    Dispatching,
    FallbackQueued,
    Consumed,
    Failed,
    Deleted,
}

impl AgentInputStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Dispatching => "dispatching",
            Self::FallbackQueued => "fallback_queued",
            Self::Consumed => "consumed",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "waiting" => Some(Self::Waiting),
            "dispatching" => Some(Self::Dispatching),
            "fallback_queued" => Some(Self::FallbackQueued),
            "consumed" => Some(Self::Consumed),
            "failed" => Some(Self::Failed),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentInputStrategy {
    NativeSteer,
    CooperativeFeedback,
    DeferredNext,
    SafeForceNext,
}

impl AgentInputStrategy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeSteer => "native_steer",
            Self::CooperativeFeedback => "cooperative_feedback",
            Self::DeferredNext => "deferred_next",
            Self::SafeForceNext => "safe_force_next",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "native_steer" => Some(Self::NativeSteer),
            "cooperative_feedback" => Some(Self::CooperativeFeedback),
            "deferred_next" => Some(Self::DeferredNext),
            "safe_force_next" => Some(Self::SafeForceNext),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInputPayload {
    pub blocks: Vec<PromptInputBlock>,
    pub display_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInputItem {
    pub id: String,
    pub conversation_id: i32,
    pub connection_id: Option<String>,
    pub target_turn_generation: Option<i64>,
    pub agent_type: AgentType,
    pub payload: AgentInputPayload,
    pub strategy: Option<AgentInputStrategy>,
    pub status: AgentInputStatus,
    pub dispatch_attempt: i32,
    pub last_error: Option<String>,
    pub sort_index: i64,
    pub force_batch_id: Option<String>,
    pub force_requested_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub consumed_at: Option<DateTime<Utc>>,
}

impl AgentInputItem {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            AgentInputStatus::Consumed | AgentInputStatus::Failed | AgentInputStatus::Deleted
        )
    }
}
