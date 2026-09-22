use chrono::Utc;

use super::maintenance_types::{review_current, MAX_REVIEWS};
use super::{MemoryReviewStatus, UserMemoryService};
use crate::app_error::AppCommandError;

impl UserMemoryService {
    pub(super) async fn reconcile_memory_reviews(&self) -> Result<(), AppCommandError> {
        let (_guard, _file_guard) = self.acquire_locks().await?;
        let policy = self.load_policy().await?;
        if !policy.enabled || !policy.agent_write_enabled {
            return Ok(());
        }
        let mut state = self.read_learning_state()?;
        let settings = self.snapshot_locked(&policy)?;
        let snapshot = super::index_parse::build_index_snapshot(&settings, Some(&state));
        let Some(maintenance) = state.maintenance.as_mut() else {
            return Ok(());
        };
        let now = Utc::now().to_rfc3339();
        let mut closed = 0;
        for review in &mut maintenance.reviews {
            if review.status == MemoryReviewStatus::Pending && !review_current(review, &snapshot) {
                review.status = MemoryReviewStatus::Dismissed;
                review.resolved_at = Some(now.clone());
                closed += 1;
            }
        }
        if closed > 0 {
            self.persist_learning_state(&state).await?;
            tracing::info!(
                closed,
                "[memory-maintenance] obsolete review evidence closed automatically"
            );
        }
        Ok(())
    }
}

pub(super) fn make_review_room(reviews: &mut Vec<super::MemoryReview>) -> bool {
    if reviews.len() < MAX_REVIEWS {
        return true;
    }
    let oldest = reviews
        .iter()
        .enumerate()
        .filter(|(_, review)| review.status != MemoryReviewStatus::Pending)
        .min_by_key(|(_, review)| review.resolved_at.as_deref())
        .map(|(index, _)| index);
    if let Some(index) = oldest {
        reviews.remove(index);
        return true;
    }
    false
}
