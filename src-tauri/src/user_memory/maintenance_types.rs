use serde::{Deserialize, Serialize};

use super::index_types::{IndexItem, IndexSnapshot};
use crate::app_error::AppCommandError;

pub(super) const MAX_REVIEWS: usize = 128;
pub(super) const REVIEW_BATCH: usize = 60;
pub(super) const MAX_REVIEW_REASON: usize = 500;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReviewStatus {
    Pending,
    Applied,
    Dismissed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryReviewSource {
    pub id: String,
    pub content_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryReview {
    pub id: String,
    pub target: MemoryReviewSource,
    pub content: String,
    pub reason: String,
    pub quote: String,
    pub evidence: Vec<MemoryReviewSource>,
    pub status: MemoryReviewStatus,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryMaintenanceState {
    pub last_attempt_at: Option<String>,
    pub last_completed_at: Option<String>,
    pub last_error_code: Option<String>,
    pub reviewed_digest: Option<String>,
    #[serde(default)]
    pub review_cursor: usize,
    #[serde(default)]
    pub reviews: Vec<MemoryReview>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMaintenanceStatus {
    pub revision: String,
    pub busy: bool,
    pub last_checked_at: Option<String>,
    pub active_count: usize,
    pub expired_count: usize,
    pub model_review: MemoryMaintenanceState,
    pub stale_review_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveMemoryReviewRequest {
    pub id: String,
    pub expected_revision: String,
    pub apply: bool,
}

pub(super) fn source(item: &IndexItem) -> MemoryReviewSource {
    MemoryReviewSource {
        id: item.id.clone(),
        content_digest: item.content_digest.clone(),
    }
}

pub(super) fn review_current(review: &MemoryReview, snapshot: &IndexSnapshot) -> bool {
    let Some(target) = find_source(&review.target, snapshot) else {
        return false;
    };
    review.evidence.iter().all(|reference| {
        find_source(reference, snapshot).is_some_and(|item| {
            item.scope_type == target.scope_type && item.scope_key == target.scope_key
        })
    })
}

fn find_source<'a>(
    source: &MemoryReviewSource,
    snapshot: &'a IndexSnapshot,
) -> Option<&'a IndexItem> {
    snapshot.items.iter().find(|item| {
        item.id == source.id
            && item.content_digest == source.content_digest
            && !item.sensitive
            && super::recall_validity::item_is_current_at(item, &chrono::Utc::now())
    })
}

pub(super) fn validate(state: Option<&MemoryMaintenanceState>) -> Result<(), AppCommandError> {
    let Some(state) = state else { return Ok(()) };
    if state.reviews.len() > MAX_REVIEWS {
        return Err(invalid());
    }
    for time in [&state.last_attempt_at, &state.last_completed_at]
        .into_iter()
        .flatten()
    {
        chrono::DateTime::parse_from_rfc3339(time).map_err(|_| invalid())?;
    }
    let mut ids = std::collections::BTreeSet::new();
    for review in &state.reviews {
        validate_review(review)?;
        if !ids.insert(&review.id) {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_review(review: &MemoryReview) -> Result<(), AppCommandError> {
    if !super::is_lower_hex_string(&review.id, 64)
        || review.reason.trim().is_empty()
        || review.reason.chars().count() > MAX_REVIEW_REASON
        || review.content.chars().count() > super::USER_MEMORY_MAX_CANDIDATE_CHARS
        || review.target.content_digest != super::helpers::hash_parts(&[review.content.as_bytes()])
        || review.quote.trim().is_empty()
        || !review.content.contains(&review.quote)
        || review.evidence.len() > REVIEW_BATCH
        || super::helpers::contains_potential_secret(&review.content)
        || super::helpers::contains_potential_secret(&review.reason)
    {
        return Err(invalid());
    }
    for reference in std::iter::once(&review.target).chain(&review.evidence) {
        if !(super::is_valid_memory_entry_id(&reference.id)
            || super::retention_view::is_document_entry_id(&reference.id))
            || !super::is_lower_hex_string(&reference.content_digest, 64)
        {
            return Err(invalid());
        }
    }
    chrono::DateTime::parse_from_rfc3339(&review.created_at).map_err(|_| invalid())?;
    if (review.status == MemoryReviewStatus::Pending) != review.resolved_at.is_none() {
        return Err(invalid());
    }
    if let Some(time) = &review.resolved_at {
        chrono::DateTime::parse_from_rfc3339(time).map_err(|_| invalid())?;
    }
    Ok(())
}

fn invalid() -> AppCommandError {
    AppCommandError::configuration_invalid("Invalid memory maintenance state")
}
