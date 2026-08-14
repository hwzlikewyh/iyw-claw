use serde::{Deserialize, Serialize};

use crate::acp::types::{AgentSkillLayout, AgentSkillScope};
use crate::models::agent::AgentType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillInventoryOwnership {
    IywManaged,
    Market,
    Plugin,
    AgentBuiltin,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillInventoryStatus {
    InstalledActive,
    InstalledInactive,
    Partial,
    AgentBuiltin,
    Duplicate,
    Conflict,
    StaleMarketRecord,
    Blocked,
    OutOfSync,
    Unreadable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillObservedLocation {
    pub root: String,
    pub path: String,
    pub agent_types: Vec<AgentType>,
    pub enabled: bool,
    pub projection_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillObservation {
    pub skill_id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: AgentSkillScope,
    pub layout: AgentSkillLayout,
    pub canonical_path: String,
    pub content_tree_hash: Option<String>,
    pub hash_error: Option<String>,
    pub ownership: SkillInventoryOwnership,
    pub read_only: bool,
    pub market_skill_id: Option<String>,
    pub installed_version: Option<String>,
    pub market_content_sha256: Option<String>,
    pub market_content_matches: Option<bool>,
    pub plugin_slug: Option<String>,
    pub plugin_component_key: Option<String>,
    pub dependencies: Vec<String>,
    pub locations: Vec<SkillObservedLocation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAgentState {
    pub agent_type: AgentType,
    pub requested_enabled: Option<bool>,
    pub effective_enabled: bool,
    pub actual_enabled: bool,
    pub required_by: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub location_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalSkillInventoryItem {
    pub skill_id: String,
    pub scope: AgentSkillScope,
    pub name: String,
    pub description: Option<String>,
    pub routing_description_chars: usize,
    pub routing_description_over_limit: bool,
    pub status: SkillInventoryStatus,
    pub duplicate: bool,
    pub conflict: bool,
    pub local_only: bool,
    pub plugin_available: bool,
    pub stale_market_record: bool,
    pub dependencies: Vec<String>,
    pub observations: Vec<SkillObservation>,
    pub agent_states: Vec<SkillAgentState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAgentDescriptionBudget {
    pub agent_type: AgentType,
    pub skill_count: usize,
    pub used_chars: usize,
    pub soft_limit_chars: usize,
    pub over_soft_limit: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInventorySnapshot {
    pub revision: String,
    pub workspace_path: Option<String>,
    pub skills: Vec<LogicalSkillInventoryItem>,
    pub description_budgets: Vec<SkillAgentDescriptionBudget>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationSetRequest {
    pub skill_id: String,
    pub scope: AgentSkillScope,
    pub workspace_path: Option<String>,
    pub agent_type: AgentType,
    pub enabled: bool,
    pub sync_mode: Option<crate::acp::types::AgentSkillSyncMode>,
    pub expected_revision: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillActivationApplyStatus {
    InSync,
    OutOfSync,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationSetResult {
    pub skill_id: String,
    pub scope: AgentSkillScope,
    pub agent_type: AgentType,
    pub requested_enabled: bool,
    pub effective_enabled: bool,
    pub actual_enabled: bool,
    pub status: SkillActivationApplyStatus,
    pub error: Option<String>,
    pub revision: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTakeOverRequest {
    pub skill_id: String,
    pub source_path: String,
    pub workspace_path: Option<String>,
    pub agent_type: AgentType,
    pub sync_mode: Option<crate::acp::types::AgentSkillSyncMode>,
    pub expected_revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReconcileRequest {
    pub workspace_path: Option<String>,
    pub agent_type: Option<AgentType>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMutationResult {
    pub status: SkillActivationApplyStatus,
    pub error: Option<String>,
    pub snapshot: SkillInventorySnapshot,
}
