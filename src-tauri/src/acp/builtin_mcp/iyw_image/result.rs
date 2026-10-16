use serde_json::Value;

use super::ImageResult;

pub(super) const TERMINAL: [&str; 3] = ["succeeded", "failed", "canceled"];

pub(super) fn preserve_images(previous: &[String], images: &mut Vec<String>) {
    let mut combined = previous.to_vec();
    for image in images.drain(..) {
        if !combined.contains(&image) {
            combined.push(image);
        }
    }
    *images = combined;
}

pub(super) fn redact_credentials(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| {
                !matches!(
                    key.to_ascii_lowercase().as_str(),
                    "token"
                        | "tokeninfo"
                        | "authorization"
                        | "cookie"
                        | "refreshtoken"
                        | "accesstoken"
                        | "access_token"
                        | "securitytoken"
                        | "accesskeyid"
                        | "accesskeysecret"
                        | "ststoken"
                        | "secret"
                        | "password"
                )
            });
            object.values_mut().for_each(redact_credentials);
        }
        Value::Array(items) => items.iter_mut().for_each(redact_credentials),
        _ => {}
    }
}

pub(super) fn result_from_value(
    operation: &str,
    value: Value,
    task_id: Option<String>,
) -> ImageResult {
    let status = normalize_status(&value);
    let task_id = task_id.or_else(|| find_task_id(&value));
    ImageResult {
        operation: operation.to_string(),
        status: status.clone(),
        task_id: task_id.clone(),
        images: extract_urls(&value),
        metadata: serde_json::json!({
            "status": status,
            "task_id": task_id,
        }),
    }
}

pub(super) fn extract_urls(value: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    collect_result_urls(value, &mut urls);
    urls.sort();
    urls.dedup();
    urls
}

fn collect_result_urls(value: &Value, urls: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    for key in ["images", "imageUrls", "resultImages"] {
        if let Some(images) = object.get(key) {
            collect_image_values(images, urls);
        }
    }
    for key in ["data", "result", "runs", "tasks", "modules"] {
        if let Some(nested) = object.get(key) {
            collect_nested_results(nested, urls);
        }
    }
}

fn collect_nested_results(value: &Value, urls: &mut Vec<String>) {
    match value {
        Value::Object(_) => collect_result_urls(value, urls),
        Value::Array(values) => values
            .iter()
            .for_each(|item| collect_result_urls(item, urls)),
        _ => {}
    }
}

fn collect_image_values(value: &Value, urls: &mut Vec<String>) {
    match value {
        Value::String(value) => push_url(value, urls),
        Value::Array(values) => values
            .iter()
            .for_each(|item| collect_image_values(item, urls)),
        Value::Object(value) => {
            for key in ["url", "image", "imageUrl"] {
                if let Some(url) = value.get(key).and_then(Value::as_str) {
                    push_url(url, urls);
                }
            }
        }
        _ => {}
    }
}

fn push_url(value: &str, urls: &mut Vec<String>) {
    if let Ok(url) = clean_url(value) {
        urls.push(url);
    }
}

pub(super) fn normalize_status(value: &Value) -> String {
    let value = value
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(value);
    if let Some(status) = value.get("status").and_then(Value::as_str) {
        return status.to_ascii_lowercase();
    }
    match value.get("process").and_then(Value::as_i64) {
        Some(10) => "succeeded",
        Some(20 | 30) => "failed",
        Some(0 | 1) => "queued",
        _ => "running",
    }
    .to_string()
}

pub(super) fn find_task_id(value: &Value) -> Option<String> {
    let nested = value
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(value);
    ["taskId", "task_id"]
        .iter()
        .find_map(|key| nested.get(*key).and_then(Value::as_str).map(str::to_string))
}

pub(super) fn clean_url(value: &str) -> Result<String, rmcp::ErrorData> {
    let mut url = reqwest::Url::parse(value)
        .map_err(|_| rmcp::ErrorData::internal_error("result image URL is invalid", None))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(rmcp::ErrorData::internal_error(
            "result image URL is not a public HTTPS URL",
            None,
        ));
    }
    url.set_fragment(None);
    Ok(url.to_string())
}
