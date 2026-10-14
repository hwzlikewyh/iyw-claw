use serde_json::{json, Value};

use super::super::{errors, ImageResult};

pub(super) struct Attempt<'a> {
    pub index: usize,
    pub candidate_index: usize,
    pub retry_index: usize,
    pub retry_delay_seconds: Option<u64>,
    pub kind: &'a str,
    pub duration_ms: u128,
    pub decision: &'a str,
}

pub(super) fn decision(result: &Result<ImageResult, rmcp::ErrorData>) -> &'static str {
    match result {
        Ok(result) if result.metadata.get("poll_error").is_some() => "stop_uncertain",
        Ok(result) if result.status == "failed" && result.images.is_empty() => "fallback",
        Ok(result) if matches!(result.status.as_str(), "running" | "queued") => "stop_pending",
        Ok(result) if matches!(result.status.as_str(), "succeeded" | "partial") => "stop_success",
        Ok(_) => "stop_terminal",
        Err(error) => {
            let data = error.data.as_ref();
            if data
                .and_then(|value| value.get("execution_status"))
                .and_then(Value::as_str)
                == Some("unknown")
            {
                "stop_uncertain"
            } else if data
                .and_then(|value| value.get("retryable"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                "fallback"
            } else {
                "stop_error"
            }
        }
    }
}

pub(super) fn attempt(
    result: &Result<ImageResult, rmcp::ErrorData>,
    context: Attempt<'_>,
) -> Value {
    let mut value = json!({"index": context.index, "type": context.kind,
        "candidate_index": context.candidate_index, "retry_index": context.retry_index,
        "retry_delay_seconds": context.retry_delay_seconds,
        "duration_ms": context.duration_ms, "decision": context.decision});
    match result {
        Ok(result) => {
            value["status"] = json!(result.status);
            value["operation"] = json!(result.operation);
            value["task_id"] = json!(result.task_id);
            for key in [
                "model_name",
                "request_id",
                "query",
                "batch_id",
                "poll_error",
            ] {
                if let Some(field) = result.metadata.get(key) {
                    value[key] = field.clone();
                }
            }
            let reason = result
                .metadata
                .get("result")
                .and_then(|result| result.get("reason"))
                .and_then(Value::as_str);
            if let Some(reason) = reason {
                value["error_details"] = errors::details(&rmcp::ErrorData::internal_error(
                    "task failed",
                    Some(json!({"message": reason})),
                ));
            }
        }
        Err(error) => {
            value["status"] = json!("failed");
            value["error_code"] = json!(error.code.0);
            value["error_details"] = errors::details(error);
        }
    }
    value
}

pub(super) fn finish(
    result: Result<ImageResult, rmcp::ErrorData>,
    attempts: Vec<Value>,
    reason: &str,
) -> Result<ImageResult, rmcp::ErrorData> {
    match result {
        Ok(mut result) => {
            result.metadata["attempts"] = json!(attempts);
            result.metadata["recovery_stop_reason"] = json!(reason);
            Ok(result)
        }
        Err(mut error) => {
            let mut details = errors::details(&error);
            details["attempts"] = json!(attempts);
            details["recovery_stop_reason"] = json!(reason);
            error.data = Some(details);
            Err(error)
        }
    }
}
