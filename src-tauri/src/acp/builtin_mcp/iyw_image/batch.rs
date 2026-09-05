use std::time::Instant;

use futures_util::{stream, StreamExt};
use serde_json::{json, Value};

use super::{
    execute_kind, DeliveryOptions, ImageBatchRequest, ImageExecution, ImageRequest, ImageResult,
    IywGatewayService, PreparedImage, SessionContext,
};

mod prepare;
mod result;

use prepare::{prepare_single, prepare_tasks, validate_batch};
use result::{
    aggregate_status, aggregate_task, collect_images, first_operation, first_task_id,
    item_statuses, log_batch, task_images, BatchCompletion,
};

const MAX_CONCURRENT_EXECUTIONS: usize = 3;

struct PreparedTask {
    index: usize,
    id: Option<String>,
    kind: String,
    request: ImageRequest,
    images: Vec<PreparedImage>,
}

struct RunOutcome {
    task_index: usize,
    run_index: usize,
    duration_ms: u128,
    result: Result<ImageResult, rmcp::ErrorData>,
}

pub(super) async fn execute_single(
    service: &IywGatewayService,
    authority: &SessionContext,
    request: ImageRequest,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let context = ExecutionContext { service, authority };
    if request.id.is_some() {
        return Err(super::invalid("id is only available inside requests"));
    }
    let delivery = request.delivery.clone().unwrap_or_default();
    let task = prepare_single(context, request).await?;
    if task.request.count() == 1 {
        return execute_compatible_single(context, &delivery, &task).await;
    }
    let runs = execute_tasks(service, std::slice::from_ref(&task)).await;
    let value = aggregate_task(&task, &runs);
    finish_single(context, delivery, value).await
}

pub(super) async fn execute_batch(
    service: &IywGatewayService,
    authority: &SessionContext,
    request: ImageBatchRequest,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let context = ExecutionContext { service, authority };
    let started = Instant::now();
    validate_batch(&request)?;
    let tasks = prepare_tasks(context, request.requests).await?;
    let runs = execute_tasks(service, &tasks).await;
    let items = tasks
        .iter()
        .map(|task| aggregate_task(task, &runs))
        .collect::<Vec<_>>();
    let images = collect_images(&items);
    let status = aggregate_status(item_statuses(&items));
    let delivery = deliver(context, &request.delivery, &images).await;
    log_batch(
        &items,
        &runs,
        BatchCompletion {
            status,
            duration_ms: started.elapsed().as_millis(),
        },
    );
    let value = json!({
        "operation": "batch",
        "status": status,
        "images": images,
        "delivery": delivery,
        "items": items,
    });
    Ok(rmcp::model::CallToolResult::structured(value))
}

async fn execute_compatible_single(
    context: ExecutionContext<'_>,
    delivery: &DeliveryOptions,
    task: &PreparedTask,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let result = execute_kind(context.service, image_execution(task)).await?;
    let delivered = if matches!(result.status.as_str(), "succeeded" | "partial") {
        deliver(context, delivery, &result.images).await
    } else {
        empty_delivery()
    };
    let value = json!({
        "operation": result.operation,
        "status": result.status,
        "task_id": result.task_id,
        "images": result.images,
        "metadata": result.metadata,
        "delivery": delivered,
    });
    Ok(rmcp::model::CallToolResult::structured(value))
}

async fn finish_single(
    context: ExecutionContext<'_>,
    delivery: DeliveryOptions,
    task: Value,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let images = task_images(&task);
    let delivered = deliver(context, &delivery, &images).await;
    let value = json!({
        "operation": first_operation(&task).unwrap_or_else(|| task["type"].as_str().unwrap_or("image")),
        "status": task["status"],
        "task_id": first_task_id(&task),
        "images": images,
        "metadata": {"runs": task["runs"]},
        "delivery": delivered,
    });
    Ok(rmcp::model::CallToolResult::structured(value))
}

async fn execute_tasks(service: &IywGatewayService, tasks: &[PreparedTask]) -> Vec<RunOutcome> {
    let mut futures = Vec::new();
    for task in tasks {
        for run_index in 0..task.request.count() {
            futures.push(execute_run(service, task, run_index));
        }
    }
    let mut runs = stream::iter(futures)
        .buffer_unordered(MAX_CONCURRENT_EXECUTIONS)
        .collect::<Vec<_>>()
        .await;
    runs.sort_by_key(|run| (run.task_index, run.run_index));
    runs
}

async fn execute_run(
    service: &IywGatewayService,
    task: &PreparedTask,
    run_index: usize,
) -> RunOutcome {
    let started = Instant::now();
    let result = execute_kind(service, image_execution(task)).await;
    if let Err(error) = &result {
        tracing::warn!(
            task_index = task.index,
            request_id = task.id.as_deref().unwrap_or(""),
            image_type = task.kind,
            run_index,
            error_code = error.code.0,
            "[iyw-image] batch execution failed"
        );
    }
    RunOutcome {
        task_index: task.index,
        run_index,
        duration_ms: started.elapsed().as_millis(),
        result,
    }
}

fn image_execution(task: &PreparedTask) -> ImageExecution<'_> {
    ImageExecution {
        request: &task.request,
        kind: &task.kind,
        images: &task.images,
    }
}

#[derive(Clone, Copy)]
struct ExecutionContext<'a> {
    service: &'a IywGatewayService,
    authority: &'a SessionContext,
}

async fn deliver(
    context: ExecutionContext<'_>,
    options: &DeliveryOptions,
    images: &[String],
) -> Value {
    if images.is_empty() {
        return empty_delivery();
    }
    context
        .service
        .deliver(
            context.authority,
            images,
            options.display,
            options.register_artifact,
        )
        .await
}

fn empty_delivery() -> Value {
    json!({"displayed": [], "artifact": null})
}
