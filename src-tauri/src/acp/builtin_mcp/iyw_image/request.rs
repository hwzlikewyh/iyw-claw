use std::time::Duration;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::{default_poll, studio, ImageSource, DEFAULT_TIMEOUT_SECONDS, FUSION_TIMEOUT_SECONDS};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImageRequest {
    pub(super) id: Option<String>,
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) prompt: Option<String>,
    #[serde(default)]
    pub(super) images: Vec<ImageSource>,
    #[serde(default)]
    pub(super) parameters: Map<String, Value>,
    pub(super) count: Option<usize>,
    #[serde(default)]
    pub(super) wait: WaitOptions,
    pub(super) delivery: Option<DeliveryOptions>,
    #[serde(default)]
    pub(super) fallbacks: Vec<FallbackRequest>,
}

impl ImageRequest {
    pub(super) fn count(&self) -> usize {
        self.count.unwrap_or(1)
    }

    pub(super) fn candidate(&self, fallback: &FallbackRequest, kind: &str) -> Self {
        Self {
            id: None,
            kind: Some(fallback.kind.clone()),
            prompt: if matches!(kind, "generate" | "edit") {
                self.prompt.clone()
            } else {
                self.parameters
                    .get("prompt")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| self.prompt.clone())
            },
            images: Vec::new(),
            parameters: fallback.parameters.clone(),
            count: None,
            wait: fallback.wait.clone().unwrap_or_else(|| self.wait.clone()),
            delivery: None,
            fallbacks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FallbackRequest {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) parameters: Map<String, Value>,
    pub(super) wait: Option<WaitOptions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImageBatchRequest {
    #[serde(rename = "type")]
    pub(super) _kind: Option<String>,
    pub(super) requests: Vec<ImageRequest>,
    #[serde(default)]
    pub(super) delivery: DeliveryOptions,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ImageToolRequest {
    Batch(ImageBatchRequest),
    Single(ImageRequest),
}

pub(super) fn parse(arguments: Value) -> Result<ImageToolRequest, rmcp::ErrorData> {
    // 先确定调用形态，避免 untagged 枚举吞掉具体参数的反序列化错误。
    let parsed = if arguments.get("requests").is_some() {
        serde_json::from_value::<ImageBatchRequest>(arguments).map(ImageToolRequest::Batch)
    } else {
        serde_json::from_value::<ImageRequest>(arguments).map(ImageToolRequest::Single)
    };
    parsed.map(normalize_parameters).map_err(|error| {
        tracing::warn!(
            error_category = ?error.classify(),
            execution_status = "not_started",
            "[iyw-image] image arguments rejected before submission"
        );
        rmcp::ErrorData::invalid_params(
            error.to_string(),
            Some(serde_json::json!({"execution_status": "not_started"})),
        )
    })
}

fn normalize_parameters(mut request: ImageToolRequest) -> ImageToolRequest {
    let items = match &mut request {
        ImageToolRequest::Single(item) => std::slice::from_mut(item),
        ImageToolRequest::Batch(batch) => batch.requests.as_mut_slice(),
    };
    for item in items {
        // Fusion 可选字段的 null 等同于省略，数量/回退预检也应遵循此规则。
        if matches!(
            item.kind.as_deref(),
            None | Some("" | "auto" | "generate" | "edit")
        ) {
            item.parameters.retain(|_, value| !value.is_null());
        }
        for fallback in &mut item.fallbacks {
            if matches!(fallback.kind.as_str(), "generate" | "edit") {
                fallback.parameters.retain(|_, value| !value.is_null());
            }
        }
    }
    request
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WaitOptions {
    pub(super) timeout_seconds: Option<u64>,
    #[serde(default = "default_poll")]
    pub(super) poll_interval_seconds: f64,
}

impl WaitOptions {
    pub(super) fn platform_timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS))
    }

    pub(super) fn http_timeout(&self, kind: &str) -> Duration {
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
            poll_interval_seconds: default_poll(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeliveryOptions {
    #[serde(default)]
    pub(super) display: bool,
    #[serde(default = "default_false")]
    pub(super) register_artifact: bool,
}

impl Default for DeliveryOptions {
    fn default() -> Self {
        Self {
            display: false,
            register_artifact: false,
        }
    }
}

const fn default_false() -> bool {
    false
}
