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
        let state = candidate_store::read_state(self.resolved_root()?)?;
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
        let _authorization_lease = acquire_lease().ok_or_else(|| {
            AppCommandError::permission_denied(
                "User memory proposal is unavailable for this session.",
            )
        })?;
        self.propose_agent_memory_locked(content, proposal.signal, source)
    }

    fn propose_agent_memory_locked(
        &self,
        content: String,
        signal: super::UserMemoryCandidateSignal,
        source: CandidateObservationSource,
    ) -> Result<UserMemoryProposalResult, AppCommandError> {
        let root = self.resolved_root()?;
        let mut state = candidate_store::read_state(root)?;
        let outcome = observe_candidate(&mut state, content, signal, source)?;
        if outcome.observation_added {
            candidate_store::write_state(root, &state)?;
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

struct ObservationOutcome {
    observation_added: bool,
    candidate: UserMemoryCandidate,
}

fn observe_candidate(
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
    // Controlled similarity merge: same signal, non-terminal, and the new
    // normalized wording is a character-multiset variant of an existing
    // candidate (e.g. "prefer dark theme" vs "prefer the dark theme").
    // Wording differences are preserved so the user can review both forms.
    if let Some(candidate) = state.candidates.iter_mut().find(|candidate| {
        candidate.signal == signal
            && !candidate.status.is_terminal()
            && candidate_store::candidates_equivalent(&candidate.content, &content)
    }) {
        return observe_existing(candidate, source, Some(content));
    }
    if state.candidates.len() >= USER_MEMORY_MAX_CANDIDATES {
        prune_terminal_oldest(state);
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

/// Reclaim the oldest resolved (terminal) candidates so learning never stops
/// permanently at the cap. Referenced targets remain until their references
/// are normalized; active candidates are never touched by this path.
fn prune_terminal_oldest(state: &mut UserMemoryLearningState) {
    let mut terminal = state
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if candidate_references::references_candidate(state, &candidate.id) {
                None
            } else {
                candidate.resolved_at.as_deref().and_then(|resolved_at| {
                    chrono::DateTime::parse_from_rfc3339(resolved_at)
                        .ok()
                        .map(|resolved_at| (index, resolved_at))
                })
            }
        })
        .collect::<Vec<_>>();
    terminal.sort_by_key(|(_, resolved_at)| resolved_at.clone());
    let reclaim = state
        .candidates
        .len()
        .saturating_sub(USER_MEMORY_MAX_CANDIDATES - 1);
    let mut reclaim_indexes = terminal
        .into_iter()
        .take(reclaim)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    reclaim_indexes.sort_unstable_by(|left, right| right.cmp(left));
    for index in reclaim_indexes {
        state.candidates.remove(index);
    }
}

fn confidence_for(observation_count: u32) -> u32 {
    observation_count.saturating_mul(20).min(100)
}
