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

#[derive(Debug, Deserialize)]
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
