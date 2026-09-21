use crate::app_error::AppCommandError;

use super::candidate_references;
use super::candidate_store;
use super::helpers::normalize_candidate;
use super::{
    new_candidate_id, AgentMemoryProposal, CandidateObservation, CandidateObservationSource,
    UserMemoryCandidate, UserMemoryCandidateStateSnapshot, UserMemoryCandidateStatus,
    UserMemoryLearningState, UserMemoryProposalResult, UserMemoryService,
    USER_MEMORY_CANDIDATE_SCHEMA_VERSION, USER_MEMORY_MAX_CANDIDATES,
    USER_MEMORY_MAX_OBSERVATION_DETAILS, USER_MEMORY_MAX_WORDING_VARIANTS,
};

impl UserMemoryService {
    pub async fn list_candidates(
        &self,
    ) -> Result<UserMemoryCandidateStateSnapshot, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let state = self.read_learning_state()?;
        candidate_store::snapshot(&state)
    }

    pub async fn propose_agent_memory_authorized(
        &self,
        proposal: AgentMemoryProposal,
        source: CandidateObservationSource,
    ) -> Result<UserMemoryProposalResult, AppCommandError> {
        self.propose_agent_memory_authorized_with_lease(proposal, source, || Some(()))
            .await
    }

    pub(crate) async fn propose_agent_memory_authorized_with_lease<F, L>(
        &self,
        proposal: AgentMemoryProposal,
        source: CandidateObservationSource,
        acquire_lease: F,
    ) -> Result<UserMemoryProposalResult, AppCommandError>
    where
        F: FnOnce() -> Option<L> + Send,
    {
        let content = normalize_candidate(&proposal.content)?;
        source.validate()?;
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        if self.is_forgotten_content(&content).await? {
            return Err(AppCommandError::permission_denied(
                "Forgotten content cannot be learned again automatically",
            ));
        }
        let _authorization_lease = acquire_lease().ok_or_else(|| {
            AppCommandError::permission_denied(
                "User memory proposal is unavailable for this session.",
            )
        })?;
        self.propose_agent_memory_locked(content, proposal.signal, source)
            .await
    }

    async fn propose_agent_memory_locked(
        &self,
        content: String,
        signal: super::UserMemoryCandidateSignal,
        source: CandidateObservationSource,
    ) -> Result<UserMemoryProposalResult, AppCommandError> {
        let mut state = self.read_learning_state()?;
        let outcome = observe_candidate(&mut state, content, signal, source)?;
        if outcome.observation_added {
            self.persist_learning_state(&state).await?;
            self.schedule_index_refresh();
        }
        let revision = candidate_store::revision(&state)?;
        Ok(UserMemoryProposalResult {
            confirmation_recommended: outcome.candidate.status
                == UserMemoryCandidateStatus::PendingConfirmation,
            observation_added: outcome.observation_added,
            candidate: outcome.candidate,
            revision,
        })
    }
}

pub(super) struct ObservationOutcome {
    pub observation_added: bool,
    pub candidate: UserMemoryCandidate,
}

pub(super) fn observe_candidate(
    state: &mut UserMemoryLearningState,
    content: String,
    signal: super::UserMemoryCandidateSignal,
    source: CandidateObservationSource,
) -> Result<ObservationOutcome, AppCommandError> {
    let digest = candidate_store::deduplication_digest(&content, signal);
    if let Some(candidate) = state
        .candidates
        .iter_mut()
        .find(|candidate| candidate.deduplication_digest == digest)
    {
        return observe_existing(candidate, source, None);
    }
    // 自动归并仅允许格式差异，语义不同的候选保留独立来源等待判断。
    if let Some(candidate) = state.candidates.iter_mut().find(|candidate| {
        candidate.signal == signal
            && !candidate.status.is_terminal()
            && candidate_store::candidates_equivalent(&candidate.content, &content)
    }) {
        return observe_existing(candidate, source, Some(content));
    }
    if state.candidates.len() >= USER_MEMORY_MAX_CANDIDATES {
        reclaim_oldest_candidate(state);
    }
    if state.candidates.len() >= USER_MEMORY_MAX_CANDIDATES {
        return Err(AppCommandError::invalid_input(
            "User memory candidate limit reached and no terminal candidates can be reclaimed",
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let observation_key =
        candidate_store::observation_key(&digest, &source.opaque_source_id, source.turn_nonce);
    let observation = CandidateObservation::from_source(source.clone(), now.clone());
    let candidate = UserMemoryCandidate {
        id: new_candidate_id(),
        deduplication_digest: digest,
        content,
        signal,
        status: UserMemoryCandidateStatus::Tentative,
        observation_count: 1,
        observations: vec![observation],
        observation_keys: vec![observation_key],
        first_observed_at: now.clone(),
        last_observed_at: now,
        confidence: confidence_for(1),
        wording_variants: Vec::new(),
        resolved_at: None,
        resolved_content: None,
        confirmed_memory_entry_id: None,
        superseded_by_candidate_id: None,
        superseded_by_memory_entry_id: None,
    };
    state.schema_version = USER_MEMORY_CANDIDATE_SCHEMA_VERSION;
    state.candidates.push(candidate.clone());
    Ok(ObservationOutcome {
        observation_added: true,
        candidate,
    })
}

fn observe_existing(
    candidate: &mut UserMemoryCandidate,
    source: CandidateObservationSource,
    wording_variant: Option<String>,
) -> Result<ObservationOutcome, AppCommandError> {
    let observation_key = candidate_store::observation_key(
        &candidate.deduplication_digest,
        &source.opaque_source_id,
        source.turn_nonce,
    );
    if candidate.status.is_terminal() || candidate.observation_keys.contains(&observation_key) {
        return Ok(ObservationOutcome {
            observation_added: false,
            candidate: candidate.clone(),
        });
    }
    if let Some(variant) = wording_variant {
        if candidate.content != variant
            && !candidate.wording_variants.contains(&variant)
            && candidate.wording_variants.len() < USER_MEMORY_MAX_WORDING_VARIANTS
        {
            candidate.wording_variants.push(variant);
        }
    }
    let now = chrono::Utc::now().to_rfc3339();
    candidate.observation_count = candidate
        .observation_count
        .checked_add(1)
        .ok_or_else(|| AppCommandError::invalid_input("Observation count limit reached"))?;
    candidate.observation_keys.push(observation_key);
    if candidate.observations.len() == USER_MEMORY_MAX_OBSERVATION_DETAILS {
        candidate.observations.remove(0);
    }
    candidate
        .observations
        .push(CandidateObservation::from_source(source, now.clone()));
    candidate.last_observed_at = now;
    candidate.confidence = confidence_for(candidate.observation_count);
    candidate.status =
        UserMemoryCandidateStatus::from_observation_count(candidate.observation_count);
    Ok(ObservationOutcome {
        observation_added: true,
        candidate: candidate.clone(),
    })
}

/// Reclaim one unreferenced candidate at capacity so learning cannot stop
/// permanently. Resolved history is preferred; otherwise the oldest active
/// observation is autonomously skipped before it is removed.
fn reclaim_oldest_candidate(state: &mut UserMemoryLearningState) {
    let mut reclaimable = state
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if candidate_references::references_candidate(state, &candidate.id) {
                None
            } else {
                let timestamp = candidate
                    .resolved_at
                    .as_deref()
                    .unwrap_or(&candidate.last_observed_at);
                chrono::DateTime::parse_from_rfc3339(timestamp)
                    .ok()
                    .map(|timestamp| (index, candidate.status.is_terminal(), timestamp))
            }
        })
        .collect::<Vec<_>>();
    reclaimable.sort_by_key(|(_, terminal, timestamp)| (!terminal, timestamp.clone()));
    if let Some((index, terminal, _)) = reclaimable.first().cloned() {
        let candidate = &state.candidates[index];
        tracing::info!(
            candidate_id = %candidate.id,
            previous_status = ?candidate.status,
            observation_count = candidate.observation_count,
            autonomous_skip = !terminal,
            "[user-memory] reclaimed oldest candidate at capacity"
        );
        state.candidates.remove(index);
    }
}

fn confidence_for(observation_count: u32) -> u32 {
    observation_count.saturating_mul(20).min(100)
}
