use serde_json::{json, Value};

use super::{
    super::{result, ImageResult},
    Operation,
};

pub(super) fn native_result(
    op: &Operation,
    mut value: Value,
    task_id: Option<String>,
) -> ImageResult {
    result::redact_credentials(&mut value);
    let task_id = task_id.or_else(|| find_id(&value));
    let mut output = result::result_from_value(op.path, value.clone(), task_id);
    collect_urls(&value, &mut output.images);
    output.images.sort();
    output.images.dedup();
    output.status = status(op, &value, &output);
    output.metadata =
        json!({"result": value, "query": op.poll.map(|path| format!("{}/{path}", op.prefix))});
    output
}

fn status(op: &Operation, value: &Value, output: &ImageResult) -> String {
    let status = normalize_status(&output.status);
    if status != "running" || output.task_id.is_some() {
        return status.to_string();
    }
    let nested = value
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(value);
    if nested.get("status").is_some() || nested.get("process").is_some() {
        return status.to_string();
    }
    if value == &Value::Bool(false) {
        return "failed".to_string();
    }
    if completed_result(op, value, output) {
        return "succeeded".to_string();
    }
    "running".to_string()
}

fn normalize_status(value: &str) -> &str {
    match value {
        "success" | "completed" | "done" => "succeeded",
        "failure" | "error" => "failed",
        "cancelled" => "canceled",
        "pending" => "queued",
        status => status,
    }
}

fn completed_result(op: &Operation, value: &Value, output: &ImageResult) -> bool {
    let data_operation = matches!(
        op.path,
        "extractColor"
            | "saveColor"
            | "detectImageGrid"
            | "classifyCanvasIntent"
            | "buildExtractPrompts"
            | "checkImage"
    );
    !output.images.is_empty() || has_media(value) || (data_operation && !value.is_null())
}

fn find_id(value: &Value) -> Option<String> {
    let nested = value
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(value);
    ["taskId", "task_id"]
        .iter()
        .find_map(|key| match nested.get(*key) {
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            Some(Value::Number(value)) => Some(value.to_string()),
            _ => None,
        })
}

fn has_media(value: &Value) -> bool {
    ["url", "fileUrl", "videoUrl", "modelUrl"]
        .iter()
        .any(|key| {
            value
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|url| result::clean_url(url).is_ok())
        })
}

fn collect_urls(value: &Value, urls: &mut Vec<String>) {
    match value {
        Value::String(value) => {
            if let Ok(url) = result::clean_url(value) {
                urls.push(url);
            }
        }
        Value::Array(values) => values.iter().for_each(|value| collect_urls(value, urls)),
        Value::Object(value) => {
            for key in ["imageUrl", "image", "images", "imageUrls", "result", "data"] {
                if let Some(value) = value.get(key) {
                    collect_urls(value, urls);
                }
            }
        }
        _ => {}
    }
}
