use crate::app_error::AppCommandError;

use super::candidate_store;
use super::helpers::{conflict, normalize_candidate};
use super::{
    is_valid_memory_entry_id, new_candidate_id, AgentMemoryProposal, CandidateObservation,
    CandidateObservationSource, UserMemoryCandidate, UserMemoryCandidateDeleteRequest,
    UserMemoryCandidateDeleteResult, UserMemoryCandidateResolution,
    UserMemoryCandidateResolutionResult, UserMemoryCandidateResolveRequest,
    UserMemoryCandidateStateSnapshot, UserMemoryCandidateStatus, UserMemoryLearningState,
    UserMemoryProposalResult, UserMemoryService, USER_MEMORY_CANDIDATE_SCHEMA_VERSION,
    USER_MEMORY_MAX_CANDIDATES, USER_MEMORY_MAX_OBSERVATION_DETAILS,
};

impl UserMemoryService {
    pub async fn list_candidates(
        &self,
    ) -> Result<UserMemoryCandidateStateSnapshot, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
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

    pub async fn resolve_candidate(
        &self,
        request: UserMemoryCandidateResolveRequest,
    ) -> Result<UserMemoryCandidateResolutionResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?;
        let mut state = candidate_store::read_state(root)?;
        require_revision(&state, &request.expected_revision)?;
        let index = find_candidate(&state, &request.candidate_id)?;
        if state.candidates[index].status.is_terminal() {
            return Err(AppCommandError::invalid_input(
                "Terminal candidates cannot be resolved again",
            ));
        }
        self.apply_resolution(&mut state, index, request.resolution)
            .await?;
        candidate_store::write_state(root, &state)?;
        Ok(UserMemoryCandidateResolutionResult {
            candidate: state.candidates[index].clone(),
            revision: candidate_store::revision(&state)?,
        })
    }

    pub async fn delete_candidate(
        &self,
        request: UserMemoryCandidateDeleteRequest,
    ) -> Result<UserMemoryCandidateDeleteResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let root = self.resolved_root()?;
        let mut state = candidate_store::read_state(root)?;
        require_revision(&state, &request.expected_revision)?;
        let index = find_candidate(&state, &request.candidate_id)?;
        if !state.candidates[index].status.is_terminal() {
            return Err(AppCommandError::invalid_input(
                "Only terminal memory candidates can be deleted",
            ));
        }
        ensure_not_referenced(&state, &request.candidate_id)?;
        state.candidates.remove(index);
        candidate_store::write_state(root, &state)?;
        Ok(UserMemoryCandidateDeleteResult {
            deleted: true,
            revision: candidate_store::revision(&state)?,
        })
    }

    async fn apply_resolution(
        &self,
        state: &mut UserMemoryLearningState,
        index: usize,
        resolution: UserMemoryCandidateResolution,
    ) -> Result<(), AppCommandError> {
        match resolution {
            UserMemoryCandidateResolution::Confirm { edited_content } => {
                let _ = edited_content;
                Err(AppCommandError::configuration_missing(
                    "User memory confirmation transaction is unavailable",
                )
                .with_detail("user_memory_confirmation_transaction_unavailable"))
            }
            UserMemoryCandidateResolution::Reject => {
                ensure_not_referenced(state, &state.candidates[index].id)?;
                state.candidates[index].mark_terminal(UserMemoryCandidateStatus::Rejected);
                Ok(())
            }
            UserMemoryCandidateResolution::SupersedeByCandidate { candidate_id } => {
                ensure_not_referenced(state, &state.candidates[index].id)?;
                validate_candidate_target(state, index, &candidate_id)?;
                state.candidates[index].mark_terminal(UserMemoryCandidateStatus::Superseded);
                state.candidates[index].superseded_by_candidate_id = Some(candidate_id);
                Ok(())
            }
            UserMemoryCandidateResolution::SupersedeByMemoryEntry { entry_id } => {
                ensure_not_referenced(state, &state.candidates[index].id)?;
                validate_memory_entry_id(&entry_id)?;
                let marker = format!("<!-- {entry_id} -->");
                if !self
                    .read_document(super::UserMemoryDocumentId::Memory)?
                    .contains(&marker)
                {
                    return Err(AppCommandError::not_found(
                        "Superseding memory entry was not found",
                    ));
                }
                state.candidates[index].mark_terminal(UserMemoryCandidateStatus::Superseded);
                state.candidates[index].superseded_by_memory_entry_id = Some(entry_id);
                Ok(())
            }
        }
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

fn require_revision(
    state: &UserMemoryLearningState,
    expected_revision: &str,
) -> Result<(), AppCommandError> {
    if candidate_store::revision(state)? == expected_revision {
        Ok(())
    } else {
        Err(conflict(
            "User memory candidates changed; reload before saving",
        ))
    }
}

fn find_candidate(state: &UserMemoryLearningState, id: &str) -> Result<usize, AppCommandError> {
    state
        .candidates
        .iter()
        .position(|candidate| candidate.id == id)
        .ok_or_else(|| AppCommandError::not_found("User memory candidate not found"))
}

fn validate_candidate_target(
    state: &UserMemoryLearningState,
    index: usize,
    target: &str,
) -> Result<(), AppCommandError> {
    let target = state
        .candidates
        .iter()
        .find(|candidate| candidate.id == target);
    if state.candidates[index].id == target.map_or("", |candidate| candidate.id.as_str())
        || target.is_none_or(|candidate| {
            matches!(
                candidate.status,
                UserMemoryCandidateStatus::Rejected | UserMemoryCandidateStatus::Superseded
            )
        })
    {
        Err(AppCommandError::invalid_input(
            "Superseding candidate target is invalid",
        ))
    } else {
        Ok(())
    }
}

fn validate_memory_entry_id(entry_id: &str) -> Result<(), AppCommandError> {
    if is_valid_memory_entry_id(entry_id) {
        Ok(())
    } else {
        Err(AppCommandError::invalid_input(
            "Superseding memory entry identifier is invalid",
        ))
    }
}

fn ensure_not_referenced(
    state: &UserMemoryLearningState,
    candidate_id: &str,
) -> Result<(), AppCommandError> {
    if state
        .candidates
        .iter()
        .any(|candidate| candidate.superseded_by_candidate_id.as_deref() == Some(candidate_id))
    {
        Err(AppCommandError::invalid_input(
            "Referenced memory candidates cannot become obsolete",
        ))
    } else {
        Ok(())
    }
}
