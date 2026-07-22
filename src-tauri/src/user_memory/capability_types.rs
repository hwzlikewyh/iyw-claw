use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryCapabilityReason {
    Available,
    NotEvaluated,
    ServiceUnavailable,
    RootUnavailable,
    PolicyDisabled,
    AgentDisabled,
    DelegationDisabled,
    ProbeOrigin,
    NoEnabledDocuments,
    NoReadableDocuments,
    AgentWritesDisabled,
    MemoryDocumentDisabled,
    MemoryDocumentUnreadable,
    MemoryDocumentReadOnly,
    CandidateStateUnavailable,
    CandidateStateInvalid,
    CandidateStateReadOnly,
    AdapterRejectsMcp,
    AdapterDropsMcp,
    HostBridgeUnavailable,
    CompanionMissing,
    CompanionIncompatible,
    CompanionProbeFailed,
    CompanionTimeout,
    CompanionToolMissing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryDegradedReason {
    MemoryDocumentUnreadable,
    ProfileDocumentUnreadable,
    SoulDocumentUnreadable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCapabilityResult {
    pub available: bool,
    pub reason: UserMemoryCapabilityReason,
    pub degraded_reasons: Vec<UserMemoryDegradedReason>,
}

impl Default for UserMemoryCapabilityResult {
    fn default() -> Self {
        Self {
            available: false,
            reason: UserMemoryCapabilityReason::NotEvaluated,
            degraded_reasons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCapabilities {
    pub read_context: UserMemoryCapabilityResult,
    pub confirmed_append: UserMemoryCapabilityResult,
    pub candidate_proposal: UserMemoryCapabilityResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionHealthStatus {
    Ready,
    Missing,
    Incompatible,
    ProbeFailed,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionHealthReason {
    Ready,
    BinaryMissing,
    NotExecutable,
    SpawnFailed,
    ExitFailed,
    ManifestMalformed,
    NameMismatch,
    VersionMismatch,
    ProtocolMismatch,
    ProbeTimeout,
    JoinFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionHealthSnapshot {
    pub status: CompanionHealthStatus,
    pub reason: CompanionHealthReason,
    pub expected_version: String,
    pub detected_version: Option<String>,
    pub selected_path: Option<PathBuf>,
    pub advertised_tools: Vec<String>,
    pub detail: Option<String>,
}

impl Default for CompanionHealthSnapshot {
    fn default() -> Self {
        Self {
            status: CompanionHealthStatus::Missing,
            reason: CompanionHealthReason::BinaryMissing,
            expected_version: env!("CARGO_PKG_VERSION").to_string(),
            detected_version: None,
            selected_path: None,
            advertised_tools: Vec::new(),
            detail: None,
        }
    }
}
