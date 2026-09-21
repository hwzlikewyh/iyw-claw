use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::cloud_retrieval::CloudGateway;
use super::UserMemoryService;
use crate::app_error::AppCommandError;

const POLICY_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudRetrievalConfig {
    pub embedding_model: String,
    pub rerank_model: String,
    pub rerank_enabled: bool,
}

pub(super) struct CloudPolicy {
    identity: String,
    fetched_at: Instant,
    config: CloudRetrievalConfig,
}

impl UserMemoryService {
    pub(super) async fn cloud_retrieval_config(
        &self,
    ) -> Result<CloudRetrievalConfig, AppCommandError> {
        let gateway = self.cloud_gateway().await?;
        self.resolve_cloud_model(&gateway).await
    }

    pub async fn set_cloud_retrieval_config(
        &self,
        _config: CloudRetrievalConfig,
    ) -> Result<(), AppCommandError> {
        Err(AppCommandError::configuration_invalid(
            "记忆检索模型由 Fusion 统一管理",
        ))
    }

    pub(super) async fn resolve_cloud_model(
        &self,
        gateway: &CloudGateway,
    ) -> Result<CloudRetrievalConfig, AppCommandError> {
        let mut cached = self.semantic.cloud_policy.lock().await;
        if let Some(policy) = cached.as_ref().filter(|policy| {
            policy.identity == gateway.identity && policy.fetched_at.elapsed() < POLICY_TTL
        }) {
            return Ok(policy.config.clone());
        }
        // 目录顺序及可用权限由 Fusion 决定，不再读取或持久化本地模型选择。
        let config = fetch_policy(gateway).await?;
        if cached
            .as_ref()
            .is_none_or(|policy| policy.identity != gateway.identity || policy.config != config)
        {
            tracing::info!(
                embedding_enabled = !config.embedding_model.is_empty(),
                rerank_enabled = config.rerank_enabled,
                "[memory-semantic] Fusion retrieval policy updated"
            );
        }
        *cached = Some(CloudPolicy {
            identity: gateway.identity.clone(),
            fetched_at: Instant::now(),
            config: config.clone(),
        });
        Ok(config)
    }
}

async fn fetch_policy(gateway: &CloudGateway) -> Result<CloudRetrievalConfig, AppCommandError> {
    let (embeddings, rerank) = tokio::join!(gateway.models("embedding"), gateway.models("rerank"));
    let embedding_model = embeddings?
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default();
    let rerank_model = match rerank {
        Ok(models) => models
            .first()
            .map(|model| model.id.clone())
            .unwrap_or_default(),
        Err(error) => {
            tracing::warn!(code = ?error.code, "[memory-semantic] rerank policy unavailable");
            String::new()
        }
    };
    Ok(CloudRetrievalConfig {
        embedding_model,
        rerank_enabled: !rerank_model.is_empty(),
        rerank_model,
    })
}
