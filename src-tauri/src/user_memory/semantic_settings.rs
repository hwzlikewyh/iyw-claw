use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

use super::{SemanticStatus, UserMemoryService};

const RECALL_ENABLED_KEY: &str = "user_memory.semantic_recall_enabled_v1";

impl UserMemoryService {
    pub(super) async fn semantic_recall_enabled(&self) -> Result<bool, AppCommandError> {
        Ok(
            app_metadata_service::get_value(&self.db, RECALL_ENABLED_KEY)
                .await
                .map_err(AppCommandError::from)?
                .as_deref()
                == Some("true"),
        )
    }

    pub async fn semantic_settings_status(&self) -> Result<SemanticStatus, AppCommandError> {
        let mut status = self.semantic_status();
        status.recall_enabled = self.semantic_recall_enabled().await?;
        let retry = super::managed_model_state::load_retry(&self.db).await?;
        status.retry_pending = retry.is_some();
        status.next_retry_at = retry.map(|state| state.next_attempt_at.to_rfc3339());
        Ok(status)
    }

    pub async fn set_semantic_recall_enabled(&self, enabled: bool) -> Result<(), AppCommandError> {
        if enabled && !self.semantic_status().supported {
            return Err(AppCommandError::configuration_invalid(
                "当前版本暂不支持本地语义检索",
            ));
        }
        app_metadata_service::upsert_value(
            &self.db,
            RECALL_ENABLED_KEY,
            if enabled { "true" } else { "false" },
        )
        .await
        .map_err(AppCommandError::from)?;
        if enabled {
            #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
            self.touch_semantic_activity();
            self.schedule_semantic_refresh();
            self.schedule_missing_model_repair();
        } else {
            self.release_semantic_runtime().await?;
        }
        tracing::info!(enabled, "[memory-semantic] Agent recall setting updated");
        Ok(())
    }
}
