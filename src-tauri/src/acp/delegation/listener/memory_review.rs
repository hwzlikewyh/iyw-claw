use serde::Deserialize;
use serde_json::{json, Value};

use super::{DelegationListener, TokenEntry};

const DEFAULT_REVIEW_LIMIT: usize = 4;
const MAX_REVIEW_LIMIT: usize = 12;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadReviews {
    limit: Option<usize>,
}

impl DelegationListener {
    pub(super) async fn admin_memory_maintenance(&self, input: Value) -> Result<Value, String> {
        let request: ReadReviews =
            serde_json::from_value(input).map_err(|_| "Invalid memory maintenance parameters")?;
        let limit = request.limit.unwrap_or(DEFAULT_REVIEW_LIMIT);
        if !(1..=MAX_REVIEW_LIMIT).contains(&limit) {
            return Err("Memory maintenance limit must be between 1 and 12".into());
        }
        let status = self
            .user_memory
            .memory_maintenance_status()
            .await
            .map_err(|error| error.message)?;
        let reviews = status
            .model_review
            .reviews
            .iter()
            .filter(|review| {
                review.status == crate::user_memory::MemoryReviewStatus::Pending
                    && !status.stale_review_ids.contains(&review.id)
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"revision":status.revision,"pendingCount":reviews.len(),
            "reviews":reviews.into_iter().take(limit).collect::<Vec<_>>(),
            "activeCount":status.active_count,"expiredCount":status.expired_count}),
        )
    }

    pub(super) async fn admin_memory_review(
        &self,
        token: String,
        entry: TokenEntry,
        input: Value,
    ) -> Result<Value, String> {
        Self::ensure_memory_admin_mutation(&entry)?;
        let request: crate::user_memory::ResolveMemoryReviewRequest = serde_json::from_value(input)
            .map_err(|_| "Invalid memory review resolution parameters")?;
        let applied = request.apply;
        let _mutation = self.acquire_memory_admin_mutation(&token, &entry).await?;
        self.user_memory
            .resolve_memory_review_as_agent(request, entry.agent_type)
            .await
            .map_err(|error| error.message)?;
        Ok(json!({"resolved":true,"applied":applied}))
    }
}
