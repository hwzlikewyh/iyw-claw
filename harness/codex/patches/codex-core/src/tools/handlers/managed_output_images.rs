use codex_protocol::mcp::CallToolResult;
use codex_protocol::models::{DEFAULT_IMAGE_DETAIL, FunctionCallOutputContentItem, ImageReference};
use regex_lite::Regex;
use serde_json::{Value, json};
use std::sync::OnceLock;

use crate::session::turn_context::TurnEnvironment;

#[path = "managed_output_images/paths.rs"]
mod paths;

const DATA_DIR_ENV: &str = "IYW_CLAW_DATA_DIR";
const MAX_OUTPUT_IMAGES: usize = 8;
const MAX_OUTPUT_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMAGE_PATH_MATCHES: usize = 32;
const MAX_STRUCTURED_STRINGS: usize = 256;
const IMAGE_PATH_PATTERN: &str =
    r#"(?i)(?:[a-z]:[\\/]|/|(?:\.?[\\/])?output[\\/])[^"'<>|\r\n]*?\.(?:png|jpe?g|webp|gif)"#;

static IMAGE_PATH_REGEX: OnceLock<Option<Regex>> = OnceLock::new();

#[derive(Clone, Debug)]
pub(crate) struct NativeOutputImage {
    data_url: String,
    size_bytes: usize,
}

#[derive(Debug, Default)]
pub(crate) struct NativeOutputImageCapture {
    images: Vec<NativeOutputImage>,
    omitted: usize,
}

impl NativeOutputImageCapture {
    pub(crate) fn is_empty(&self) -> bool {
        self.images.is_empty() && self.omitted == 0
    }

    pub(crate) fn into_function_output_items(self) -> Vec<FunctionCallOutputContentItem> {
        self.images
            .into_iter()
            .map(|image| FunctionCallOutputContentItem::InputImage {
                image: ImageReference::Inline {
                    image_url: image.data_url,
                },
                detail: Some(DEFAULT_IMAGE_DETAIL),
            })
            .collect()
    }

    pub(crate) fn summary(&self) -> Option<String> {
        (!self.images.is_empty() || self.omitted > 0).then(|| {
            format!(
                "Native session images attached: {}; omitted or unavailable: {}.",
                self.images.len(),
                self.omitted
            )
        })
    }

    pub(crate) fn append_context_to_mcp_result(
        &self,
        result: &mut CallToolResult,
        original: &CallToolResult,
    ) {
        if self.is_empty() {
            return;
        }
        if let Some(structured) = &original.structured_content {
            result.structured_content = None;
            result.content.push(json!({
                "type": "text",
                "text": structured.to_string()
            }));
        }
        if let Some(summary) = self.summary() {
            result
                .content
                .push(json!({"type": "text", "text": summary}));
        }
    }
}

pub(crate) async fn capture_exec_output(
    output: &[u8],
    environment: &TurnEnvironment,
) -> NativeOutputImageCapture {
    let text = String::from_utf8_lossy(output);
    paths::capture_paths(&[text.as_ref()], environment).await
}

pub(crate) async fn capture_mcp_result(
    result: &CallToolResult,
    environment: &TurnEnvironment,
) -> NativeOutputImageCapture {
    let mut sources = result
        .content
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .take(MAX_STRUCTURED_STRINGS)
        .collect::<Vec<_>>();
    if let Some(structured) = &result.structured_content {
        collect_strings(structured, &mut sources);
    }
    paths::capture_paths(&sources, environment).await
}

fn collect_strings<'a>(value: &'a Value, output: &mut Vec<&'a str>) {
    let mut pending = vec![value];
    let mut visited = 0_usize;
    while let Some(value) = pending.pop() {
        if output.len() >= MAX_STRUCTURED_STRINGS {
            return;
        }
        visited += 1;
        let pending_capacity =
            MAX_STRUCTURED_STRINGS.saturating_sub(visited.saturating_add(pending.len()));
        match value {
            Value::String(text) => output.push(text),
            Value::Array(items) => pending.extend(items.iter().take(pending_capacity)),
            Value::Object(items) => pending.extend(items.values().take(pending_capacity)),
            _ => {}
        }
    }
}
