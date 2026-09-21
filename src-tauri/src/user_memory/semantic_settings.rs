use crate::app_error::AppCommandError;

use super::{SemanticStatus, UserMemoryService};

impl UserMemoryService {
    pub(super) async fn semantic_recall_enabled(&self) -> Result<bool, AppCommandError> {
        let policy = self.load_policy_unrecovered().await?;
        Ok(policy.enabled && policy.documents.values().any(|enabled| *enabled))
    }

    pub async fn semantic_settings_status(&self) -> Result<SemanticStatus, AppCommandError> {
        let mut status = self.semantic_status();
        status.recall_enabled = self.semantic_recall_enabled().await?;
        if status.recall_enabled {
            status.config = self.cloud_retrieval_config().await?;
        }
        Ok(status)
    }

    pub async fn set_semantic_recall_enabled(&self, _enabled: bool) -> Result<(), AppCommandError> {
        Err(AppCommandError::configuration_invalid(
            "记忆检索默认启用并由 Fusion 统一管理",
        ))
    }
}
