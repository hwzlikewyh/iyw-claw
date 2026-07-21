use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::models::agent::AgentType;

pub const USER_MEMORY_MAX_DOCUMENT_CHARS: usize = 65_536;
pub const USER_MEMORY_MAX_APPEND_CHARS: usize = 1_000;
pub const USER_MEMORY_MAX_CONTEXT_CHARS: usize = 24_576;
pub const USER_MEMORY_MIGRATION_RECEIPT_FILE: &str = ".user-memory-migration.json";

pub const USER_MEMORY_AGENT_TYPES: [AgentType; 11] = [
    AgentType::ClaudeCode,
    AgentType::Codex,
    AgentType::OpenCode,
    AgentType::Gemini,
    AgentType::OpenClaw,
    AgentType::Cline,
    AgentType::Hermes,
    AgentType::CodeBuddy,
    AgentType::KimiCode,
    AgentType::Pi,
    AgentType::Grok,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryDocumentId {
    Memory,
    Profile,
    Soul,
}

impl UserMemoryDocumentId {
    pub const ALL: [Self; 3] = [Self::Memory, Self::Profile, Self::Soul];

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Memory => "user-memory.md",
            Self::Profile => "user-profile.md",
            Self::Soul => "user-soul.md",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserMemoryPolicy {
    pub enabled: bool,
    pub agent_write_enabled: bool,
    pub inherit_to_subagents: bool,
    pub per_agent: BTreeMap<AgentType, bool>,
    pub documents: BTreeMap<UserMemoryDocumentId, bool>,
}

impl Default for UserMemoryPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            agent_write_enabled: true,
            inherit_to_subagents: true,
            per_agent: USER_MEMORY_AGENT_TYPES
                .into_iter()
                .map(|agent| (agent, true))
                .collect(),
            documents: UserMemoryDocumentId::ALL
                .into_iter()
                .map(|document| (document, true))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryDocumentSnapshot {
    pub id: UserMemoryDocumentId,
    pub file_name: String,
    pub path: PathBuf,
    pub content: String,
    pub etag: String,
    pub enabled: bool,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemorySettingsSnapshot {
    pub enabled: bool,
    pub agent_write_enabled: bool,
    pub inherit_to_subagents: bool,
    pub per_agent: BTreeMap<AgentType, bool>,
    pub documents: BTreeMap<UserMemoryDocumentId, UserMemoryDocumentSnapshot>,
    pub revision: String,
    pub stale_running_sessions: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryDocumentPatch {
    pub content: Option<String>,
    pub enabled: Option<bool>,
    pub expected_etag: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryUpdateRequest {
    pub expected_revision: String,
    pub enabled: Option<bool>,
    pub agent_write_enabled: Option<bool>,
    pub inherit_to_subagents: Option<bool>,
    pub per_agent: Option<BTreeMap<AgentType, bool>>,
    #[serde(default)]
    pub documents: BTreeMap<UserMemoryDocumentId, UserMemoryDocumentPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryUpdateResult {
    pub settings: UserMemorySettingsSnapshot,
    pub affected_running_sessions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMemoryOrigin {
    Root,
    Delegation,
    Probe,
}

#[derive(Debug, Clone)]
pub struct UserMemoryContextSnapshot {
    pub revision: String,
    pub effective_fingerprint: String,
    pub rendered: Option<Arc<str>>,
    pub memory_write_enabled: bool,
    pub origin: UserMemoryOrigin,
}

impl UserMemoryContextSnapshot {
    pub fn disabled(origin: UserMemoryOrigin) -> Self {
        Self {
            revision: String::new(),
            effective_fingerprint: String::new(),
            rendered: None,
            memory_write_enabled: false,
            origin,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryAppend {
    pub content: String,
    pub agent_type: AgentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryAppendResult {
    pub appended: bool,
    pub entry_id: String,
    pub created_at: String,
    pub revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryLegacySourceKind {
    ConfiguredHome,
    DefaultHome,
    InstallData,
    AppData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryMigrationSource {
    pub kind: UserMemoryLegacySourceKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryMigrationStatus {
    Copied,
    SkippedExisting,
    InvalidSource,
    SourceMissing,
    SourceIoFailed,
    DestinationIoFailed,
}

impl UserMemoryMigrationStatus {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Copied | Self::SkippedExisting | Self::InvalidSource | Self::SourceMissing
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryMigrationFileResult {
    pub status: UserMemoryMigrationStatus,
    pub source: Option<PathBuf>,
    #[serde(default)]
    pub conflicting_sources: Vec<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryMigrationReceipt {
    pub schema_version: u32,
    pub considered_sources: Vec<UserMemoryMigrationSource>,
    pub files: BTreeMap<UserMemoryDocumentId, UserMemoryMigrationFileResult>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryMigrationReport {
    pub receipt: UserMemoryMigrationReceipt,
    pub warnings: Vec<String>,
}
