use crate::session::step_context::StepContext;
use codex_file_system::CreateDirectoryOptions;
use codex_file_system::EnvironmentAccess;
use codex_file_system::RemoveOptions;
use codex_file_system::WriteFileOptions;
use codex_protocol::mcp::CallToolResult;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_path_uri::PathUri;
use serde_json::Value;
use serde_json::json;
use std::sync::OnceLock;
use uuid::Uuid;

mod bounded_json;

const PREVIEW_THRESHOLD_BYTES: usize = 16 * 1024;
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PREVIEW_BYTES: usize = 8 * 1024;
const PREVIEW_DEPTH: usize = 4;
const PREVIEW_ITEMS: usize = 3;
const WRAPPER_TOKEN_RESERVE: usize = 256;
const MAX_UTF8_CHAR_BYTES: usize = 4;
const MAX_CONCURRENT_PREVIEWS: usize = 2;

struct SavedResult {
    file: PathUri,
    environment_id: String,
    original_bytes: usize,
}

pub(crate) async fn prepare(
    result: &CallToolResult,
    step: &StepContext,
    policy: TruncationPolicy,
) -> Option<CallToolResult> {
    if !result.content.iter().any(|item| large_text(item).is_some()) {
        return None;
    }
    static PREVIEWS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    let _permit = PREVIEWS
        .get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_PREVIEWS))
        .acquire()
        .await
        .ok()?;
    let raw = bounded_json::encode(result, MAX_CAPTURE_BYTES)?;
    let (environment, access) = step.environments().into_iter().next()?;
    let directory = environment.cwd().join(".xinghe/tool-results").ok()?;
    let file = directory.join(&format!("{}.json", Uuid::new_v4())).ok()?;
    let saved = SavedResult {
        file,
        environment_id: environment.selection.environment_id.clone(),
        original_bytes: raw.len(),
    };
    let presented = model_result(result, &saved, policy)?;
    preserve(&access, (&directory, &saved.file), raw).await?;
    Some(presented)
}

async fn preserve(
    access: &impl EnvironmentAccess,
    paths: (&PathUri, &PathUri),
    raw: Vec<u8>,
) -> Option<()> {
    let options = CreateDirectoryOptions {
        recursive: true,
        follow_symlinks: false,
    };
    access.create_directory(paths.0, options).await.ok()?;
    if let Err(error) = access
        .write_file(
            paths.1,
            raw,
            WriteFileOptions {
                follow_symlinks: false,
            },
        )
        .await
    {
        let _ = access
            .remove(
                paths.1,
                RemoveOptions {
                    recursive: false,
                    force: true,
                    follow_symlinks: false,
                },
            )
            .await;
        tracing::warn!(error = %error, "failed to preserve oversized MCP output; retaining normal output");
        return None;
    }
    Some(())
}

fn large_text(item: &Value) -> Option<&str> {
    if item.get("type")?.as_str()? != "text" {
        return None;
    }
    let text = item.get("text")?.as_str()?;
    (text.len() > PREVIEW_THRESHOLD_BYTES).then_some(text)
}

fn model_result(
    result: &CallToolResult,
    saved: &SavedResult,
    policy: TruncationPolicy,
) -> Option<CallToolResult> {
    let budget = policy.token_budget().saturating_sub(WRAPPER_TOKEN_RESERVE);
    let count = result
        .content
        .iter()
        .filter(|item| large_text(item).is_some())
        .count()
        .max(1);
    let mut content = Vec::with_capacity(result.content.len());
    for (index, item) in result.content.iter().enumerate() {
        content.push(match large_text(item) {
            Some(text) => preview_content(text, saved, (index, count, budget))?,
            None => item.clone(),
        });
    }
    let presented = CallToolResult {
        content,
        structured_content: None,
        is_error: result.is_error,
        meta: result.meta.clone(),
    };
    let text_tokens: usize = presented
        .content
        .iter()
        .filter_map(|item| item.get("text")?.as_str())
        .map(approx_token_count)
        .sum();
    if text_tokens > budget {
        return None;
    }
    Some(presented)
}

fn preview_content(
    text: &str,
    saved: &SavedResult,
    (index, count, budget): (usize, usize, usize),
) -> Option<Value> {
    let candidate =
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_owned()));
    let bounded = preview(&candidate, MAX_PREVIEW_BYTES / count, PREVIEW_DEPTH);
    let mut value = preview_entry(bounded, saved, index);
    let mut encoded = bounded_json::encode(&value, MAX_PREVIEW_BYTES);
    if encoded.as_ref().is_none_or(|raw| {
        std::str::from_utf8(raw).map_or(true, |text| approx_token_count(text) > budget / count)
    }) {
        value.as_object_mut()?.remove("preview");
        encoded = bounded_json::encode(&value, MAX_PREVIEW_BYTES);
    }
    Some(json!({"type":"text", "text":String::from_utf8(encoded?).ok()?}))
}

fn preview_entry(preview: Value, saved: &SavedResult, index: usize) -> Value {
    json!({
        "truncated": true, "preview_is_partial": true,
        "result_file": saved.file.inferred_native_path_string(),
        "environment_id": saved.environment_id, "content_index": index,
        "original_bytes": saved.original_bytes, "preview": preview,
        "next_action": "Read the saved MCP result JSON with filesystem tools for omitted fields. Missing preview fields are unknown, never zero or evidence of absence."
    })
}

fn preview(value: &Value, budget: usize, depth: usize) -> Value {
    if bounded_json::fits(value, budget) {
        return value.clone();
    }
    if depth == 0 || budget == 0 {
        return json!({"omitted": true});
    }
    match value {
        Value::Array(items) => {
            let count = items.len().min(PREVIEW_ITEMS).max(1);
            Value::Array(
                items
                    .iter()
                    .take(count)
                    .map(|item| preview(item, budget / count, depth - 1))
                    .collect(),
            )
        }
        Value::Object(fields) => {
            let count = fields.len().min(PREVIEW_ITEMS).max(1);
            Value::Object(
                fields
                    .iter()
                    .take(PREVIEW_ITEMS)
                    .map(|(key, value)| (key.clone(), preview(value, budget / count, depth - 1)))
                    .collect(),
            )
        }
        Value::String(text) => {
            Value::String(text.chars().take(budget / MAX_UTF8_CHAR_BYTES).collect())
        }
        _ => value.clone(),
    }
}
