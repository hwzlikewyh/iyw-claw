use crate::app_error::AppCommandError;

use super::candidate_references;
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
                self.schedule_index_refresh();
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
        normalize_references_before_delete(&mut state, index)?;
        state.candidates.remove(index);
        candidate_store::write_state(root, &state)?;
        self.schedule_index_refresh();
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
                let candidate_id = state.candidates[index].id.clone();
                let affected = candidate_references::reject_references(state, &candidate_id);
                state.candidates[index].mark_terminal(UserMemoryCandidateStatus::Rejected);
                log_reference_normalization(&candidate_id, "rejected", affected);
                Ok(())
            }
            UserMemoryCandidateResolution::SupersedeByCandidate { candidate_id } => {
                validate_candidate_target(state, index, &candidate_id)?;
                let source_id = state.candidates[index].id.clone();
                let affected =
                    candidate_references::redirect_references(state, &source_id, &candidate_id);
                state.candidates[index].mark_terminal(UserMemoryCandidateStatus::Superseded);
                state.candidates[index].superseded_by_candidate_id = Some(candidate_id);
                log_reference_normalization(&source_id, "candidate", affected);
                Ok(())
            }
            UserMemoryCandidateResolution::SupersedeByMemoryEntry { entry_id } => {
                self.ensure_memory_entry_exists(&entry_id)?;
                supersede_by_memory_entry(state, index, entry_id)
            }
        }
    }

    fn ensure_memory_entry_exists(&self, entry_id: &str) -> Result<(), AppCommandError> {
        validate_memory_entry_id(entry_id)?;
        let marker = format!("<!-- {entry_id} -->");
        if self
            .read_document(UserMemoryDocumentId::Memory)?
            .contains(&marker)
        {
            Ok(())
        } else {
            Err(AppCommandError::not_found(
                "Superseding memory entry was not found",
            ))
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
        || target.is_none_or(|candidate| candidate.status.is_terminal())
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

fn supersede_by_memory_entry(
    state: &mut UserMemoryLearningState,
    index: usize,
    entry_id: String,
) -> Result<(), AppCommandError> {
    let candidate_id = state.candidates[index].id.clone();
    let affected =
        candidate_references::supersede_references_by_memory_entry(state, &candidate_id, &entry_id);
    state.candidates[index].mark_terminal(UserMemoryCandidateStatus::Superseded);
    state.candidates[index].superseded_by_memory_entry_id = Some(entry_id);
    log_reference_normalization(&candidate_id, "memory_entry", affected);
    Ok(())
}

fn normalize_references_before_delete(
    state: &mut UserMemoryLearningState,
    index: usize,
) -> Result<(), AppCommandError> {
    let candidate = state.candidates[index].clone();
    let affected = match candidate.status {
        UserMemoryCandidateStatus::Confirmed => {
            let content = candidate.resolved_content.as_deref().ok_or_else(|| {
                AppCommandError::configuration_invalid("Confirmed candidate content is missing")
            })?;
            let entry_id = candidate
                .confirmed_memory_entry_id
                .as_deref()
                .ok_or_else(|| {
                    AppCommandError::configuration_invalid("Confirmed memory entry is missing")
                })?;
            let propagated_at = chrono::Utc::now().to_rfc3339();
            let confirmed = candidate_references::ConfirmedMemory {
                content,
                entry_id,
                resolved_at: &propagated_at,
            };
            candidate_references::confirm_references(state, &candidate.id, &confirmed)
        }
        _ if candidate_references::references_candidate(state, &candidate.id) => {
            return Err(AppCommandError::configuration_invalid(
                "Terminal candidate has unresolved references",
            ));
        }
        _ => 0,
    };
    log_reference_normalization(&candidate.id, "deleted", affected);
    Ok(())
}

fn log_reference_normalization(candidate_id: &str, outcome: &str, affected: usize) {
    if affected > 0 {
        tracing::info!(
            candidate_id,
            outcome,
            affected,
            "[user-memory] normalized candidate references"
        );
    }
}
