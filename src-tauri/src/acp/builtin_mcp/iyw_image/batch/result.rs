use serde_json::{json, Map, Value};

use super::{PreparedTask, RunOutcome};

pub(super) fn aggregate_task(task: &PreparedTask, runs: &[RunOutcome]) -> Value {
    let selected = runs
        .iter()
        .filter(|run| run.task_index == task.index)
        .collect::<Vec<_>>();
    let statuses = selected
        .iter()
        .map(|run| run_status(run))
        .collect::<Vec<_>>();
    let run_values = selected
        .iter()
        .map(|run| run_value(run))
        .collect::<Vec<_>>();
    let images = collect_images(&run_values);
    json!({
        "index": task.index,
        "id": task.id.as_deref(),
        "type": task.kind.as_str(),
        "status": aggregate_status(statuses),
        "images": images,
        "runs": run_values,
    })
}

fn run_status(run: &RunOutcome) -> &str {
    match &run.result {
        Ok(result) => result.status.as_str(),
        Err(_) => "failed",
    }
}

fn run_value(run: &RunOutcome) -> Value {
    match &run.result {
        Ok(result) => json!({
            "index": run.run_index,
            "operation": result.operation.as_str(),
            "status": result.status.as_str(),
            "task_id": result.task_id.as_deref(),
            "images": completed_images(result),
            "metadata": safe_metadata(result),
            "duration_ms": run.duration_ms,
        }),
        Err(error) => json!({
            "index": run.run_index,
            "status": "failed",
            "task_id": null,
            "images": [],
            "error": "image execution failed",
            "error_code": error.code.0,
            "duration_ms": run.duration_ms,
        }),
    }
}

fn safe_metadata(result: &super::ImageResult) -> Value {
    let mut metadata = Map::new();
    if let Some(model_name) = result
        .metadata
        .get("model_name")
        .and_then(Value::as_str)
        .filter(|value| value.chars().count() <= 256)
    {
        metadata.insert(
            "model_name".to_string(),
            Value::String(model_name.to_string()),
        );
    }
    Value::Object(metadata)
}

fn completed_images(result: &super::ImageResult) -> &[String] {
    if matches!(result.status.as_str(), "succeeded" | "partial") {
        &result.images
    } else {
        &[]
    }
}

pub(super) fn aggregate_status<'a>(statuses: impl IntoIterator<Item = &'a str>) -> &'static str {
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.iter().all(|status| *status == "succeeded") {
        "succeeded"
    } else if statuses
        .iter()
        .any(|status| matches!(*status, "succeeded" | "partial"))
    {
        "partial"
    } else if statuses
        .iter()
        .any(|status| matches!(*status, "running" | "queued"))
    {
        "running"
    } else if statuses.iter().all(|status| *status == "canceled") {
        "canceled"
    } else if statuses.iter().all(|status| *status == "failed") {
        "failed"
    } else {
        "partial"
    }
}

pub(super) fn item_statuses(items: &[Value]) -> impl Iterator<Item = &str> {
    items
        .iter()
        .filter_map(|item| item.get("status").and_then(Value::as_str))
}

pub(super) fn collect_images(values: &[Value]) -> Vec<String> {
    let mut images = Vec::new();
    for value in values {
        for image in image_values(value) {
            if !images.iter().any(|existing| existing == image) {
                images.push(image.to_string());
            }
        }
    }
    images
}

fn image_values(value: &Value) -> impl Iterator<Item = &str> {
    value
        .get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

pub(super) fn task_images(task: &Value) -> Vec<String> {
    collect_images(std::slice::from_ref(task))
}

pub(super) fn first_task_id(task: &Value) -> Option<&str> {
    task.get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|run| run.get("task_id").and_then(Value::as_str))
}

pub(super) fn first_operation(task: &Value) -> Option<&str> {
    task.get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|run| run.get("operation").and_then(Value::as_str))
}

pub(super) struct BatchCompletion<'a> {
    pub(super) status: &'a str,
    pub(super) duration_ms: u128,
}

pub(super) fn log_batch(items: &[Value], runs: &[RunOutcome], completion: BatchCompletion<'_>) {
    let statuses = runs.iter().map(run_status).collect::<Vec<_>>();
    tracing::info!(
        item_count = items.len(),
        execution_count = runs.len(),
        succeeded = count(&statuses, "succeeded"),
        failed = count(&statuses, "failed"),
        running = statuses
            .iter()
            .filter(|value| matches!(**value, "running" | "queued"))
            .count(),
        duration_ms = completion.duration_ms,
        status = completion.status,
        "[iyw-image] batch execution completed"
    );
}

fn count(statuses: &[&str], expected: &str) -> usize {
    statuses.iter().filter(|value| **value == expected).count()
}
