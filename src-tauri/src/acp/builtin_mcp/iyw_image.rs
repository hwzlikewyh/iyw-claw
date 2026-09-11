use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Map, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

mod batch;
mod commerce;
mod fission;
mod fusion;
mod input;
mod native;
mod result;
mod studio;
mod validation;
mod validation_special;

use input::{prepare_images, upload_images, ImageSource, PreparedImage};

pub(super) const DEFAULT_TIMEOUT_SECONDS: u64 = 600;
pub(super) const FUSION_TIMEOUT_SECONDS: u64 = 300;
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
    pub(super) timeout_seconds: Option<u64>,
    #[serde(default = "default_poll")]
    pub(super) poll_interval_seconds: f64,
}

impl WaitOptions {
    pub(super) fn platform_timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS))
    }

    fn http_timeout(&self, kind: &str) -> Duration {
        let default = if matches!(kind, "generate" | "edit") {
            FUSION_TIMEOUT_SECONDS
        } else {
            studio::http_timeout(kind).unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        };
        Duration::from_secs(
            self.timeout_seconds
                .filter(|seconds| *seconds > 0)
                .unwrap_or(default),
        )
    }
}

impl Default for WaitOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: None,
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
    if request.wait.timeout_seconds.is_some_and(|seconds| {
        Instant::now()
            .checked_add(Duration::from_secs(seconds))
            .is_none()
    }) {
        return Err(invalid("timeoutSeconds exceeds the supported timer range"));
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
        "fission".to_string()
    } else if image_count == 1
        && ["系列", "延展", "延伸", "延申", "extend", "series"]
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
    if matches!(kind, "generate" | "fission") && !images.is_empty() {
        return Err(invalid(
            "generate and fission do not accept source images; choose variation, extend, mix, or a specialized IYW platform image tool",
        ));
    }
    if matches!(kind, "variation" | "extend") && images.len() > 1 {
        return Err(invalid(
            "variation and extend accept one source image; use mix to combine references",
        ));
    }
    match kind {
        _ if studio::supports(kind) => studio::validate_request(request, kind, images),
        _ if native::operation(kind).is_some() => native::validate_request(request, kind, images),
        "generate" | "edit" => {
            required_prompt(request.prompt.as_deref())?;
            if kind == "edit" && images.is_empty() {
                return Err(invalid("edit requires at least one image"));
            }
            fusion::requested_model(&request.parameters)?;
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
    let mut service = service.clone();
    let timeout = execution.request.wait.http_timeout(execution.kind);
    service.image_timeout = Some(timeout);
    tracing::info!(
        image_type = execution.kind,
        timeout_seconds = timeout.as_secs(),
        custom_timeout_seconds = execution.request.wait.timeout_seconds,
        "[iyw-image] image execution timeout selected"
    );
    match execution.kind {
        kind if studio::supports(kind) => studio::generate(&service, execution).await,
        kind if native::operation(kind).is_some() => native::generate(&service, execution).await,
        "generate" => fusion::generate(&service, execution.request).await,
        "edit" => fusion::edit(&service, execution.request, execution.images).await,
        "fission" => fission::generate(&service, execution.request).await,
        _ => commerce::generate(&service, execution).await,
    }
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
        return Err(missing_prompt());
    }
    Ok(prompt.to_string())
}

pub(super) fn missing_prompt() -> rmcp::ErrorData {
    tracing::warn!(
        field = "prompt",
        execution_status = "not_started",
        "[iyw-image] prompt validation rejected the request before generation"
    );
    rmcp::ErrorData::invalid_params(
        "prompt is required; supply non-blank prompt in the tool arguments or each requests item",
        Some(serde_json::json!({
            "code": "image_prompt_required", "field": "prompt", "execution_status": "not_started",
            "guidance": "Put the user's image description in prompt directly, without an extra arguments wrapper. Assistant text is not a tool argument. Correct the same operation once; do not replay empty arguments or switch tools/types. Stop if the correction fails."
        })),
    )
}
