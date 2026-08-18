//! TurnComplete harvest queue: bounded, persisted, async candidate extraction.
//!
//! The ACP TurnComplete hook (wired by Task 13) submits a small
//! [`MemoryHarvestRequest`] via [`UserMemoryService::submit_turn_harvest`].
//! This module persists a checkpoint in the user memory root, deduplicates by
//! (conversation, turn nonce), and extracts conservative candidate
//! observations asynchronously without blocking the UI completion event.
//!
//! States: `queued -> extracting -> proposed | noop | failed -> dead`.
//! Recovery: on restart, `queued`/`extracting` records are re-queued, and
//! `failed` records with attempts below the cap are retried with bounded
//! backoff; `dead` records stay terminal.

use std::path::Path;
use std::sync::{Arc, OnceLock, Weak};

use serde::{Deserialize, Serialize};

use crate::app_error::AppCommandError;
use crate::models::agent::AgentType;

use super::candidate_store;
use super::helpers::{contains_potential_secret, hash_parts, normalize_candidate};
use super::structured_file;
use super::{
    AgentMemoryProposal, CandidateObservationSource, UserMemoryCandidateSignal,
    UserMemoryProposalResult, UserMemoryService, USER_MEMORY_MAX_CANDIDATE_CHARS,
};

pub const USER_MEMORY_HARVEST_FILE: &str = ".user-memory-harvest.json";
pub const USER_MEMORY_HARVEST_SCHEMA_VERSION: u32 = 1;
pub const USER_MEMORY_HARVEST_MAX_QUEUED: usize = 256;
pub const USER_MEMORY_HARVEST_MAX_RETRIES: u32 = 3;
pub const USER_MEMORY_HARVEST_MIN_CONTENT_CHARS: usize = 24;
pub const USER_MEMORY_HARVEST_MAX_STATE_CHARS: usize = 16_777_216;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryHarvestState {
    Queued,
    Extracting,
    Proposed,
    Noop,
    Failed,
    Dead,
}

impl UserMemoryHarvestState {
    pub(crate) fn is_recoverable(self) -> bool {
        matches!(self, Self::Queued | Self::Extracting)
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Proposed | Self::Noop | Self::Dead)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMemoryHarvestFailureKind {
    Io,
    InvalidInput,
    SensitiveContent,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryHarvestRequest {
    /// Conversation identifier as a string (Snowflake-safe). Uniqueness with
    /// `turn_nonce` is the queue's deduplication key.
    pub conversation: String,
    /// Turn nonce from the connection's `MemoryTurnTracker` (accepted prompt
    /// start; unique within a connection).
    pub turn_nonce: u64,
    /// Agent that ran the turn.
    pub agent_type: AgentType,
    /// Raw stop reason from `TurnComplete`; abnormal reasons are `noop`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Sanitized, truncated semantic reference to the user's input for this
    /// turn. Must be supplied by the hook already stripped of secrets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_input_ref: Option<String>,
    /// Sanitized, truncated semantic reference to the assistant's final
    /// output for this turn (used only as evidence, never persisted verbatim
    /// beyond the checkpoint's bounded copy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_input_ref: Option<String>,
    /// Host wall-clock submission time (RFC 3339).
    pub submitted_at: String,
}

impl MemoryHarvestRequest {
    pub(crate) fn dedup_key(&self) -> String {
        hash_parts(&[self.conversation.as_bytes(), &self.turn_nonce.to_le_bytes()])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HarvestRecord {
    pub(crate) request: MemoryHarvestRequest,
    pub(crate) state: UserMemoryHarvestState,
    pub(crate) attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) failure_kind: Option<UserMemoryHarvestFailureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) failure_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) noop_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) candidate_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) processed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) processing_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HarvestCheckpoint {
    schema_version: u32,
    records: Vec<HarvestRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_harvest_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_success_write_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_failure_at: Option<String>,
}

impl Default for HarvestCheckpoint {
    fn default() -> Self {
        Self {
            schema_version: USER_MEMORY_HARVEST_SCHEMA_VERSION,
            records: Vec::new(),
            last_harvest_at: None,
            last_success_write_at: None,
            last_failure_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryHarvestSubmitResult {
    pub enqueued: bool,
    pub duplicate: bool,
    pub queued_total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryHarvestStatus {
    pub queued: u32,
    pub extracting: u32,
    pub proposed: u32,
    pub noop: u32,
    pub failed: u32,
    pub dead: u32,
    pub backlog: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_harvest_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_write_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryHarvestRescanPreview {
    pub re_queued: u32,
    pub retained_terminal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryHarvestRescanResult {
    pub preview: UserMemoryHarvestRescanPreview,
    pub executed: bool,
}

/// In-memory queue handle. Clone shares the same wake/worker. The worker task
/// is spawned lazily on the first submit and survives for the app lifetime;
/// pending checkpoint records are re-drained after a restart. `Notify` stores
/// a permit when the worker is not yet waiting, so submits can never be lost.
#[derive(Clone, Default)]
pub(crate) struct HarvestQueue {
    inner: Arc<HarvestQueueInner>,
}

#[derive(Default)]
struct HarvestQueueInner {
    wake: tokio::sync::Notify,
    worker_spawned: OnceLock<()>,
}

impl UserMemoryService {
    /// Hook API for the ACP TurnComplete wiring (Task 13).
    ///
    /// Persists the request into the checkpoint (durable before the worker
    /// runs), deduplicates by (conversation, turn nonce), and wakes the
    /// background worker. Never blocks the UI completion event beyond the
    /// bounded file write.
    pub async fn submit_turn_harvest(
        self: &Arc<Self>,
        request: MemoryHarvestRequest,
    ) -> Result<UserMemoryHarvestSubmitResult, AppCommandError> {
        validate_harvest_request(&request)?;
        let result = {
            let root = self.harvest_root()?;
            let mut checkpoint = self.harvest_checkpoint().await?;
            let key = request.dedup_key();
            if checkpoint
                .records
                .iter()
                .any(|record| record.request.dedup_key() == key && !record.state.is_terminal())
            {
                return Ok(UserMemoryHarvestSubmitResult {
                    enqueued: false,
                    duplicate: true,
                    queued_total: checkpoint.records.len() as u32,
                });
            }
            if checkpoint.records.len() >= USER_MEMORY_HARVEST_MAX_QUEUED {
                return Err(AppCommandError::invalid_input(
                    "User memory harvest queue is full",
                ));
            }
            checkpoint.records.push(HarvestRecord {
                request,
                state: UserMemoryHarvestState::Queued,
                attempts: 0,
                failure_kind: None,
                failure_detail: None,
                noop_reason: None,
                candidate_ids: None,
                processed_at: None,
                processing_ms: None,
            });
            checkpoint.last_harvest_at = Some(chrono::Utc::now().to_rfc3339());
            let queued_total = checkpoint.records.len() as u32;
            write_checkpoint(&root, &checkpoint)?;
            Ok(UserMemoryHarvestSubmitResult {
                enqueued: true,
                duplicate: false,
                queued_total,
            })
        };
        self.ensure_harvest_worker();
        result
    }

    pub async fn harvest_status(&self) -> Result<UserMemoryHarvestStatus, AppCommandError> {
        let checkpoint = self.harvest_checkpoint().await?;
        Ok(project_harvest_status(&checkpoint))
    }

    /// Re-queue unprocessed or recoverable records. Returns a preview first;
    /// callers confirm before `execute = true` takes effect.
    pub async fn rescan_harvest(
        self: &Arc<Self>,
        execute: bool,
    ) -> Result<UserMemoryHarvestRescanResult, AppCommandError> {
        let root = self.harvest_root()?;
        let mut checkpoint = self.harvest_checkpoint().await?;
        let mut re_queued = 0u32;
        let mut retained_terminal = 0u32;
        for record in &mut checkpoint.records {
            if record.state.is_terminal() {
                retained_terminal += 1;
                continue;
            }
            if record.state.is_recoverable()
                || (record.state == UserMemoryHarvestState::Failed
                    && record.attempts < USER_MEMORY_HARVEST_MAX_RETRIES)
            {
                re_queued += 1;
                if execute {
                    record.state = UserMemoryHarvestState::Queued;
                }
            }
        }
        let preview = UserMemoryHarvestRescanPreview {
            re_queued,
            retained_terminal,
        };
        if execute {
            write_checkpoint(&root, &checkpoint)?;
        }
        self.ensure_harvest_worker();
        Ok(UserMemoryHarvestRescanResult {
            preview,
            executed: execute,
        })
    }

    /// Recompute the candidate index (digests / observation keys) from stored
    /// candidate content. Idempotent normalization used by the settings UI's
    /// "rebuild candidate index" action.
    pub async fn rebuild_candidate_index(
        &self,
        execute: bool,
    ) -> Result<UserMemoryCandidateIndexRebuildResult, AppCommandError> {
        let (_io_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let root = self.resolved_root()?.to_path_buf();
        let mut state = candidate_store::read_state(&root)?;
        let mut affected = 0u32;
        for candidate in &mut state.candidates {
            let next_digest =
                candidate_store::deduplication_digest(&candidate.content, candidate.signal);
            if next_digest != candidate.deduplication_digest {
                candidate.deduplication_digest = next_digest;
                candidate.observation_keys = candidate
                    .observations
                    .iter()
                    .map(|observation| {
                        candidate_store::observation_key(
                            &candidate.deduplication_digest,
                            &observation.opaque_source_id,
                            observation.turn_nonce,
                        )
                    })
                    .collect();
                affected += 1;
            }
        }
        if execute {
            candidate_store::write_state(&root, &state)?;
            self.schedule_index_refresh();
        }
        Ok(UserMemoryCandidateIndexRebuildResult {
            affected,
            executed: execute,
            revision: candidate_store::revision(&state)?,
        })
    }

    /// Background loop: drain recoverable checkpoint records and process new
    /// submissions until the runtime tears down. The wake notify carries a
    /// permit when no worker is waiting, so a submit before spawn is not lost.
    fn ensure_harvest_worker(self: &Arc<Self>) {
        let _ = self.harvest.inner.worker_spawned.get_or_init(|| {
            let weak: Weak<Self> = Arc::downgrade(self);
            let inner = Arc::clone(&self.harvest.inner);
            tokio::spawn(async move {
                loop {
                    inner.wake.notified().await;
                    let Some(service) = weak.upgrade() else {
                        break;
                    };
                    let _ = service.process_recoverable_harvest().await;
                }
            });
            ()
        });
        self.harvest.inner.wake.notify_one();
    }

    async fn process_recoverable_harvest(self: &Arc<Self>) -> Result<(), AppCommandError> {
        let checkpoint = self.harvest_checkpoint().await?;
        let recoverable = checkpoint
            .records
            .iter()
            .filter(|record| {
                record.state.is_recoverable()
                    || (record.state == UserMemoryHarvestState::Failed
                        && record.attempts < USER_MEMORY_HARVEST_MAX_RETRIES)
            })
            .map(|record| record.request.clone())
            .collect::<Vec<_>>();
        for request in recoverable {
            let _ = self.process_harvest_request(request).await;
        }
        Ok(())
    }

    async fn process_harvest_request(
        self: &Arc<Self>,
        request: MemoryHarvestRequest,
    ) -> Result<(), AppCommandError> {
        let started = std::time::Instant::now();
        let root = self.harvest_root()?;
        {
            let mut checkpoint = self.harvest_checkpoint().await?;
            let Some(index) = checkpoint
                .records
                .iter()
                .position(|record| record.request.dedup_key() == request.dedup_key())
            else {
                return Ok(());
            };
            if checkpoint.records[index].state.is_terminal() {
                return Ok(());
            }
            checkpoint.records[index].state = UserMemoryHarvestState::Extracting;
            checkpoint.records[index].attempts =
                checkpoint.records[index].attempts.saturating_add(1);
            write_checkpoint(&root, &checkpoint)?;
        }

        let outcome = self.extract_and_propose(&request).await;

        let mut checkpoint = self.harvest_checkpoint().await?;
        let Some(index) = checkpoint
            .records
            .iter()
            .position(|record| record.request.dedup_key() == request.dedup_key())
        else {
            return Ok(());
        };
        let now = chrono::Utc::now().to_rfc3339();
        let elapsed = started.elapsed().as_millis() as u64;
        match outcome {
            Ok(ExtractionOutcome::Proposed(candidate_ids)) => {
                checkpoint.records[index].state = UserMemoryHarvestState::Proposed;
                checkpoint.records[index].candidate_ids = Some(candidate_ids);
                checkpoint.records[index].processed_at = Some(now.clone());
                checkpoint.records[index].processing_ms = Some(elapsed);
                checkpoint.last_success_write_at = Some(now);
            }
            Ok(ExtractionOutcome::Noop(reason)) => {
                checkpoint.records[index].state = UserMemoryHarvestState::Noop;
                checkpoint.records[index].noop_reason = Some(reason);
                checkpoint.records[index].processed_at = Some(now);
                checkpoint.records[index].processing_ms = Some(elapsed);
            }
            Err(error) => {
                let kind = harvest_failure_kind(&error);
                checkpoint.records[index].state =
                    if checkpoint.records[index].attempts >= USER_MEMORY_HARVEST_MAX_RETRIES {
                        UserMemoryHarvestState::Dead
                    } else {
                        UserMemoryHarvestState::Failed
                    };
                checkpoint.records[index].failure_kind = Some(kind);
                checkpoint.records[index].failure_detail =
                    Some(error.detail.unwrap_or(error.message));
                checkpoint.records[index].processed_at = Some(now.clone());
                checkpoint.records[index].processing_ms = Some(elapsed);
                checkpoint.last_failure_at = Some(now);
            }
        }
        write_checkpoint(&root, &checkpoint)?;
        Ok(())
    }

    async fn extract_and_propose(
        &self,
        request: &MemoryHarvestRequest,
    ) -> Result<ExtractionOutcome, AppCommandError> {
        if abnormal_stop_reason(request.stop_reason.as_deref()) {
            return Ok(ExtractionOutcome::Noop("abnormal stop reason".to_string()));
        }
        let Some(user_input) = request.user_input_ref.as_deref() else {
            return Ok(ExtractionOutcome::Noop("missing user input".to_string()));
        };
        if user_input.chars().count() < USER_MEMORY_HARVEST_MIN_CONTENT_CHARS {
            return Ok(ExtractionOutcome::Noop("content too short".to_string()));
        }
        if contains_potential_secret(user_input) {
            return Ok(ExtractionOutcome::Noop("sensitive content".to_string()));
        }
        let mut candidate_ids = Vec::new();
        for sentence in candidate_sentences(user_input) {
            let Some(signal) = durable_signal(&sentence) else {
                continue;
            };
            let content = normalize_candidate(&sentence)?;
            let proposal = self
                .propose_harvest_candidate(content, signal, request)
                .await?;
            if proposal.observation_added || proposal.confirmation_recommended {
                candidate_ids.push(proposal.candidate.id);
            }
        }
        if candidate_ids.is_empty() {
            return Ok(ExtractionOutcome::Noop(
                "no durable signal in user input".to_string(),
            ));
        }
        Ok(ExtractionOutcome::Proposed(candidate_ids))
    }

    async fn propose_harvest_candidate(
        &self,
        content: String,
        signal: UserMemoryCandidateSignal,
        request: &MemoryHarvestRequest,
    ) -> Result<UserMemoryProposalResult, AppCommandError> {
        let opaque_source_id = derive_harvest_source_id(&request.conversation);
        self.propose_agent_memory_authorized(
            AgentMemoryProposal { content, signal },
            CandidateObservationSource {
                agent_type: request.agent_type,
                opaque_source_id,
                turn_nonce: request.turn_nonce,
            },
        )
        .await
    }

    async fn harvest_checkpoint(&self) -> Result<HarvestCheckpoint, AppCommandError> {
        let root = self.harvest_root()?;
        read_checkpoint(&root)
    }

    fn harvest_root(&self) -> Result<std::path::PathBuf, AppCommandError> {
        Ok(self.resolved_root()?.to_path_buf())
    }
}

enum ExtractionOutcome {
    Proposed(Vec<String>),
    Noop(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMemoryCandidateIndexRebuildResult {
    pub affected: u32,
    pub executed: bool,
    pub revision: String,
}

fn validate_harvest_request(request: &MemoryHarvestRequest) -> Result<(), AppCommandError> {
    if request.conversation.is_empty() || request.turn_nonce == 0 {
        return Err(AppCommandError::invalid_input(
            "Harvest request must carry a conversation and turn nonce",
        ));
    }
    for reference in [&request.user_input_ref, &request.assistant_input_ref] {
        if let Some(reference) = reference {
            if reference.chars().count() > USER_MEMORY_MAX_CANDIDATE_CHARS * 4 {
                return Err(AppCommandError::invalid_input(
                    "Harvest reference exceeds size limit",
                ));
            }
        }
    }
    Ok(())
}

/// Build a sanitized, bounded reference for the harvest hook (Task 13):
/// collapses whitespace/control characters into single spaces, trims, and
/// truncates to `USER_MEMORY_MAX_CANDIDATE_CHARS * 4` chars so the request
/// always passes `validate_harvest_request`. Returns `None` when nothing
/// meaningful remains (empty / whitespace-only input).
pub fn harvest_reference(input: &str) -> Option<String> {
    let cap = USER_MEMORY_MAX_CANDIDATE_CHARS * 4;
    let mut cleaned = String::new();
    let mut chars = 0usize;
    let mut pending_space = false;
    for ch in input.chars() {
        if chars >= cap {
            break;
        }
        if ch.is_whitespace() || ch.is_control() {
            pending_space = true;
            continue;
        }
        if pending_space && !cleaned.is_empty() {
            cleaned.push(' ');
            chars += 1;
            pending_space = false;
        }
        cleaned.push(ch);
        chars += 1;
    }
    let cleaned = cleaned.trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn abnormal_stop_reason(reason: Option<&str>) -> bool {
    let Some(reason) = reason else {
        return false;
    };
    let reason = reason.trim().to_ascii_lowercase();
    matches!(
        reason.as_str(),
        "cancelled"
            | "cancel"
            | "error"
            | "failed"
            | "failure"
            | "rate_limit_exceeded"
            | "interrupted"
            | "timeout"
    ) || reason.is_empty()
}

fn candidate_sentences(input: &str) -> Vec<String> {
    let separators = ['。', '！', '？', '!', '?', '\n'];
    let mut sentences = Vec::new();
    let mut current = String::new();
    for character in input.chars() {
        current.push(character);
        if separators.contains(&character) {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        sentences.push(trimmed.to_string());
    }
    sentences
}

fn durable_signal(sentence: &str) -> Option<UserMemoryCandidateSignal> {
    let lower = sentence.to_ascii_lowercase();
    const CORRECTION: &[&str] = &[
        "不要",
        "别再",
        "不要再",
        "不再",
        "never",
        "do not",
        "don't",
        "stop",
    ];
    const PREFERENCE: &[&str] = &[
        "喜欢",
        "偏好",
        "希望以后",
        "以后",
        "总是",
        "每次",
        "prefer",
        "always",
        "usually",
    ];
    const FACT: &[&str] = &["记住", "记得", "remember", "我是", "我的"];
    if CORRECTION.iter().any(|marker| sentence.contains(marker)) {
        return Some(UserMemoryCandidateSignal::Correction);
    }
    if PREFERENCE.iter().any(|marker| lower.contains(marker)) {
        return Some(UserMemoryCandidateSignal::Preference);
    }
    if FACT.iter().any(|marker| sentence.contains(marker)) {
        return Some(UserMemoryCandidateSignal::Fact);
    }
    None
}

pub(crate) fn derive_harvest_source_id(conversation: &str) -> String {
    hash_parts(&[b"iyw-claw:harvest-source:v1\0", conversation.as_bytes()])
}

fn read_checkpoint(root: &Path) -> Result<HarvestCheckpoint, AppCommandError> {
    let checkpoint = structured_file::read_json_optional::<HarvestCheckpoint>(
        root,
        USER_MEMORY_HARVEST_FILE,
        USER_MEMORY_HARVEST_MAX_STATE_CHARS,
    )?;
    let mut checkpoint = checkpoint.unwrap_or_default();
    if checkpoint.schema_version != USER_MEMORY_HARVEST_SCHEMA_VERSION {
        return Err(AppCommandError::configuration_invalid(
            "User memory harvest checkpoint version is unsupported",
        ));
    }
    // Restart recovery: mark interrupted extraction back to queued.
    for record in &mut checkpoint.records {
        if record.state == UserMemoryHarvestState::Extracting {
            record.state = UserMemoryHarvestState::Queued;
        }
    }
    Ok(checkpoint)
}

fn write_checkpoint(root: &Path, checkpoint: &HarvestCheckpoint) -> Result<(), AppCommandError> {
    if checkpoint.schema_version != USER_MEMORY_HARVEST_SCHEMA_VERSION {
        return Err(AppCommandError::configuration_invalid(
            "User memory harvest checkpoint version is unsupported",
        ));
    }
    structured_file::ensure_writable_optional(root, USER_MEMORY_HARVEST_FILE)?;
    structured_file::write_json_atomic(root, USER_MEMORY_HARVEST_FILE, checkpoint)
}

fn project_harvest_status(checkpoint: &HarvestCheckpoint) -> UserMemoryHarvestStatus {
    let mut queued = 0u32;
    let mut extracting = 0u32;
    let mut proposed = 0u32;
    let mut noop = 0u32;
    let mut failed = 0u32;
    let mut dead = 0u32;
    for record in &checkpoint.records {
        match record.state {
            UserMemoryHarvestState::Queued => queued += 1,
            UserMemoryHarvestState::Extracting => extracting += 1,
            UserMemoryHarvestState::Proposed => proposed += 1,
            UserMemoryHarvestState::Noop => noop += 1,
            UserMemoryHarvestState::Failed => failed += 1,
            UserMemoryHarvestState::Dead => dead += 1,
        }
    }
    UserMemoryHarvestStatus {
        queued,
        extracting,
        proposed,
        noop,
        failed,
        dead,
        backlog: queued + extracting + failed,
        last_harvest_at: checkpoint.last_harvest_at.clone(),
        last_success_write_at: checkpoint.last_success_write_at.clone(),
        last_failure_at: checkpoint.last_failure_at.clone(),
    }
}

fn harvest_failure_kind(error: &AppCommandError) -> UserMemoryHarvestFailureKind {
    use crate::app_error::AppErrorCode;
    match error.code {
        AppErrorCode::IoError | AppErrorCode::DatabaseError => UserMemoryHarvestFailureKind::Io,
        AppErrorCode::InvalidInput | AppErrorCode::PermissionDenied => {
            UserMemoryHarvestFailureKind::InvalidInput
        }
        _ => UserMemoryHarvestFailureKind::Internal,
    }
}
