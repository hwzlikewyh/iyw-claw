use chrono::{SecondsFormat, Utc};

use super::{candidate_store, MemoryReviewStatus, ResolveMemoryReviewRequest, UserMemoryService};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub async fn resolve_memory_review(
        &self,
        request: ResolveMemoryReviewRequest,
    ) -> Result<(), AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        let mut state = self.read_learning_state()?;
        if candidate_store::revision(&state)? != request.expected_revision {
            return Err(super::helpers::conflict(
                "Memory review changed; reload before resolving",
            ));
        }
        let settings = self.snapshot_locked(&policy)?;
        let snapshot = super::index_parse::build_index_snapshot(&settings, Some(&state));
        let review = find_review(&state, &request.id)?.clone();
        if review.status != MemoryReviewStatus::Pending {
            return Err(AppCommandError::invalid_input(
                "Memory review is already resolved",
            ));
        }
        if request.apply {
            apply_review(&mut state, &review, (&snapshot, &policy))?;
        }
        let entry = state
            .maintenance
            .as_mut()
            .expect("review exists")
            .reviews
            .iter_mut()
            .find(|review| review.id == request.id)
            .expect("review exists");
        entry.status = if request.apply {
            MemoryReviewStatus::Applied
        } else {
            MemoryReviewStatus::Dismissed
        };
        entry.resolved_at = Some(Utc::now().to_rfc3339());
        self.persist_learning_state(&state).await?;
        self.schedule_index_refresh();
        tracing::info!(
            applied = request.apply,
            "[memory-maintenance] user resolved review"
        );
        Ok(())
    }
}

fn find_review<'a>(
    state: &'a super::UserMemoryLearningState,
    id: &str,
) -> Result<&'a super::MemoryReview, AppCommandError> {
    state
        .maintenance
        .as_ref()
        .and_then(|maintenance| maintenance.reviews.iter().find(|review| review.id == id))
        .ok_or_else(|| AppCommandError::not_found("Memory review was not found"))
}

fn apply_review(
    state: &mut super::UserMemoryLearningState,
    review: &super::MemoryReview,
    context: (&super::index_types::IndexSnapshot, &super::UserMemoryPolicy),
) -> Result<(), AppCommandError> {
    let (snapshot, policy) = context;
    if !super::maintenance_types::review_current(review, snapshot) {
        return Err(super::helpers::conflict(
            "Memory review evidence changed; review current entries",
        ));
    }
    let item = snapshot
        .items
        .iter()
        .find(|item| item.id == review.target.id)
        .expect("validated target");
    super::helpers::ensure_manual_document_write_allowed(policy, document_for_kind(&item.kind)?)?;
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    state.retention.insert(
        item.id.clone(),
        super::MemoryRetention {
            content_digest: item.content_digest.clone(),
            source_revision: item.source_revision.clone(),
            expires_at: now.clone(),
            reason: review.reason.clone(),
            updated_at: now,
        },
    );
    Ok(())
}

fn document_for_kind(kind: &str) -> Result<super::UserMemoryDocumentId, AppCommandError> {
    match kind {
        "memory" => Ok(super::UserMemoryDocumentId::Memory),
        "profile" => Ok(super::UserMemoryDocumentId::Profile),
        "soul" => Ok(super::UserMemoryDocumentId::Soul),
        _ => Err(AppCommandError::invalid_input(
            "Unsupported memory review target",
        )),
    }
}
