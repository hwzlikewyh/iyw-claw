use std::collections::BTreeSet;
use std::path::Path;

use crate::app_error::AppCommandError;

use super::helpers::{hash_parts, memory_entry_id, normalize_candidate};
use super::structured_file;
use super::{
    is_lower_hex_string, is_valid_candidate_id, is_valid_memory_entry_id,
    is_valid_opaque_source_id, UserMemoryCandidate, UserMemoryCandidateSignal,
    UserMemoryCandidateStateSnapshot, UserMemoryCandidateStatus, UserMemoryLearningState,
    USER_MEMORY_CANDIDATE_FILE, USER_MEMORY_CANDIDATE_INVALID_REASON,
    USER_MEMORY_CANDIDATE_SCHEMA_VERSION, USER_MEMORY_MAX_CANDIDATES,
    USER_MEMORY_MAX_OBSERVATION_DETAILS,
};

const USER_MEMORY_MAX_CANDIDATE_STATE_CHARS: usize = 16_777_216;

pub(super) fn read_state(root: &Path) -> Result<UserMemoryLearningState, AppCommandError> {
    let state = structured_file::read_json_optional(
        root,
        USER_MEMORY_CANDIDATE_FILE,
        USER_MEMORY_MAX_CANDIDATE_STATE_CHARS,
    )
    .map_err(wrap_read_error)?
    .unwrap_or_default();
    validate_state(&state)?;
    Ok(state)
}

pub(super) fn write_state(
    root: &Path,
    state: &UserMemoryLearningState,
) -> Result<(), AppCommandError> {
    validate_state(state)?;
    ensure_serialized_size(state)?;
    structured_file::ensure_writable_optional(root, USER_MEMORY_CANDIDATE_FILE)?;
    structured_file::write_json_atomic(root, USER_MEMORY_CANDIDATE_FILE, state)
}

pub(super) fn snapshot(
    state: &UserMemoryLearningState,
) -> Result<UserMemoryCandidateStateSnapshot, AppCommandError> {
    Ok(UserMemoryCandidateStateSnapshot {
        revision: revision(state)?,
        candidates: state.candidates.clone(),
    })
}

pub(super) fn revision(state: &UserMemoryLearningState) -> Result<String, AppCommandError> {
    validate_state(state)?;
    let generation = serde_json::to_vec(state)
        .map_err(|error| invalid_state(format!("serialization failed: {error}")))?;
    Ok(hash_parts(&[&generation]))
}

pub(super) fn deduplication_digest(content: &str, signal: UserMemoryCandidateSignal) -> String {
    hash_parts(&[
        content.to_lowercase().as_bytes(),
        signal.as_str().as_bytes(),
    ])
}

pub(super) fn observation_key(candidate_digest: &str, source_id: &str, turn_nonce: u64) -> String {
    let nonce = turn_nonce.to_le_bytes();
    hash_parts(&[candidate_digest.as_bytes(), source_id.as_bytes(), &nonce])
}

fn validate_state(state: &UserMemoryLearningState) -> Result<(), AppCommandError> {
    if state.schema_version != USER_MEMORY_CANDIDATE_SCHEMA_VERSION {
        return Err(invalid_state("unsupported schema version"));
    }
    if state.candidates.len() > USER_MEMORY_MAX_CANDIDATES {
        return Err(invalid_state("candidate record limit exceeded"));
    }
    let mut ids = BTreeSet::new();
    let mut digests = BTreeSet::new();
    for candidate in &state.candidates {
        validate_candidate(candidate)?;
        if !ids.insert(candidate.id.as_str())
            || !digests.insert(candidate.deduplication_digest.as_str())
        {
            return Err(invalid_state("duplicate candidate identity"));
        }
    }
    validate_supersession_targets(state)
}

fn validate_candidate(candidate: &UserMemoryCandidate) -> Result<(), AppCommandError> {
    let normalized = normalize_candidate(&candidate.content)
        .map_err(|error| invalid_state(error.to_string()))?;
    let digest = deduplication_digest(&candidate.content, candidate.signal);
    if normalized != candidate.content
        || digest != candidate.deduplication_digest
        || !is_valid_candidate_id(&candidate.id)
    {
        return Err(invalid_state("candidate identity is inconsistent"));
    }
    let retained = (candidate.observation_count as usize).min(USER_MEMORY_MAX_OBSERVATION_DETAILS);
    if candidate.observation_count == 0
        || candidate.observations.is_empty()
        || candidate.observations.len() != retained
        || candidate.observation_keys.len() != candidate.observation_count as usize
    {
        return Err(invalid_state("candidate observation bounds are invalid"));
    }
    validate_observations(candidate)?;
    validate_observation_keys(candidate)?;
    validate_status(candidate)
}

fn validate_observation_keys(candidate: &UserMemoryCandidate) -> Result<(), AppCommandError> {
    let mut keys = BTreeSet::new();
    for key in &candidate.observation_keys {
        if !is_lower_hex_string(key, 64) || !keys.insert(key.as_str()) {
            return Err(invalid_state("candidate observation key is invalid"));
        }
    }
    for observation in &candidate.observations {
        let key = observation_key(
            &candidate.deduplication_digest,
            &observation.opaque_source_id,
            observation.turn_nonce,
        );
        if !keys.contains(key.as_str()) {
            return Err(invalid_state("candidate observation key is inconsistent"));
        }
    }
    Ok(())
}

fn validate_observations(candidate: &UserMemoryCandidate) -> Result<(), AppCommandError> {
    let first = parse_timestamp(&candidate.first_observed_at)?;
    let last = parse_timestamp(&candidate.last_observed_at)?;
    if first > last {
        return Err(invalid_state("candidate timestamp order is invalid"));
    }
    let mut identities = BTreeSet::new();
    let mut previous = None;
    for observation in &candidate.observations {
        if !is_valid_opaque_source_id(&observation.opaque_source_id) {
            return Err(invalid_state("candidate source identifier is invalid"));
        }
        let observed_at = parse_timestamp(&observation.observed_at)?;
        if observed_at < first
            || observed_at > last
            || previous.is_some_and(|previous| observed_at < previous)
        {
            return Err(invalid_state("candidate observation time is invalid"));
        }
        previous = Some(observed_at);
        if observation.turn_nonce == 0
            || !identities.insert((&observation.opaque_source_id, observation.turn_nonce))
        {
            return Err(invalid_state("candidate observation identity is invalid"));
        }
    }
    let retained_first = parse_timestamp(&candidate.observations[0].observed_at)?;
    let retained_last = parse_timestamp(&candidate.observations.last().unwrap().observed_at)?;
    if retained_last != last || (candidate.observation_count <= 10 && retained_first != first) {
        Err(invalid_state(
            "candidate observation bounds are inconsistent",
        ))
    } else {
        Ok(())
    }
}

fn validate_status(candidate: &UserMemoryCandidate) -> Result<(), AppCommandError> {
    let unresolved = candidate.resolved_at.is_none()
        && candidate.resolved_content.is_none()
        && candidate.confirmed_memory_entry_id.is_none();
    let no_supersession = candidate.superseded_by_candidate_id.is_none()
        && candidate.superseded_by_memory_entry_id.is_none();
    let valid = match candidate.status {
        UserMemoryCandidateStatus::Tentative => candidate.observation_count == 1,
        UserMemoryCandidateStatus::Emerging => candidate.observation_count == 2,
        UserMemoryCandidateStatus::PendingConfirmation => candidate.observation_count >= 3,
        UserMemoryCandidateStatus::Confirmed => validate_confirmation(candidate).is_ok(),
        UserMemoryCandidateStatus::Rejected => {
            candidate.has_valid_resolution_time()
                && candidate.resolved_content.is_none()
                && candidate.confirmed_memory_entry_id.is_none()
                && no_supersession
        }
        UserMemoryCandidateStatus::Superseded => validate_supersession(candidate),
    };
    let active = matches!(
        candidate.status,
        UserMemoryCandidateStatus::Tentative
            | UserMemoryCandidateStatus::Emerging
            | UserMemoryCandidateStatus::PendingConfirmation
    );
    if valid && (!active || (unresolved && no_supersession)) {
        Ok(())
    } else {
        Err(invalid_state("candidate lifecycle fields are invalid"))
    }
}

fn validate_confirmation(candidate: &UserMemoryCandidate) -> Result<(), AppCommandError> {
    let content = candidate
        .resolved_content
        .as_deref()
        .ok_or_else(|| invalid_state("confirmed content is missing"))?;
    let entry = candidate
        .confirmed_memory_entry_id
        .as_deref()
        .ok_or_else(|| invalid_state("confirmed entry is missing"))?;
    let normalized =
        normalize_candidate(content).map_err(|error| invalid_state(error.to_string()))?;
    if normalized == content
        && candidate.has_valid_resolution_time()
        && is_valid_memory_entry_id(entry)
        && entry == memory_entry_id(content)
        && candidate.superseded_by_candidate_id.is_none()
        && candidate.superseded_by_memory_entry_id.is_none()
    {
        Ok(())
    } else {
        Err(invalid_state("confirmed candidate fields are invalid"))
    }
}

fn validate_supersession(candidate: &UserMemoryCandidate) -> bool {
    let candidate_target = candidate
        .superseded_by_candidate_id
        .as_deref()
        .is_some_and(is_valid_candidate_id);
    let memory_target = candidate
        .superseded_by_memory_entry_id
        .as_deref()
        .is_some_and(is_valid_memory_entry_id);
    candidate_target ^ memory_target
        && candidate.superseded_by_candidate_id.as_deref() != Some(candidate.id.as_str())
        && candidate.has_valid_resolution_time()
        && candidate.resolved_content.is_none()
        && candidate.confirmed_memory_entry_id.is_none()
}

fn validate_supersession_targets(state: &UserMemoryLearningState) -> Result<(), AppCommandError> {
    for candidate in &state.candidates {
        let Some(target_id) = candidate.superseded_by_candidate_id.as_deref() else {
            continue;
        };
        let target = state
            .candidates
            .iter()
            .find(|target| target.id == target_id);
        let valid = target.is_some_and(|target| {
            target.id != candidate.id
                && !matches!(
                    target.status,
                    UserMemoryCandidateStatus::Rejected | UserMemoryCandidateStatus::Superseded
                )
        });
        if !valid {
            return Err(invalid_state("superseded candidate target is invalid"));
        }
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<chrono::DateTime<chrono::FixedOffset>, AppCommandError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|error| invalid_state(format!("candidate timestamp is invalid: {error}")))
}

fn invalid_state(detail: impl Into<String>) -> AppCommandError {
    AppCommandError::configuration_invalid("User memory candidate state is invalid").with_detail(
        format!("{USER_MEMORY_CANDIDATE_INVALID_REASON}: {}", detail.into()),
    )
}

fn wrap_read_error(error: AppCommandError) -> AppCommandError {
    invalid_state(error.detail.unwrap_or(error.message))
}

fn ensure_serialized_size(state: &UserMemoryLearningState) -> Result<(), AppCommandError> {
    let content = serde_json::to_string_pretty(state)
        .map_err(|error| invalid_state(format!("serialization failed: {error}")))?;
    if content.chars().count() <= USER_MEMORY_MAX_CANDIDATE_STATE_CHARS {
        Ok(())
    } else {
        Err(AppCommandError::invalid_input(
            "User memory candidate state capacity reached",
        ))
    }
}
