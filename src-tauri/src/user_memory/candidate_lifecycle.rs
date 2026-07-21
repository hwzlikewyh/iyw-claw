use crate::app_error::AppCommandError;

use super::candidate_store;
use super::helpers::normalize_candidate;
use super::{
    new_candidate_id, AgentMemoryProposal, CandidateObservation, CandidateObservationSource,
    UserMemoryCandidate, UserMemoryCandidateStateSnapshot, UserMemoryCandidateStatus,
    UserMemoryLearningState, UserMemoryProposalResult, UserMemoryService,
    USER_MEMORY_CANDIDATE_SCHEMA_VERSION, USER_MEMORY_MAX_CANDIDATES,
    USER_MEMORY_MAX_OBSERVATION_DETAILS,
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
        let content = normalize_candidate(&proposal.content)?;
        source.validate()?;
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let root = self.resolved_root()?;
        let mut state = candidate_store::read_state(root)?;
        let outcome = observe_candidate(&mut state, content, proposal.signal, source)?;
        if outcome.observation_added {
            candidate_store::write_state(root, &state)?;
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
        return observe_existing(candidate, source);
    }
    if state.candidates.len() >= USER_MEMORY_MAX_CANDIDATES {
        return Err(AppCommandError::invalid_input(
            "User memory candidate limit reached",
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
    candidate.status =
        UserMemoryCandidateStatus::from_observation_count(candidate.observation_count);
    Ok(ObservationOutcome {
        observation_added: true,
        candidate: candidate.clone(),
    })
}
