use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};

use super::super::{errors, result::clean_url, ImageResult, IywGatewayService};

pub(super) async fn finish(
    service: &IywGatewayService,
    operation: &str,
    (model_name, response): (&str, &Value),
) -> Result<ImageResult, rmcp::ErrorData> {
    let entries = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| failure(response))?;
    // 上传分支的 Future 只在堆上排队，避免迭代器按值搬运大型状态。
    let pending = entries
        .iter()
        .map(|entry| Box::pin(materialize(service, entry)))
        .collect::<Vec<_>>();
    let results = stream::iter(pending)
        .buffered(super::super::limits::MAX_CONCURRENT_EXECUTIONS)
        .collect::<Vec<_>>()
        .await;
    let mut images = Vec::new();
    let mut failures = Vec::new();
    for (index, result) in results.into_iter().enumerate() {
        match result {
            Ok(url) => images.push(url),
            Err(error) => {
                failures.push(json!({"index": index, "details": errors::details(&error)}))
            }
        }
    }
    if images.is_empty() {
        return Err(failed_output(response, model_name, failures));
    }
    tracing::info!(
        operation,
        succeeded = images.len(),
        failed = failures.len(),
        "[iyw-image] Fusion output preparation completed"
    );
    Ok(ImageResult {
        operation: operation.to_string(),
        status: if failures.is_empty() {
            "succeeded"
        } else {
            "partial"
        }
        .to_string(),
        task_id: None,
        images,
        metadata: json!({"model_name": model_name, "request_id": response.get("_iyw_request_id"),
            "output_errors": failures}),
    })
}

fn failed_output(response: &Value, model_name: &str, failures: Vec<Value>) -> rmcp::ErrorData {
    let mut error = errors::model_failure(failure(response), model_name);
    if let Some(data) = &mut error.data {
        data["output_errors"] = json!(failures);
    }
    error
}

async fn materialize(
    service: &IywGatewayService,
    entry: &Value,
) -> Result<String, rmcp::ErrorData> {
    if let Some(url) = entry.get("url").and_then(Value::as_str) {
        if let Ok(url) = clean_url(url) {
            return Ok(url);
        }
    }
    let encoded = entry
        .get("b64_json")
        .and_then(Value::as_str)
        .ok_or_else(|| output_error("Image output contains no valid HTTPS URL or image data"))?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| output_error("Image output Base64 is invalid"))?;
    let mime = crate::acp::delegation::image_format::detect_mime(&bytes)
        .ok_or_else(|| output_error("Image output format is unsupported"))?;
    let extension = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => return Err(output_error("Image output must be PNG/JPEG/WebP")),
    };
    service.upload_bytes(bytes, mime, extension).await
}

fn output_error(message: &str) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        message.to_string(),
        Some(json!({"message": message,
        "stage": "output_materialize", "generation_completed": true, "retryable": false})),
    )
}

fn failure(response: &Value) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        "Generation completed but no output could be prepared; do not regenerate blindly",
        Some(
            json!({"code": "image_output_unavailable", "stage": "output_materialize",
            "request_id": response.get("_iyw_request_id"), "generation_completed": true,
            "execution_status": "unknown", "retryable": false}),
        ),
    )
}
