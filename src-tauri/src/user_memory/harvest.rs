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
use super::harvest_store::{self, StoreOutcome};
use super::helpers::{contains_potential_secret, hash_parts, normalize_candidate};
use super::structured_file;
use super::{
    AgentExperience, AgentExperienceEvidence, AgentMemoryProposal, CandidateObservationSource,
    UserMemoryCandidateSignal, UserMemoryProposalResult, UserMemoryService,
    USER_MEMORY_MAX_CANDIDATE_CHARS, USER_MEMORY_MAX_EXPERIENCES,
    USER_MEMORY_MAX_EXPERIENCE_EVIDENCE,
};

pub const USER_MEMORY_HARVEST_FILE: &str = ".user-memory-harvest.json";
pub const USER_MEMORY_HARVEST_SCHEMA_VERSION: u32 = 1;
pub const USER_MEMORY_HARVEST_MAX_QUEUED: usize = 256;
pub const USER_MEMORY_HARVEST_MAX_RETRIES: u32 = 3;
pub const USER_MEMORY_HARVEST_MIN_CONTENT_CHARS: usize = 24;
pub const USER_MEMORY_HARVEST_MAX_STATE_CHARS: usize = 16_777_216;
const HARVEST_OUTBOX_IMPORT_KEY: &str = "user_memory.harvest_outbox_imported_v1";

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
    /// the durable conversation turn generation in `turn_nonce` is the queue's
    /// deduplication key. The field name is retained for wire compatibility.
    pub conversation: String,
    /// Turn nonce from the connection's `MemoryTurnTracker` (accepted prompt
    /// start; unique within a connection).
    pub turn_nonce: u64,
    /// Agent that ran the turn.
    pub agent_type: AgentType,
    /// Opaque workspace scope used only for Agent experience retrieval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_key: Option<String>,
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
    /// Sanitized, bounded summary of failed tool outcomes for experience
    /// extraction; raw tool payloads are never persisted here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_outcome_ref: Option<String>,
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
    pub(crate) experience_ids: Option<Vec<String>>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

impl UserMemoryHarvestStatus {
    pub(super) fn set_count(&mut self, state: &str, count: u32) {
        match state {
            "queued" => self.queued = count,
            "extracting" => self.extracting = count,
            "proposed" => self.proposed = count,
            "noop" => self.noop = count,
            "failed" => self.failed = count,
            "dead" => self.dead = count,
            _ => {}
        }
    }
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
        let result = harvest_store::submit(&self.db, &request).await;
        self.ensure_harvest_worker();
        result
    }

    pub async fn harvest_status(&self) -> Result<UserMemoryHarvestStatus, AppCommandError> {
        harvest_store::status(&self.db).await
    }

    /// Re-queue unprocessed or recoverable records. Returns a preview first;
    /// callers confirm before `execute = true` takes effect.
    pub async fn rescan_harvest(
        self: &Arc<Self>,
        execute: bool,
    ) -> Result<UserMemoryHarvestRescanResult, AppCommandError> {
        let result = harvest_store::rescan(&self.db, execute).await?;
        self.ensure_harvest_worker();
        Ok(result)
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
                    loop {
                        match service.process_recoverable_harvest().await {
                            Ok(0) => match harvest_store::next_retry_delay(&service.db).await {
                                Ok(Some(delay)) if delay.is_zero() => continue,
                                Ok(Some(delay)) => {
                                    tokio::select! {
                                        _ = tokio::time::sleep(delay) => {}
                                        _ = inner.wake.notified() => {}
                                    }
                                }
                                Ok(None) => break,
                                Err(error) => {
                                    tracing::warn!(
                                        error = %error,
                                        "[user-memory] harvest retry schedule read failed"
                                    );
                                    break;
                                }
                            },
                            Ok(_) => continue,
                            Err(error) => {
                                tracing::warn!(
                                    error = %error,
                                    "[user-memory] harvest worker pass failed"
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                break;
                            }
                        }
                    }
                }
            });
            ()
        });
        self.harvest.inner.wake.notify_one();
    }

    /// Start the harvest worker during application startup so records left by a
    /// previous process are drained even before the next completed turn.
    pub fn start_background_workers(self: &Arc<Self>) {
        self.ensure_harvest_worker();
        let service = Arc::clone(self);
        tokio::spawn(async move {
            match import_legacy_harvest_once(&service).await {
                Ok(imported) if imported > 0 => {
                    tracing::info!(imported, "[user-memory] imported legacy harvest checkpoint")
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(
                    error = %error,
                    "[user-memory] legacy harvest import failed"
                ),
            }
            if let Err(error) = harvest_store::recover_interrupted(&service.db).await {
                tracing::warn!(error = %error, "[user-memory] harvest recovery failed");
            }
            service.harvest.inner.wake.notify_one();
        });
    }

    async fn process_recoverable_harvest(self: &Arc<Self>) -> Result<usize, AppCommandError> {
        let recoverable = harvest_store::recoverable(&self.db).await?;
        let recoverable_count = recoverable.len();
        for request in recoverable {
            if let Err(error) = self.process_harvest_request(request).await {
                tracing::warn!(error = %error, "[user-memory] harvest record processing failed");
                return Err(error);
            }
        }
        Ok(recoverable_count)
    }

    async fn process_harvest_request(
        self: &Arc<Self>,
        request: MemoryHarvestRequest,
    ) -> Result<(), AppCommandError> {
        let started = std::time::Instant::now();
        let key = request.dedup_key();
        if !harvest_store::claim(&self.db, &key).await? {
            return Ok(());
        }
        let _ = super::task_history_store::project(&self.db, &request).await;
        let outcome = self.extract_and_propose(&request).await;
        let elapsed = started.elapsed().as_millis() as u64;
        let outcome = match outcome {
            Ok(ExtractionOutcome::Proposed {
                candidate_ids,
                experience_ids,
            }) => StoreOutcome::Proposed {
                candidate_ids,
                experience_ids,
            },
            Ok(ExtractionOutcome::Noop(reason)) => StoreOutcome::Noop(reason),
            Err(error) => StoreOutcome::Failed {
                kind: harvest_failure_kind(&error),
                detail: error.detail.unwrap_or(error.message),
            },
        };
        harvest_store::finish(&self.db, &key, elapsed, outcome).await
    }

    async fn extract_and_propose(
        &self,
        request: &MemoryHarvestRequest,
    ) -> Result<ExtractionOutcome, AppCommandError> {
        let mut candidate_ids = Vec::new();
        if !abnormal_stop_reason(request.stop_reason.as_deref()) {
            if let Some(user_input) = request.user_input_ref.as_deref() {
                if user_input.chars().count() >= USER_MEMORY_HARVEST_MIN_CONTENT_CHARS
                    && !contains_potential_secret(user_input)
                {
                    for sentence in candidate_sentences(user_input) {
                        let Some(signal) = durable_signal(&sentence) else {
                            continue;
                        };
                        let Some(content) = normalize_harvest_candidate(&sentence, "user")? else {
                            continue;
                        };
                        let proposal = self
                            .propose_harvest_candidate(content, signal, request)
                            .await?;
                        if proposal.observation_added || proposal.confirmation_recommended {
                            candidate_ids.push(proposal.candidate.id);
                        }
                    }
                }
            }
        }
        let experience_ids = self.extract_experiences(request).await?;
        if candidate_ids.is_empty() && experience_ids.is_empty() {
            return Ok(ExtractionOutcome::Noop(
                "no reusable signal in completed turn".to_string(),
            ));
        }
        Ok(ExtractionOutcome::Proposed {
            candidate_ids,
            experience_ids,
        })
    }

    async fn extract_experiences(
        &self,
        request: &MemoryHarvestRequest,
    ) -> Result<Vec<String>, AppCommandError> {
        let mut ids = Vec::new();
        if let Some(text) = request.assistant_input_ref.as_deref() {
            for sentence in candidate_sentences(text) {
                if !experience_sentence_allowed(&sentence) {
                    continue;
                }
                let Some(content) = normalize_harvest_candidate(&sentence, "experience")? else {
                    continue;
                };
                ids.push(self.record_agent_experience(content, request).await?);
            }
        }
        Ok(ids)
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

    async fn record_agent_experience(
        &self,
        content: String,
        request: &MemoryHarvestRequest,
    ) -> Result<String, AppCommandError> {
        let (_io_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?.to_path_buf();
        let mut state = candidate_store::read_state(&root)?;
        let digest = experience_digest(&content);
        let id = experience_id(&digest);
        let now = chrono::Utc::now().to_rfc3339();
        let evidence = AgentExperienceEvidence {
            opaque_source_id: derive_harvest_source_id(&request.conversation),
            turn_nonce: request.turn_nonce,
            observed_at: now.clone(),
        };
        if let Some(existing) = state
            .experiences
            .iter_mut()
            .find(|experience| experience.content_digest == digest)
        {
            if !existing.evidence.iter().any(|item| {
                item.opaque_source_id == evidence.opaque_source_id
                    && item.turn_nonce == evidence.turn_nonce
            }) {
                existing.observation_count = existing.observation_count.saturating_add(1);
                existing.confidence = (40 + existing.observation_count.saturating_mul(15)).min(100);
                existing.last_observed_at = now;
                existing.evidence.push(evidence);
                if existing.evidence.len() > USER_MEMORY_MAX_EXPERIENCE_EVIDENCE {
                    existing.evidence.remove(0);
                }
                candidate_store::write_state(&root, &state)?;
            }
        } else {
            if state.experiences.len() >= USER_MEMORY_MAX_EXPERIENCES {
                state.experiences.sort_by(|left, right| {
                    left.confidence
                        .cmp(&right.confidence)
                        .then(left.last_observed_at.cmp(&right.last_observed_at))
                });
                state.experiences.remove(0);
            }
            state.experiences.push(AgentExperience {
                id: id.clone(),
                content_digest: digest,
                content,
                agent_type: request.agent_type,
                scope_type: if request
                    .workspace_key
                    .as_deref()
                    .is_some_and(|key| !key.is_empty())
                {
                    "workspace".to_string()
                } else {
                    "global".to_string()
                },
                scope_key: request
                    .workspace_key
                    .clone()
                    .filter(|key| !key.is_empty())
                    .unwrap_or_default(),
                observation_count: 1,
                confidence: 40,
                first_observed_at: now.clone(),
                last_observed_at: now,
                evidence: vec![evidence],
                superseded_by: None,
            });
            candidate_store::write_state(&root, &state)?;
        }
        drop(_file_guard);
        drop(_io_guard);
        self.schedule_index_refresh();
        Ok(id)
    }

    fn harvest_root(&self) -> Result<std::path::PathBuf, AppCommandError> {
        Ok(self.resolved_root()?.to_path_buf())
    }
}

fn normalize_harvest_candidate(
    sentence: &str,
    candidate_type: &'static str,
) -> Result<Option<String>, AppCommandError> {
    match normalize_candidate(sentence) {
        Ok(content) => Ok(Some(content)),
        Err(error)
            if error.code == crate::app_error::AppErrorCode::InvalidInput
                && error.message == "Memory entry is too long" =>
        {
            tracing::warn!(
                target: "user_memory",
                candidate_type,
                content_chars = sentence.chars().count(),
                max_chars = super::USER_MEMORY_MAX_CANDIDATE_CHARS,
                "skipped oversized harvest candidate"
            );
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

async fn import_legacy_harvest_once(
    service: &Arc<UserMemoryService>,
) -> Result<usize, AppCommandError> {
    use crate::db::service::app_metadata_service;
    if app_metadata_service::get_value(&service.db, HARVEST_OUTBOX_IMPORT_KEY)
        .await
        .map_err(AppCommandError::from)?
        .is_some()
    {
        return Ok(0);
    }
    let root = service.harvest_root()?;
    let checkpoint = read_checkpoint(&root)?;
    let imported = super::harvest_legacy::import(&service.db, &checkpoint.records).await?;
    app_metadata_service::upsert_value(&service.db, HARVEST_OUTBOX_IMPORT_KEY, "1")
        .await
        .map_err(AppCommandError::from)?;
    Ok(imported)
}

enum ExtractionOutcome {
    Proposed {
        candidate_ids: Vec<String>,
        experience_ids: Vec<String>,
    },
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
    if request
        .workspace_key
        .as_deref()
        .is_some_and(|key| key.chars().count() > 512)
    {
        return Err(AppCommandError::invalid_input(
            "Harvest workspace scope exceeds size limit",
        ));
    }
    for reference in [
        &request.user_input_ref,
        &request.assistant_input_ref,
        &request.tool_outcome_ref,
    ] {
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
    (!cleaned.is_empty() && !contains_potential_secret(&cleaned)).then_some(cleaned)
}

pub(super) fn abnormal_stop_reason(reason: Option<&str>) -> bool {
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
        "不对",
        "其实",
        "不要",
        "别再",
        "不要再",
        "不再",
        "以后不要",
        "请改",
        "改成",
        "改为",
        "never",
        "do not",
        "don't",
        "stop",
        "actually",
        "instead",
    ];
    const PREFERENCE: &[&str] = &[
        "我喜欢",
        "我偏好",
        "喜欢",
        "偏好",
        "希望以后",
        "以后请",
        "总是",
        "从不",
        "每次",
        "必须",
        "for me",
        "i like",
        "i prefer",
        "prefer",
        "always",
        "usually",
    ];
    const FACT: &[&str] = &[
        "记住",
        "记得",
        "请记住",
        "remember",
        "我是",
        "我叫",
        "我的名字",
        "my name",
        "i am",
    ];
    if CORRECTION.iter().any(|marker| lower.contains(marker)) {
        return Some(UserMemoryCandidateSignal::Correction);
    }
    if PREFERENCE.iter().any(|marker| lower.contains(marker)) {
        return Some(UserMemoryCandidateSignal::Preference);
    }
    if FACT.iter().any(|marker| lower.contains(marker)) {
        return Some(UserMemoryCandidateSignal::Fact);
    }
    None
}

pub(crate) fn experience_digest(content: &str) -> String {
    hash_parts(&[b"iyw-agent-experience:v1\0", content.as_bytes()])
}

fn experience_id(digest: &str) -> String {
    format!("iyw-experience-{}", &digest[..32])
}

fn experience_sentence_allowed(sentence: &str) -> bool {
    sentence.chars().count() >= USER_MEMORY_HARVEST_MIN_CONTENT_CHARS
        && is_experience_sentence(sentence)
        && !contains_potential_secret(sentence)
}

fn is_experience_sentence(sentence: &str) -> bool {
    const MARKERS: &[&str] = &[
        "原因",
        "根因",
        "问题在",
        "解决",
        "解决方案",
        "修复",
        "排查",
        "结论",
        "注意",
        "避免",
        "建议",
        "经验",
        "教训",
        "下次",
        "改为",
        "workaround",
        "root cause",
        "fixed",
        "lesson",
        "next time",
        "avoid",
        "pitfall",
        "best practice",
    ];
    let lower = sentence.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|marker| sentence.contains(marker) || lower.contains(marker))
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
