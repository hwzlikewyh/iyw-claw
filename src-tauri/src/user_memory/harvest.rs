//! TurnComplete harvest queue: bounded, persisted, async candidate extraction.
//!
//! The ACP TurnComplete hook (wired by Task 13) submits a small
//! [`MemoryHarvestRequest`] via [`UserMemoryService::submit_turn_harvest`].
//! This module persists a checkpoint in the user memory root, deduplicates by
//! (conversation, turn nonce), and accepts explicit structured Agent lessons
//! asynchronously without blocking the UI completion event. User-memory
//! candidates are proposed by the Agent through the authenticated MCP route;
//! the host never guesses them from ordinary conversation text.
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
use super::helpers::{contains_potential_secret, hash_parts};
use super::structured_file;
use super::{
    AgentExperience, AgentExperienceEvidence, UserMemoryService, USER_MEMORY_MAX_CANDIDATE_CHARS,
    USER_MEMORY_MAX_EXPERIENCES, USER_MEMORY_MAX_EXPERIENCE_EVIDENCE,
};

pub const USER_MEMORY_HARVEST_FILE: &str = ".user-memory-harvest.json";
pub const USER_MEMORY_HARVEST_SCHEMA_VERSION: u32 = 1;
pub const USER_MEMORY_HARVEST_MAX_QUEUED: usize = 256;
pub const USER_MEMORY_HARVEST_MAX_RETRIES: u32 = 3;
pub const USER_MEMORY_HARVEST_MIN_CONTENT_CHARS: usize = 24;
pub const USER_MEMORY_HARVEST_MAX_STATE_CHARS: usize = 16_777_216;
const AGENT_LESSON_START: &str = "<!-- IYW_CLAW_AGENT_LESSON_V1 ";
const AGENT_LESSON_END: &str = " -->";
const MAX_AGENT_LESSON_CHARS: usize = 2_400;
const HARVEST_OUTBOX_IMPORT_KEY: &str = "user_memory.harvest_outbox_imported_v1";
const HARVEST_STORE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

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
    #[serde(default)]
    pub pending_submissions: u32,
    #[serde(default)]
    pub discovered_unqueued: u32,
    #[serde(default)]
    pub skipped_sensitive_unqueued: u32,
    #[serde(default)]
    pub skipped_context_poor_unqueued: u32,
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
    pub retryable_dead: u32,
    pub retained_terminal: u32,
    pub recovered_dead: u32,
    pub discovered_unqueued: u32,
    pub recovered_unqueued: u32,
    pub skipped_sensitive: u32,
    pub skipped_context_poor: u32,
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
        let result = self.persist_harvest_submission(&request).await;
        self.ensure_harvest_worker();
        result
    }

    pub async fn harvest_status(&self) -> Result<UserMemoryHarvestStatus, AppCommandError> {
        let mut status = harvest_store::status(&self.db).await?;
        status.pending_submissions = self.pending_harvest_submissions().await?;
        let discovery = super::harvest_reconcile::discover(self).await?;
        status.discovered_unqueued = discovery.requests.len() as u32;
        status.skipped_sensitive_unqueued = discovery.skipped_sensitive;
        status.skipped_context_poor_unqueued = discovery.skipped_context_poor;
        status.backlog = status
            .backlog
            .saturating_add(status.pending_submissions)
            .saturating_add(status.discovered_unqueued);
        Ok(status)
    }

    /// Re-queue unprocessed or recoverable records. Returns a preview first;
    /// callers confirm before `execute = true` takes effect.
    pub async fn rescan_harvest(
        self: &Arc<Self>,
        execute: bool,
    ) -> Result<UserMemoryHarvestRescanResult, AppCommandError> {
        let discovery = super::harvest_reconcile::discover(self).await?;
        let mut result = harvest_store::rescan(&self.db, execute).await?;
        result.preview.discovered_unqueued = discovery.requests.len() as u32;
        result.preview.skipped_sensitive = discovery.skipped_sensitive;
        result.preview.skipped_context_poor = discovery.skipped_context_poor;
        if execute {
            let staged = self.pending_harvest_submissions().await? as usize;
            let available = harvest_store::available_slots(&self.db)
                .await?
                .saturating_sub(staged);
            for request in discovery.requests.into_iter().take(available) {
                if self.persist_harvest_submission(&request).await?.enqueued {
                    result.preview.recovered_unqueued += 1;
                }
            }
            self.ensure_harvest_worker();
        }
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
        let mut state = self.read_learning_state()?;
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
            self.persist_learning_state(&state).await?;
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
                                tokio::time::sleep(HARVEST_STORE_RETRY_DELAY).await;
                                inner.wake.notify_one();
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
        #[cfg(not(feature = "tauri-runtime"))]
        self.schedule_semantic_refresh();
        self.ensure_harvest_worker();
        self.start_maintenance_worker();
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
        let pending_error = self.replay_pending_harvest().await.err();
        let recoverable = harvest_store::recoverable(&self.db).await?;
        if recoverable.is_empty() {
            if let Some(error) = pending_error {
                return Err(error);
            }
        }
        let recoverable_count = recoverable.len();
        for request in recoverable {
            self.wait_for_foreground().await;
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
        let outcome = self.extract_lessons(&request).await;
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
                retryable: harvest_error_is_retryable(&error),
                detail: error.detail.unwrap_or(error.message),
            },
        };
        harvest_store::finish(&self.db, &key, elapsed, outcome).await
    }

    async fn extract_lessons(
        &self,
        request: &MemoryHarvestRequest,
    ) -> Result<ExtractionOutcome, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        if !policy.enabled
            || !policy.agent_write_enabled
            || !policy
                .per_agent
                .get(&request.agent_type)
                .copied()
                .unwrap_or(true)
        {
            return Ok(ExtractionOutcome::Noop(
                "memory learning is disabled".to_string(),
            ));
        }
        let experience_ids = self.extract_experiences(request).await?;
        let candidate_ids = self.extract_background_candidates(request).await?;
        if self.learning_config().await?.enabled {
            if let Err(error) = self.refresh_generated_views().await {
                tracing::warn!(code=?error.code,"[memory-views] generated view refresh deferred");
            }
        }
        if experience_ids.is_empty() && candidate_ids.is_empty() {
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
        let Some(text) = request.assistant_input_ref.as_deref() else {
            return Ok(Vec::new());
        };
        let mut ids = Vec::new();
        for lesson in extract_agent_lessons(text) {
            if self.is_forgotten_content(&lesson).await? {
                continue;
            }
            ids.push(self.record_agent_experience(lesson, request).await?);
        }
        Ok(ids)
    }

    async fn record_agent_experience(
        &self,
        content: String,
        request: &MemoryHarvestRequest,
    ) -> Result<String, AppCommandError> {
        let (_io_guard, _file_guard) = self.acquire_locks().await?;
        let mut state = self.read_learning_state()?;
        let digest = experience_digest(&content);
        let scope_key = self
            .memory_workspace_key(request.workspace_key.as_deref())
            .unwrap_or_default();
        let mut id = experience_id(&hash_parts(&[digest.as_bytes(), scope_key.as_bytes()]));
        let now = chrono::Utc::now().to_rfc3339();
        let evidence = AgentExperienceEvidence {
            opaque_source_id: derive_harvest_source_id(&request.conversation),
            turn_nonce: request.turn_nonce,
            observed_at: now.clone(),
        };
        if let Some(existing) = state.experiences.iter_mut().find(|experience| {
            experience.content_digest == digest
                && self
                    .memory_workspace_key(Some(&experience.scope_key))
                    .unwrap_or_default()
                    == scope_key
        }) {
            id = existing.id.clone();
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
                self.persist_learning_state(&state).await?;
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
                scope_key,
                observation_count: 1,
                confidence: 40,
                first_observed_at: now.clone(),
                last_observed_at: now,
                evidence: vec![evidence],
                superseded_by: None,
            });
            self.persist_learning_state(&state).await?;
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

pub(super) fn validate_harvest_request(
    request: &MemoryHarvestRequest,
) -> Result<(), AppCommandError> {
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

/// 经验位于回答末尾，先保留有效经验，再为普通回答分配剩余长度。
pub fn assistant_harvest_reference(input: &str) -> Option<String> {
    let visible = harvest_reference(&strip_agent_lessons(input));
    let Some(start) = input.rfind(AGENT_LESSON_START) else {
        return visible;
    };
    let envelope = input[start..].trim();
    if extract_agent_lessons(envelope).is_empty() {
        tracing::debug!("[memory-harvest] invalid experience envelope omitted");
        return visible;
    }
    let cap = USER_MEMORY_MAX_CANDIDATE_CHARS * 4;
    let remaining = cap.saturating_sub(envelope.chars().count() + "\n\n".len());
    let prefix = visible
        .unwrap_or_default()
        .chars()
        .take(remaining)
        .collect::<String>();
    if prefix.is_empty() {
        Some(envelope.to_string())
    } else {
        Some(format!("{}\n\n{envelope}", prefix.trim_end()))
    }
}

/// Parse only the explicit Agent-owned lesson envelope. The host deliberately
/// does not infer lessons from ordinary prose: a missing envelope means no
/// experience is persisted.
pub fn extract_agent_lessons(input: &str) -> Vec<String> {
    let mut lessons = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = input[cursor..].find(AGENT_LESSON_START) {
        let start = cursor + relative + AGENT_LESSON_START.len();
        let Some(end_relative) = input[start..].find(AGENT_LESSON_END) else {
            break;
        };
        let end = start + end_relative;
        if !input[end + AGENT_LESSON_END.len()..].trim().is_empty() {
            cursor = end + AGENT_LESSON_END.len();
            continue;
        }
        let payload = input[start..end].trim();
        if let Some(lesson) = parse_agent_lesson(payload) {
            if !lessons.contains(&lesson) {
                lessons.push(lesson);
            }
        }
        cursor = end + AGENT_LESSON_END.len();
    }
    lessons
}

/// Remove internal lesson envelopes before a transcript or UI projection is
/// persisted. This keeps Agent-led learning invisible to the user.
pub fn strip_agent_lessons(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;
    while let Some(relative) = input[cursor..].find(AGENT_LESSON_START) {
        let start = cursor + relative;
        output.push_str(&input[cursor..start]);
        let payload_start = start + AGENT_LESSON_START.len();
        let Some(end_relative) = input[payload_start..].find(AGENT_LESSON_END) else {
            // An incomplete internal envelope is never user-facing content.
            // Drop it rather than leaking protocol text into the transcript.
            output.truncate(output.trim_end().len());
            cursor = input.len();
            break;
        };
        cursor = payload_start + end_relative + AGENT_LESSON_END.len();
    }
    output.push_str(&input[cursor..]);
    output.trim().to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentLessonEnvelope {
    context: String,
    outcome: String,
    lesson: String,
    evidence: String,
    verification: String,
    reuse_when: String,
}

fn parse_agent_lesson(payload: &str) -> Option<String> {
    if payload.chars().count() > MAX_AGENT_LESSON_CHARS {
        return None;
    }
    let lesson: AgentLessonEnvelope = serde_json::from_str(payload).ok()?;
    let fields = [
        lesson.context,
        lesson.outcome,
        lesson.lesson,
        lesson.evidence,
        lesson.verification,
        lesson.reuse_when,
    ];
    if fields.iter().any(|value| value.trim().is_empty())
        || fields.iter().any(|value| contains_potential_secret(value))
    {
        return None;
    }
    let content = format!(
        "Context: {}; Outcome: {}; Lesson: {}; Evidence: {}; Verification: {}; Reuse when: {}",
        fields[0], fields[1], fields[2], fields[3], fields[4], fields[5]
    );
    (content.chars().count() >= USER_MEMORY_HARVEST_MIN_CONTENT_CHARS
        && content.chars().count() <= USER_MEMORY_MAX_CANDIDATE_CHARS * 2)
        .then_some(content)
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

pub(crate) fn experience_digest(content: &str) -> String {
    hash_parts(&[b"iyw-agent-experience:v1\0", content.as_bytes()])
}

fn experience_id(digest: &str) -> String {
    format!("iyw-experience-{}", &digest[..32])
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

fn harvest_error_is_retryable(error: &AppCommandError) -> bool {
    use crate::app_error::AppErrorCode;
    matches!(
        error.code,
        AppErrorCode::IoError | AppErrorCode::DatabaseError | AppErrorCode::NetworkError
    )
}

#[cfg(test)]
mod tests {
    use super::{extract_agent_lessons, strip_agent_lessons};

    fn envelope() -> &'static str {
        r#"<!-- IYW_CLAW_AGENT_LESSON_V1 {"context":"Rust build","outcome":"fixed","lesson":"Use the isolated target when the shared Cargo lock is stale","evidence":"cargo stayed before rustc","verification":"isolated cargo check passed","reuseWhen":"a Windows Cargo check stalls before rustc"} -->"#
    }

    #[test]
    fn only_explicit_structured_lessons_are_extracted() {
        let input = format!("visible answer\n{}", envelope());
        let lessons = extract_agent_lessons(&input);
        assert_eq!(lessons.len(), 1);
        assert!(lessons[0].contains("Use the isolated target"));
    }

    #[test]
    fn lesson_envelope_is_removed_from_visible_text() {
        let input = format!("visible answer\n{}", envelope());
        assert_eq!(strip_agent_lessons(&input), "visible answer");
    }

    #[test]
    fn malformed_lesson_never_leaks_to_visible_text() {
        let input = "visible answer <!-- IYW_CLAW_AGENT_LESSON_V1 {\"lesson\":\"oops\"}";
        assert_eq!(strip_agent_lessons(input), "visible answer");
        assert!(extract_agent_lessons(input).is_empty());
    }

    #[test]
    fn ordinary_reflection_prose_is_not_a_lesson() {
        let input = "结论：问题已经修复。建议下次先检查缓存。";
        assert!(extract_agent_lessons(input).is_empty());
    }

    #[test]
    fn lesson_must_be_the_final_internal_block() {
        let input = format!("{}\nextra visible text", envelope());
        assert!(extract_agent_lessons(&input).is_empty());
    }
}
