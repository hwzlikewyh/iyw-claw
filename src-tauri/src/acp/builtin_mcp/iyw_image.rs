use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

mod commerce;
mod fission;
mod fusion;
mod input;
mod result;
mod validation;
mod validation_special;

use input::{prepare_images, ImageSource, PreparedImage};

pub(super) const DEFAULT_TIMEOUT_SECONDS: u64 = 180;
pub(super) const DEFAULT_POLL_SECONDS: f64 = 2.0;
pub(super) const MAX_PROMPT_CHARS: usize = 12_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ImageRequest {
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) prompt: Option<String>,
    #[serde(default)]
    images: Vec<ImageSource>,
    #[serde(default)]
    pub(super) parameters: Map<String, Value>,
    #[serde(default)]
    pub(super) wait: WaitOptions,
    #[serde(default)]
    pub(super) delivery: DeliveryOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WaitOptions {
    #[serde(default = "default_timeout")]
    pub(super) timeout_seconds: u64,
    #[serde(default = "default_poll")]
    pub(super) poll_interval_seconds: f64,
}

impl Default for WaitOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            poll_interval_seconds: DEFAULT_POLL_SECONDS,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeliveryOptions {
    #[serde(default = "default_true")]
    pub(super) display: bool,
    #[serde(default = "default_true")]
    pub(super) register_artifact: bool,
}

impl Default for DeliveryOptions {
    fn default() -> Self {
        Self {
            display: true,
            register_artifact: true,
        }
    }
}

pub(super) struct ImageResult {
    pub(super) operation: String,
    pub(super) status: String,
    pub(super) task_id: Option<String>,
    pub(super) images: Vec<String>,
    pub(super) metadata: Value,
}

pub(super) async fn generate(
    service: &IywGatewayService,
    authority: &SessionContext,
    arguments: Value,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let request: ImageRequest = serde_json::from_value(arguments)
        .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
    validate_request(&request)?;
    let images = prepare_images(service, authority.cwd(), &request.images).await?;
    let kind = select_kind(request.kind.as_deref(), &request.prompt, images.len());
    let mut result = execute_kind(service, &request, &kind, &images).await?;
    let delivery = deliver_result(service, authority, &request, &result).await;
    result.metadata = json!({
        "operation": result.operation,
        "status": result.status,
        "task_id": result.task_id,
        "images": result.images,
        "metadata": result.metadata,
        "delivery": delivery,
    });
    Ok(rmcp::model::CallToolResult::structured(result.metadata))
}

fn validate_request(request: &ImageRequest) -> Result<(), rmcp::ErrorData> {
    if request
        .prompt
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_PROMPT_CHARS)
    {
        return Err(invalid("prompt exceeds 12000 characters"));
    }
    if request.images.len() > 10 {
        return Err(invalid("images accepts at most 10 items"));
    }
    if request.wait.timeout_seconds > 600 {
        return Err(invalid("timeoutSeconds must be between 0 and 600"));
    }
    if !(0.0 < request.wait.poll_interval_seconds && request.wait.poll_interval_seconds <= 30.0) {
        return Err(invalid("pollIntervalSeconds must be between 0 and 30"));
    }
    Ok(())
}

fn select_kind(kind: Option<&str>, prompt: &Option<String>, image_count: usize) -> String {
    if let Some(kind) = kind.filter(|value| !value.trim().is_empty() && *value != "auto") {
        return kind.to_string();
    }
    let text = prompt.as_deref().unwrap_or_default().to_ascii_lowercase();
    if image_count == 0 {
        "generate".to_string()
    } else if image_count == 1
        && ["系列", "延展", "同系列", "extend", "series"]
            .iter()
            .any(|term| text.contains(term))
    {
        "extend".to_string()
    } else if image_count == 1 {
        "variation".to_string()
    } else {
        "mix".to_string()
    }
}

async fn execute_kind(
    service: &IywGatewayService,
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<ImageResult, rmcp::ErrorData> {
    match kind {
        "generate" => fusion::generate(service, request).await,
        "edit" => fusion::edit(service, request, images).await,
        "fission" => fission::generate(service, request).await,
        _ => commerce::generate(service, request, kind, images).await,
    }
}

async fn deliver_result(
    service: &IywGatewayService,
    authority: &SessionContext,
    request: &ImageRequest,
    result: &ImageResult,
) -> Value {
    if result.status == "succeeded" && !result.images.is_empty() {
        service
            .deliver(
                authority,
                &result.images,
                request.delivery.display,
                request.delivery.register_artifact,
            )
            .await
    } else {
        json!({"displayed": [], "artifact": null})
    }
}

pub(super) const fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

pub(super) const fn default_poll() -> f64 {
    DEFAULT_POLL_SECONDS
}

const fn default_true() -> bool {
    true
}

pub(super) fn invalid(message: impl Into<String>) -> rmcp::ErrorData {
    rmcp::ErrorData::invalid_params(message.into(), None)
}

pub(super) fn required_prompt(prompt: Option<&str>) -> Result<String, rmcp::ErrorData> {
    let prompt = prompt.unwrap_or_default().trim();
    if prompt.is_empty() {
        return Err(invalid("prompt is required"));
    }
    Ok(prompt.to_string())
}
