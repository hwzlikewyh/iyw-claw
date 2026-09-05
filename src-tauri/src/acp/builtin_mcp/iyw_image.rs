use serde::Deserialize;
use serde_json::{Map, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

mod batch;
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
    pub(super) id: Option<String>,
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) prompt: Option<String>,
    #[serde(default)]
    images: Vec<ImageSource>,
    #[serde(default)]
    pub(super) parameters: Map<String, Value>,
    pub(super) count: Option<usize>,
    #[serde(default)]
    pub(super) wait: WaitOptions,
    pub(super) delivery: Option<DeliveryOptions>,
}

impl ImageRequest {
    pub(super) fn count(&self) -> usize {
        self.count.unwrap_or(1)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ImageBatchRequest {
    pub(super) requests: Vec<ImageRequest>,
    #[serde(default)]
    pub(super) delivery: DeliveryOptions,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ImageToolRequest {
    Batch(ImageBatchRequest),
    Single(ImageRequest),
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeliveryOptions {
    #[serde(default)]
    pub(super) display: bool,
    #[serde(default = "default_true")]
    pub(super) register_artifact: bool,
}

impl Default for DeliveryOptions {
    fn default() -> Self {
        Self {
            display: false,
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

struct ImageExecution<'a> {
    request: &'a ImageRequest,
    kind: &'a str,
    images: &'a [PreparedImage],
}

pub(super) async fn generate(
    service: &IywGatewayService,
    authority: &SessionContext,
    arguments: Value,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let request: ImageToolRequest = serde_json::from_value(arguments)
        .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
    match request {
        ImageToolRequest::Batch(request) => batch::execute_batch(service, authority, request).await,
        ImageToolRequest::Single(request) => {
            batch::execute_single(service, authority, request).await
        }
    }
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
    if request.count.is_some() && !(1..=4).contains(&request.count()) {
        return Err(invalid("count must be between 1 and 4"));
    }
    if request.count.is_some()
        && (request.parameters.contains_key("n") || request.parameters.contains_key("batchSize"))
    {
        return Err(invalid(
            "count cannot be combined with parameters.n or parameters.batchSize",
        ));
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

fn preflight_kind(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    match kind {
        "generate" => required_prompt(request.prompt.as_deref()).map(|_| ()),
        "edit" => {
            required_prompt(request.prompt.as_deref())?;
            if images.is_empty() {
                return Err(invalid("edit requires at least one image"));
            }
            Ok(())
        }
        "fission" => fission::validate_request(request),
        _ => commerce::validate_request(request, kind, images),
    }
}

async fn execute_kind(
    service: &IywGatewayService,
    execution: ImageExecution<'_>,
) -> Result<ImageResult, rmcp::ErrorData> {
    match execution.kind {
        "generate" => fusion::generate(service, execution.request).await,
        "edit" => fusion::edit(service, execution.request, execution.images).await,
        "fission" => fission::generate(service, execution.request).await,
        _ => commerce::generate(service, execution).await,
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
