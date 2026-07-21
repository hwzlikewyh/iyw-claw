use crate::app_error::AppCommandError;

use super::candidate_store;
use super::helpers::conflict;
use super::{
    is_valid_memory_entry_id, UserMemoryCandidateDeleteRequest, UserMemoryCandidateDeleteResult,
    UserMemoryCandidateResolution, UserMemoryCandidateResolutionResult,
    UserMemoryCandidateResolveRequest, UserMemoryCandidateStatus, UserMemoryDocumentId,
    UserMemoryLearningState, UserMemoryService,
};

impl UserMemoryService {
    pub async fn resolve_candidate(
        &self,
        request: UserMemoryCandidateResolveRequest,
    ) -> Result<UserMemoryCandidateResolutionResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
        let root = self.resolved_root()?;
        let mut state = candidate_store::read_state(root)?;
        require_revision(&state, &request.expected_revision)?;
        let index = find_candidate(&state, &request.candidate_id)?;
        if state.candidates[index].status.is_terminal() {
            return Err(AppCommandError::invalid_input(
                "Terminal candidates cannot be resolved again",
            ));
        }
        match request.resolution {
            UserMemoryCandidateResolution::Confirm { edited_content } => {
                self.confirm_candidate_locked(state, index, edited_content)
                    .await
            }
            resolution => {
                self.apply_resolution(&mut state, index, resolution).await?;
                candidate_store::write_state(root, &state)?;
                Ok(UserMemoryCandidateResolutionResult {
                    candidate: state.candidates[index].clone(),
                    revision: candidate_store::revision(&state)?,
                })
            }
        }
    }

    pub async fn delete_candidate(
        &self,
        request: UserMemoryCandidateDeleteRequest,
    ) -> Result<UserMemoryCandidateDeleteResult, AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        self.recover_pending_transaction().await?;
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
            UserMemoryCandidateResolution::Confirm { .. } => {
                Err(AppCommandError::configuration_invalid(
                    "Memory confirmation requires a transaction",
                ))
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
                    .read_document(UserMemoryDocumentId::Memory)?
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
