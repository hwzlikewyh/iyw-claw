use serde_json::{json, Value};

use super::super::{
    result::{extract_urls, find_task_id, result_from_value},
    ImageResult,
};

pub(super) fn batch_result(operation: &str, mut value: Value, id: &str) -> ImageResult {
    super::super::result::redact_credentials(&mut value);
    let status = match value.get("statusLabel").and_then(Value::as_str) {
        Some("pending") => "queued",
        Some("completed") => "succeeded",
        Some("partial") => "partial",
        Some("failed") => "failed",
        _ => "running",
    };
    ImageResult {
        operation: format!("{operation}/submit"),
        status: status.into(),
        task_id: None,
        images: extract_urls(&value),
        metadata: json!({"batch_id": id, "query": format!("/ai-application/api/{operation}/status"), "result": value}),
    }
}

pub(super) fn product_result(kind: &str, operation: &str, mut value: Value) -> ImageResult {
    super::super::result::redact_credentials(&mut value);
    let mut result = result_from_value(operation, value.clone(), None);
    result.status = match result.status.as_str() {
        "completed" | "success" => "succeeded".into(),
        "pending" => "queued".into(),
        "cancelled" => "canceled".into(),
        _ => result.status,
    };
    if result.task_id.is_none()
        && result.status == "running"
        && !result.images.is_empty()
        && value.get("status").is_none()
        && value.get("process").is_none()
    {
        result.status = "succeeded".into();
    }
    let query = if kind == "a-plus-edit" {
        "/ai-application/api/commerce/getCommerceTaskDetail"
    } else {
        "/ai-application/api/commerce/searchTaskResult"
    };
    result.metadata = json!({"result": value, "query": query});
    result.task_id = find_task_id(&result.metadata["result"]);
    result
}
