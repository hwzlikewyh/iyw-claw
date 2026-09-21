use serde::{Deserialize, Serialize};

use super::UserMemoryService;
use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;

const CONFIG_KEY: &str = "user_memory.cloud_retrieval_v1";

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudRetrievalConfig {
    pub embedding_model: String,
    pub rerank_model: String,
    pub rerank_enabled: bool,
}

impl UserMemoryService {
    pub(super) async fn cloud_retrieval_config(
        &self,
    ) -> Result<CloudRetrievalConfig, AppCommandError> {
        let value = app_metadata_service::get_value(&self.db, CONFIG_KEY).await?;
        value
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|_| AppCommandError::configuration_invalid("云端记忆配置无效"))
            })
            .unwrap_or_else(|| Ok(CloudRetrievalConfig::default()))
    }

    pub async fn set_cloud_retrieval_config(
        &self,
        config: CloudRetrievalConfig,
    ) -> Result<(), AppCommandError> {
        let current = self.cloud_retrieval_config().await?;
        self.validate_cloud_selection(&config, &current).await?;
        let changed = config.embedding_model != current.embedding_model;
        self.semantic
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let _permit = self
            .semantic
            .task
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppCommandError::task_execution_failed("Memory worker unavailable"))?;
        self.persist_cloud_config(&config).await?;
        self.semantic
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if changed {
            #[cfg(all(feature = "memory-semantic", target_pointer_width = "64"))]
            self.drop_semantic_index().await?;
        }
        drop(_permit);
        self.schedule_semantic_refresh();
        Ok(())
    }

    async fn validate_cloud_selection(
        &self,
        config: &CloudRetrievalConfig,
        current: &CloudRetrievalConfig,
    ) -> Result<(), AppCommandError> {
        let embed_changed =
            !config.embedding_model.is_empty() && config.embedding_model != current.embedding_model;
        let rank_changed = config.rerank_enabled
            && (!current.rerank_enabled || config.rerank_model != current.rerank_model);
        if !embed_changed && !rank_changed {
            return Ok(());
        }
        let models = self.retrieval_models().await?;
        if embed_changed
            && !models
                .embeddings
                .iter()
                .any(|model| model.id == config.embedding_model)
        {
            return Err(AppCommandError::invalid_input("请选择可用的记忆嵌入模型"));
        }
        if rank_changed
            && !models
                .rerank
                .iter()
                .any(|model| model.id == config.rerank_model)
        {
            return Err(AppCommandError::invalid_input("请选择可用的重排模型"));
        }
        Ok(())
    }

    pub(super) async fn persist_cloud_config(
        &self,
        config: &CloudRetrievalConfig,
    ) -> Result<(), AppCommandError> {
        let value = serde_json::to_string(config)
            .map_err(|_| AppCommandError::configuration_invalid("云端记忆配置无效"))?;
        app_metadata_service::upsert_value(&self.db, CONFIG_KEY, &value).await?;
        Ok(())
    }

    pub(super) async fn resolve_cloud_model(
        &self,
        gateway: &super::cloud_retrieval::CloudGateway,
    ) -> Result<CloudRetrievalConfig, AppCommandError> {
        let mut config = self.cloud_retrieval_config().await?;
        if config.embedding_model.is_empty() {
            let models = gateway.models("embedding").await?;
            config.embedding_model = models
                .first()
                .ok_or_else(|| {
                    AppCommandError::configuration_missing(
                        "云端记忆模型尚未配置，将继续使用全文检索",
                    )
                })?
                .id
                .clone();
            self.persist_cloud_config(&config).await?;
        }
        Ok(config)
    }
}
