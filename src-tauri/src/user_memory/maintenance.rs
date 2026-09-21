use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use super::{candidate_store, MemoryMaintenanceStatus, UserMemoryService};
use crate::app_error::AppCommandError;

const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(super) struct MaintenanceRuntime {
    started: AtomicBool,
    pub task: tokio::sync::Mutex<()>,
    checked_at: Mutex<Option<String>>,
}

impl UserMemoryService {
    pub(super) fn start_maintenance_worker(self: &Arc<Self>) {
        if self.maintenance.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(MAINTENANCE_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let Some(service) = weak.upgrade() else { break };
                if let Err(error) = service.run_memory_maintenance().await {
                    tracing::warn!(code = ?error.code, "[memory-maintenance] cycle failed");
                }
            }
        });
    }

    pub async fn run_memory_maintenance(&self) -> Result<(), AppCommandError> {
        let Ok(_guard) = self.maintenance.task.try_lock() else {
            return Ok(());
        };
        self.release_semantic_if_idle().await?;
        self.read_index_source().await?;
        self.reconcile_memory_reviews().await?;
        if let Err(error) = self.retry_authority_export().await {
            tracing::warn!(code = ?error.code, "[memory-maintenance] compatibility export deferred");
        }
        self.schedule_index_refresh();
        *self
            .maintenance
            .checked_at
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(chrono::Utc::now().to_rfc3339());
        self.review_obsolete_memories().await
    }

    pub async fn memory_maintenance_status(
        &self,
    ) -> Result<MemoryMaintenanceStatus, AppCommandError> {
        let busy = self.maintenance.task.try_lock().is_err();
        let checked = self
            .maintenance
            .checked_at
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        self.read_index_source_with(move |settings, learning| {
            let state = learning.unwrap_or_default();
            let snapshot = super::index_parse::build_index_snapshot(&settings, Some(&state));
            let now = chrono::Utc::now();
            let active_count = snapshot
                .items
                .iter()
                .filter(|item| super::recall_validity::item_is_current_at(item, &now))
                .count();
            let model_review = state.maintenance.clone().unwrap_or_default();
            let stale_review_ids = model_review
                .reviews
                .iter()
                .filter(|review| !super::maintenance_types::review_current(review, &snapshot))
                .map(|review| review.id.clone())
                .collect();
            Ok(MemoryMaintenanceStatus {
                revision: candidate_store::revision(&state)?,
                busy,
                last_checked_at: checked,
                active_count,
                expired_count: snapshot.items.len() - active_count,
                model_review,
                stale_review_ids,
            })
        })
        .await?
    }
}
