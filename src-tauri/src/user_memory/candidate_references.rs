use super::{UserMemoryCandidate, UserMemoryCandidateStatus, UserMemoryLearningState};

pub(super) struct ConfirmedMemory<'a> {
    pub(super) content: &'a str,
    pub(super) entry_id: &'a str,
    pub(super) resolved_at: &'a str,
}

/// 保持候选引用为单跳，目标终结时可直接归一化所有上游候选。
pub(super) fn redirect_references(
    state: &mut UserMemoryLearningState,
    source_id: &str,
    target_id: &str,
) -> usize {
    let mut affected = 0;
    for candidate in referencing_candidates_mut(state, source_id) {
        candidate.superseded_by_candidate_id = Some(target_id.to_string());
        affected += 1;
    }
    affected
}

pub(super) fn supersede_references_by_memory_entry(
    state: &mut UserMemoryLearningState,
    candidate_id: &str,
    entry_id: &str,
) -> usize {
    let mut affected = 0;
    for candidate in referencing_candidates_mut(state, candidate_id) {
        candidate.superseded_by_candidate_id = None;
        candidate.superseded_by_memory_entry_id = Some(entry_id.to_string());
        affected += 1;
    }
    affected
}

pub(super) fn confirm_candidate_and_references(
    state: &mut UserMemoryLearningState,
    candidate_id: &str,
    confirmed: &ConfirmedMemory<'_>,
) -> usize {
    if let Some(candidate) = state
        .candidates
        .iter_mut()
        .find(|candidate| candidate.id == candidate_id)
    {
        mark_confirmed(candidate, confirmed);
    }
    confirm_references(state, candidate_id, confirmed)
}

pub(super) fn confirm_references(
    state: &mut UserMemoryLearningState,
    candidate_id: &str,
    confirmed: &ConfirmedMemory<'_>,
) -> usize {
    let mut affected = 0;
    for candidate in referencing_candidates_mut(state, candidate_id) {
        mark_confirmed(candidate, confirmed);
        affected += 1;
    }
    affected
}

pub(super) fn reject_references(state: &mut UserMemoryLearningState, candidate_id: &str) -> usize {
    let mut affected = 0;
    for candidate in referencing_candidates_mut(state, candidate_id) {
        candidate.mark_terminal(UserMemoryCandidateStatus::Rejected);
        candidate.superseded_by_candidate_id = None;
        candidate.superseded_by_memory_entry_id = None;
        affected += 1;
    }
    affected
}

pub(super) fn rewrite_memory_entry_references(
    state: &mut UserMemoryLearningState,
    old_entry_id: &str,
    confirmed: &ConfirmedMemory<'_>,
) -> usize {
    let mut affected = 0;
    for candidate in &mut state.candidates {
        if candidate.confirmed_memory_entry_id.as_deref() == Some(old_entry_id) {
            mark_confirmed(candidate, confirmed);
            affected += 1;
        } else if candidate.superseded_by_memory_entry_id.as_deref() == Some(old_entry_id) {
            candidate.superseded_by_memory_entry_id = Some(confirmed.entry_id.to_string());
            affected += 1;
        }
    }
    affected
}

pub(super) fn preserves_referenced_memory_entries(
    state: &UserMemoryLearningState,
    current: &str,
    next: &str,
) -> bool {
    referenced_memory_entry_ids(state).all(|entry_id| {
        let marker = format!("<!-- {entry_id} -->");
        current
            .lines()
            .find(|line| line.contains(&marker))
            .is_some_and(|line| next.lines().any(|next_line| next_line == line))
    })
}

fn referenced_memory_entry_ids(state: &UserMemoryLearningState) -> impl Iterator<Item = &str> {
    state
        .candidates
        .iter()
        .flat_map(|candidate| {
            [
                candidate.confirmed_memory_entry_id.as_deref(),
                candidate.superseded_by_memory_entry_id.as_deref(),
            ]
        })
        .flatten()
}

pub(super) fn references_candidate(state: &UserMemoryLearningState, candidate_id: &str) -> bool {
    state
        .candidates
        .iter()
        .any(|candidate| candidate.superseded_by_candidate_id.as_deref() == Some(candidate_id))
}

fn referencing_candidates_mut<'a>(
    state: &'a mut UserMemoryLearningState,
    candidate_id: &'a str,
) -> impl Iterator<Item = &'a mut UserMemoryCandidate> {
    state.candidates.iter_mut().filter(move |candidate| {
        candidate.superseded_by_candidate_id.as_deref() == Some(candidate_id)
    })
}

fn mark_confirmed(candidate: &mut UserMemoryCandidate, confirmed: &ConfirmedMemory<'_>) {
    candidate.status = UserMemoryCandidateStatus::Confirmed;
    candidate.resolved_at = Some(confirmed.resolved_at.to_string());
    candidate.resolved_content = Some(confirmed.content.to_string());
    candidate.confirmed_memory_entry_id = Some(confirmed.entry_id.to_string());
    candidate.superseded_by_candidate_id = None;
    candidate.superseded_by_memory_entry_id = None;
}
