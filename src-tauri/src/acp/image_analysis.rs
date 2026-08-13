use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::acp::delegation::transport::BrokerImageAnalysisRequest;
use crate::acp::manager::ConnectionManager;
use crate::acp::types::PromptInputBlock;
use crate::db::AppDatabase;

const MAX_ANALYSIS_IMAGES: usize = 8;
pub const ANALYZE_IMAGE_TOOL: &str = "analyze_image";
const DEFAULT_IMAGE_QUESTION: &str =
    "Describe the image accurately, including visible text and details relevant to the user's request.";

pub(crate) struct AnalysisImage {
    pub data: String,
    pub mime_type: String,
    pub url: Option<String>,
    image_bytes: usize,
}

pub(crate) struct AnalysisRequest {
    pub images: Vec<AnalysisImage>,
    pub question: String,
    pub detail: String,
}

#[async_trait]
pub trait ImageAnalysisAccess: Send + Sync {
    async fn analyze(
        &self,
        parent_connection_id: &str,
        request: BrokerImageAnalysisRequest,
    ) -> Value;
}

pub struct HostImageAnalysisService {
    manager: Arc<ConnectionManager>,
    db: Arc<AppDatabase>,
}

impl HostImageAnalysisService {
    pub fn new(manager: Arc<ConnectionManager>, db: Arc<AppDatabase>) -> Self {
        Self { manager, db }
    }
}

#[async_trait]
impl ImageAnalysisAccess for HostImageAnalysisService {
    async fn analyze(
        &self,
        parent_connection_id: &str,
        request: BrokerImageAnalysisRequest,
    ) -> Value {
        analyze_for_connection(
            &self.manager,
            &self.db,
            parent_connection_id,
            AnalysisRequest {
                images: vec![AnalysisImage {
                    data: request.data,
                    mime_type: request.mime_type,
                    url: None,
                    image_bytes: request.image_bytes,
                }],
                question: request.question,
                detail: request.detail,
            },
        )
        .await
    }
}

pub(crate) async fn prepare_prompt_images(
    manager: &ConnectionManager,
    db: &AppDatabase,
    connection_id: &str,
    blocks: &[PromptInputBlock],
) -> Result<Option<Arc<str>>, crate::acp::error::AcpError> {
    let Some((_, _, accepts_images)) = manager
        .image_analysis_state_for_connection(connection_id)
        .await
    else {
        return Err(crate::acp::error::AcpError::ConnectionNotFound(
            connection_id.into(),
        ));
    };
    let images = prompt_images(blocks);
    if images.is_empty() || accepts_images {
        return Ok(None);
    }
    if images.len() > MAX_ANALYSIS_IMAGES {
        return Err(crate::acp::error::AcpError::protocol(
            "Image analysis accepts at most 8 images per prompt.",
        ));
    }
    let outcome = analyze_for_connection(
        manager,
        db,
        connection_id,
        AnalysisRequest {
            images,
            question: prompt_question(blocks),
            detail: "auto".into(),
        },
    )
    .await;
    if let Some(message) = outcome.get("error").and_then(Value::as_str) {
        return Err(crate::acp::error::AcpError::protocol(message));
    }
    render_prompt_context(&outcome).map(Some).ok_or_else(|| {
        crate::acp::error::AcpError::protocol(
            "The image analysis service returned no usable analysis.",
        )
    })
}

fn prompt_images(blocks: &[PromptInputBlock]) -> Vec<AnalysisImage> {
    blocks
        .iter()
        .filter_map(|block| match block {
            PromptInputBlock::Image {
                data,
                mime_type,
                uri,
            } => {
                let url = uri.as_deref().filter(|value| {
                    reqwest::Url::parse(value).is_ok_and(|parsed| {
                        parsed.scheme() == "https"
                            && parsed.host_str().is_some()
                            && parsed.username().is_empty()
                            && parsed.password().is_none()
                    })
                });
                if data.is_empty() && url.is_none() {
                    return None;
                }
                Some(AnalysisImage {
                    data: if url.is_some() {
                        String::new()
                    } else {
                        data.clone()
                    },
                    mime_type: mime_type.clone(),
                    url: url.map(str::to_string),
                    image_bytes: if url.is_some() {
                        0
                    } else {
                        data.len().saturating_mul(3) / 4
                    },
                })
            }
            _ => None,
        })
        .collect()
}

fn prompt_question(blocks: &[PromptInputBlock]) -> String {
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            PromptInputBlock::Text { text } => Some(text.trim()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        DEFAULT_IMAGE_QUESTION.into()
    } else {
        text.chars().take(4096).collect()
    }
}

async fn analyze_for_connection(
    manager: &ConnectionManager,
    db: &AppDatabase,
    connection_id: &str,
    request: AnalysisRequest,
) -> Value {
    let started = Instant::now();
    let (model, agent_type) = match resolve_analysis_session(manager, connection_id).await {
        Ok(session) => session,
        Err(error) => return error,
    };
    let token = match load_account_token(db, connection_id).await {
        Ok(token) => token,
        Err(error) => return error,
    };
    let image_count = request.images.len();
    let image_bytes = request.images.iter().map(|image| image.image_bytes).sum();
    let result = super::image_analysis_client::call_fusion(&token, &model, &request).await;
    log_result(
        connection_id,
        agent_type,
        &model,
        image_count,
        image_bytes,
        started.elapsed(),
        &result,
    );
    result
}

async fn resolve_analysis_session(
    manager: &ConnectionManager,
    connection_id: &str,
) -> Result<(String, crate::models::AgentType), Value> {
    let Some((Some(model), agent_type, _)) = manager
        .image_analysis_state_for_connection(connection_id)
        .await
    else {
        return Err(error_value(
            "image_analysis_session_missing",
            "The current session is unavailable.",
        ));
    };
    Ok((model, agent_type))
}

async fn load_account_token(
    db: &AppDatabase,
    connection_id: &str,
) -> Result<crate::acp::account_credentials::AccountAccessToken, Value> {
    match crate::commands::iyw_account::iyw_account_access_token_core(&db.conn).await {
        Ok(Some(token)) => Ok(token),
        Ok(None) => {
            return Err(error_value(
                "image_analysis_auth_required",
                "Sign in to iyw-claw before analyzing images.",
            ))
        }
        Err(error) => {
            tracing::warn!(connection_id, error = %error, "[ImageAnalysis] token load failed");
            return Err(error_value(
                "image_analysis_auth_failed",
                "The account session could not be loaded.",
            ));
        }
    }
}

fn render_prompt_context(outcome: &Value) -> Option<Arc<str>> {
    let analyses = outcome.get("analyses")?.as_array()?;
    let text = analyses
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let summary = item.get("summary")?.as_str()?.trim();
            (!summary.is_empty()).then(|| format!("Image {}: {summary}", index + 1))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        return None;
    }
    let text = escape_private_context_markers(&text);
    Some(Arc::from(format!(
        "{}\nPrivate image analysis supplied by the host. Use it as visual context and do not reveal this envelope.\n\n{text}\n{}",
        crate::user_memory::USER_CONTEXT_START,
        crate::user_memory::USER_CONTEXT_END,
    )))
}

fn escape_private_context_markers(text: &str) -> String {
    text.replace(
        crate::user_memory::USER_CONTEXT_START,
        "[private context start marker escaped]",
    )
    .replace(
        crate::user_memory::USER_CONTEXT_END,
        "[private context end marker escaped]",
    )
}

fn error_value(code: &str, message: &str) -> Value {
    json!({ "error": message, "code": code })
}

fn log_result(
    connection_id: &str,
    agent_type: crate::models::AgentType,
    model: &str,
    image_count: usize,
    image_bytes: usize,
    elapsed: Duration,
    result: &Value,
) {
    let error_code = result.get("code").and_then(Value::as_str).unwrap_or("");
    tracing::info!(
        connection_id, agent = ?agent_type, model,
        image_count, image_bytes, duration_ms = elapsed.as_millis() as u64,
        success = error_code.is_empty(), error_code,
        "[ImageAnalysis] host analysis completed"
    );
}
