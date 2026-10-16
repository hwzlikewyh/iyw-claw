use std::time::Instant;

use serde_json::{json, Value};

use super::{
    super::{ImageResult, IywGatewayService, WaitOptions},
    output,
};

pub(super) async fn execute(
    service: &IywGatewayService,
    operation: &str,
    (body, wait): (Value, &WaitOptions),
) -> Result<ImageResult, rmcp::ErrorData> {
    let prefix = format!("/ai-application/api/{operation}");
    tracing::info!(operation, "[iyw-image] batch-center submission started");
    let created = service.post_image_gateway(&prefix, "submit", body).await?;
    let id = created
        .get("batchId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            rmcp::ErrorData::internal_error(
                "Batch submission returned no batchId; do not resubmit blindly",
                Some(json!({"execution_status":"unknown"})),
            )
        })?
        .to_string();
    let result = output::batch_result(operation, created, &id);
    if wait.timeout_seconds == Some(0) {
        return Ok(result);
    }
    poll(service, (operation, wait), result).await
}

async fn poll(
    service: &IywGatewayService,
    (operation, wait): (&str, &WaitOptions),
    mut result: ImageResult,
) -> Result<ImageResult, rmcp::ErrorData> {
    let prefix = format!("/ai-application/api/{operation}");
    let id = result.metadata["batch_id"]
        .as_str()
        .expect("validated batch id")
        .to_string();
    let deadline = Instant::now() + wait.platform_timeout();
    while !["succeeded", "partial", "failed", "canceled"].contains(&result.status.as_str())
        && Instant::now() < deadline
    {
        if !super::super::poll::pause(service, wait, deadline).await {
            break;
        }
        let response = super::super::poll::query(
            service,
            (&prefix, "status"),
            (json!({"batchId": id}), deadline),
        )
        .await;
        match response {
            Ok(value) if value.get("batchId").and_then(Value::as_str) == Some(id.as_str()) => {
                let previous = std::mem::take(&mut result.images);
                result = output::batch_result(operation, value, &id);
                super::super::result::preserve_images(&previous, &mut result.images);
            }
            _ => {
                tracing::warn!(
                    operation,
                    "[iyw-image] batch-center query failed; preserving batchId"
                );
                result.metadata["poll_error"] = json!("Query failed or batchId mismatched; query the original batchId before any new submission");
                break;
            }
        }
    }
    tracing::info!(
        operation,
        status = result.status,
        "[iyw-image] batch-center wait finished"
    );
    Ok(result)
}
