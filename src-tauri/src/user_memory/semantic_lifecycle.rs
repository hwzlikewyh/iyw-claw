use std::time::{Duration, Instant};

use super::{semantic_model, UserMemoryService};
use crate::app_error::AppCommandError;

const SEMANTIC_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

impl UserMemoryService {
    pub(super) fn touch_semantic_activity(&self) {
        *self
            .semantic
            .last_activity
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Instant::now();
    }

    pub(super) fn semantic_recently_used(&self) -> bool {
        self.semantic
            .last_activity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .elapsed()
            < SEMANTIC_IDLE_TIMEOUT
    }

    pub(super) async fn release_semantic_if_idle(&self) -> Result<(), AppCommandError> {
        if self.semantic_recently_used() {
            return Ok(());
        }
        let Ok(permit) = self.semantic.task.clone().try_acquire_owned() else {
            return Ok(());
        };
        if self.semantic_recently_used() {
            return Ok(());
        }
        if self
            .semantic
            .index
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_none()
        {
            return Ok(());
        }
        self.drop_semantic_index().await?;
        tracing::info!("[memory-semantic] idle runtime released");
        drop(permit);
        Ok(())
    }

    pub(super) async fn drop_semantic_index(&self) -> Result<(), AppCommandError> {
        self.semantic.query_cache.clear();
        let runtime = self.semantic.clone();
        tokio::task::spawn_blocking(move || {
            *runtime
                .index
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
        })
        .await
        .map_err(semantic_model::model_error)?;
        self.mark_semantic_released();
        Ok(())
    }
}
