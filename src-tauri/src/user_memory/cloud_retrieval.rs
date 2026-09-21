use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{Client, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;

use super::UserMemoryService;
use crate::app_error::AppCommandError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_VECTOR_DIMENSIONS: usize = 65536;

pub(super) struct CloudGateway {
    base: String,
    token: crate::acp::account_credentials::AccountAccessToken,
    pub identity: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalModelOption {
    pub id: String,
    #[serde(alias = "display_name")]
    pub display_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalModels {
    pub embeddings: Vec<RetrievalModelOption>,
    pub rerank: Vec<RetrievalModelOption>,
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<RetrievalModelOption>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
    embedding_space: String,
}

#[derive(Clone)]
pub(super) struct CloudVector {
    pub values: Vec<f32>,
    pub space: String,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct RerankResponse {
    results: Vec<RerankItem>,
}

#[derive(Deserialize)]
struct RerankItem {
    index: usize,
    relevance_score: f64,
}

impl UserMemoryService {
    pub(super) async fn cloud_gateway(&self) -> Result<CloudGateway, AppCommandError> {
        let (token, scope) =
            crate::commands::iyw_account::iyw_account_memory_credentials_core(&self.db)
                .await?
                .ok_or_else(|| {
                    AppCommandError::authentication_failed("请先登录后使用云端记忆检索")
                })?;
        let base = crate::acp::provider_overlay::model_gateway_base_url_for(
            crate::models::agent::AgentType::Codex,
        );
        let url =
            crate::chat_channel::natural_router_config::normalize_chat_completions_url(&base)?;
        let base = url.trim_end_matches("/chat/completions").to_string();
        let identity = super::helpers::hash_parts(&[base.as_bytes(), scope.as_bytes()]);
        Ok(CloudGateway {
            base,
            token,
            identity,
        })
    }

    pub async fn retrieval_models(&self) -> Result<RetrievalModels, AppCommandError> {
        let gateway = self.cloud_gateway().await?;
        let (embeddings, rerank) =
            tokio::try_join!(gateway.models("embedding"), gateway.models("rerank"))?;
        Ok(RetrievalModels { embeddings, rerank })
    }
}

impl CloudGateway {
    pub async fn models(&self, kind: &str) -> Result<Vec<RetrievalModelOption>, AppCommandError> {
        let response = client()?
            .get(format!("{}/models", self.base))
            .header("token", self.token.expose())
            .query(&[("model_type", kind)])
            .send()
            .await
            .map_err(network_error)?;
        let list: ModelList = decode_response(response).await?;
        Ok(list
            .data
            .into_iter()
            .filter(|model| !model.id.trim().is_empty() && !model.display_name.trim().is_empty())
            .collect())
    }

    pub async fn embed(&self, model: &str, text: &str) -> Result<CloudVector, AppCommandError> {
        let response: EmbeddingResponse = self
            .post("embeddings", json!({"model":model,"input":text}))
            .await?;
        let mut items = response.data;
        if items.len() != 1
            || items[0].index != 0
            || response.embedding_space.len() != 64
            || !response
                .embedding_space
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid_response());
        }
        let vector = items.pop().expect("one embedding").embedding;
        let norm: f64 = vector.iter().map(|value| f64::from(*value).powi(2)).sum();
        if vector.is_empty()
            || vector.len() > MAX_VECTOR_DIMENSIONS
            || !norm.is_finite()
            || norm <= 0.0
        {
            return Err(invalid_response());
        }
        Ok(CloudVector {
            values: vector,
            space: response.embedding_space,
        })
    }

    pub async fn rerank(
        &self,
        model: &str,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<usize>, AppCommandError> {
        let response: RerankResponse = self
            .post(
                "rerank",
                json!({
                    "model":model,"query":query,"documents":documents,"top_n":documents.len()
                }),
            )
            .await?;
        if response.results.len() != documents.len() {
            return Err(invalid_response());
        }
        let mut seen = std::collections::BTreeSet::new();
        for result in &response.results {
            if result.index >= documents.len()
                || !result.relevance_score.is_finite()
                || !seen.insert(result.index)
            {
                return Err(invalid_response());
            }
        }
        let mut results = response.results;
        results.sort_by(|left, right| right.relevance_score.total_cmp(&left.relevance_score));
        Ok(results.into_iter().map(|result| result.index).collect())
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, AppCommandError> {
        let response = client()?
            .post(format!("{}/{path}", self.base))
            .header("token", self.token.expose())
            .json(&body)
            .send()
            .await
            .map_err(network_error)?;
        decode_response(response).await
    }
}

fn client() -> Result<&'static Client, AppCommandError> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .pool_max_idle_per_host(2)
                .build()
                .map_err(|_| "HTTP client unavailable".into())
        })
        .as_ref()
        .map_err(|_| AppCommandError::network("云端记忆连接暂时不可用"))
}

async fn decode_response<T: DeserializeOwned>(
    mut response: Response,
) -> Result<T, AppCommandError> {
    let status = response.status();
    if !status.is_success() {
        return Err(
            AppCommandError::network("云端记忆服务暂时不可用，将继续使用已有检索结果")
                .with_detail(format!("http_status={}", status.as_u16())),
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(invalid_response());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid_response())
}

fn network_error(error: reqwest::Error) -> AppCommandError {
    AppCommandError::network("云端记忆连接暂时不可用").with_detail(format!(
        "timeout={},connect={}",
        error.is_timeout(),
        error.is_connect()
    ))
}

fn invalid_response() -> AppCommandError {
    AppCommandError::network("云端记忆服务返回了无效的检索结果")
}
