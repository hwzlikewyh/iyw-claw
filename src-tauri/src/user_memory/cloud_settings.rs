use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::cloud_retrieval::CloudGateway;
use super::UserMemoryService;
use crate::app_error::AppCommandError;

const POLICY_TTL: Duration = Duration::from_secs(60);
// 目录请求失败或重排目录暂不可用时的重试间隔，避免每次召回都重打 Fusion。
const POLICY_RETRY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudRetrievalConfig {
    pub embedding_model: String,
    pub rerank_model: String,
    pub rerank_enabled: bool,
}

pub(super) struct CloudPolicy {
    identity: String,
    expires_at: Instant,
    outcome: Result<CloudRetrievalConfig, AppCommandError>,
}

struct FetchedPolicy {
    config: CloudRetrievalConfig,
    // 重排目录请求失败时为 false，此时只短暂缓存。
    complete: bool,
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
            policy.identity == gateway.identity && policy.expires_at > Instant::now()
        }) {
            return policy.outcome.clone();
        }
        // 目录顺序及可用权限由 Fusion 决定，不再读取或持久化本地模型选择。
        let fetched = fetch_policy(gateway).await;
        let ttl = match &fetched {
            Ok(policy) if policy.complete => POLICY_TTL,
            _ => POLICY_RETRY_INTERVAL,
        };
        let outcome = fetched.map(|policy| policy.config);
        match &outcome {
            Ok(config) => {
                let unchanged = cached.as_ref().is_some_and(|policy| {
                    policy.identity == gateway.identity
                        && policy.outcome.as_ref().ok() == Some(config)
                });
                if !unchanged {
                    tracing::info!(
                        embedding_enabled = !config.embedding_model.is_empty(),
                        rerank_enabled = config.rerank_enabled,
                        "[memory-semantic] Fusion retrieval policy updated"
                    );
                }
            }
            Err(error) => tracing::warn!(
                code = ?error.code,
                retry_after_secs = ttl.as_secs(),
                "[memory-semantic] Fusion retrieval policy unavailable"
            ),
        }
        *cached = Some(CloudPolicy {
            identity: gateway.identity.clone(),
            expires_at: Instant::now() + ttl,
            outcome: outcome.clone(),
        });
        outcome
    }
}

async fn fetch_policy(gateway: &CloudGateway) -> Result<FetchedPolicy, AppCommandError> {
    let (embeddings, rerank) = tokio::join!(gateway.models("embedding"), gateway.models("rerank"));
    let embedding_model = embeddings?
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default();
    let complete = rerank.is_ok();
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
    Ok(FetchedPolicy {
        config: CloudRetrievalConfig {
            embedding_model,
            rerank_enabled: !rerank_model.is_empty(),
            rerank_model,
        },
        complete,
    })
}
