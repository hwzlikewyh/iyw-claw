use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::result::{extract_urls, find_task_id, normalize_status};
use super::{invalid, required_prompt, ImageRequest, ImageResult, IywGatewayService};

pub(super) async fn generate(
    service: &IywGatewayService,
    request: &ImageRequest,
) -> Result<ImageResult, rmcp::ErrorData> {
    let prompt = required_prompt(request.prompt.as_deref())?;
    let models = request
        .parameters
        .get("models")
        .cloned()
        .unwrap_or_else(default_models);
    validate_models(&models)?;
    let mut payload = request.parameters.clone();
    payload.insert("prompt".to_string(), Value::String(prompt));
    payload.entry("jsonData".to_string()).or_insert(Value::Null);
    payload.insert("models".to_string(), models);
    let created = service
        .post_gateway(
            "/ai-application/api/microModel",
            "v2/batch",
            Value::Object(payload),
        )
        .await?;
    let task_ids = created_task_ids(&created);
    if task_ids.is_empty() {
        return Err(rmcp::ErrorData::internal_error(
            "fission batch returned no task IDs",
            None,
        ));
    }
    if request.wait.timeout_seconds == 0 {
        return Ok(group_result(task_ids, Vec::new()));
    }
    wait_for_tasks(service, request, task_ids).await
}

async fn wait_for_tasks(
    service: &IywGatewayService,
    request: &ImageRequest,
    task_ids: Vec<String>,
) -> Result<ImageResult, rmcp::ErrorData> {
    let deadline = Instant::now() + Duration::from_secs(request.wait.timeout_seconds);
    loop {
        let reports = load_reports(service, &task_ids).await?;
        if group_status(&reports) != "running" || Instant::now() >= deadline {
            return Ok(group_result(task_ids, reports));
        }
        tokio::time::sleep(Duration::from_secs_f64(request.wait.poll_interval_seconds)).await;
    }
}

async fn load_reports(
    service: &IywGatewayService,
    task_ids: &[String],
) -> Result<Vec<Value>, rmcp::ErrorData> {
    let mut reports = Vec::with_capacity(task_ids.len());
    for task_id in task_ids {
        reports.push(
            service
                .post_gateway(
                    "/ai-application/api/microModel",
                    "GetDetails",
                    json!({"taskId": task_id}),
                )
                .await?,
        );
    }
    Ok(reports)
}

fn group_result(task_ids: Vec<String>, reports: Vec<Value>) -> ImageResult {
    let status = if reports.is_empty() {
        "running"
    } else {
        group_status(&reports)
    };
    let task_id = task_ids.first().cloned();
    let metadata = json!({"task_ids": task_ids, "tasks": reports});
    ImageResult {
        operation: "fission".to_string(),
        status: status.to_string(),
        task_id,
        images: extract_urls(&metadata),
        metadata,
    }
}

fn group_status(reports: &[Value]) -> &'static str {
    let statuses = reports.iter().map(normalize_status).collect::<Vec<_>>();
    if statuses.iter().all(|status| status == "succeeded") {
        "succeeded"
    } else if statuses
        .iter()
        .any(|status| matches!(status.as_str(), "queued" | "running"))
    {
        "running"
    } else if statuses.iter().all(|status| status == "canceled") {
        "canceled"
    } else if statuses.iter().all(|status| status == "failed") {
        "failed"
    } else {
        "partial"
    }
}

fn created_task_ids(value: &Value) -> Vec<String> {
    value
        .get("tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(find_task_id)
        .collect()
}

fn validate_models(models: &Value) -> Result<(), rmcp::ErrorData> {
    let models = models
        .as_array()
        .ok_or_else(|| invalid("models must be an array"))?;
    if models.is_empty() || models.iter().any(|model| !model.is_object()) {
        return Err(invalid(
            "models must contain at least one configuration object",
        ));
    }
    Ok(())
}

fn default_models() -> Value {
    json!([{"platform": "4", "size": 20, "stats": {"model": "local_flux"}}])
}
