use std::time::Instant;

use futures_util::{stream, StreamExt};
use serde_json::Value;

use super::super::{limits::MAX_BATCH_ITEMS, recovery};
use super::{
    execute_tasks,
    prepare::{plan_tasks, prepare_task},
    result::aggregate_task,
    ExecutionContext, ImageRequest, PreparedTask, RunOutcome,
};

pub(super) async fn execute(
    context: ExecutionContext<'_>,
    requests: Vec<ImageRequest>,
) -> Result<Vec<Value>, rmcp::ErrorData> {
    let tasks = plan_tasks(requests)?;
    // 全部静态校验先完成，避免后面的非法请求导致前面的任务已扣费。
    for task in &tasks {
        recovery::validate_models(
            context.service,
            (&task.request, &task.kind),
            &task.candidates,
        )
        .await?;
    }
    let mut items = stream::iter(tasks)
        .map(|task| execute_item(context, task))
        .buffer_unordered(MAX_BATCH_ITEMS)
        .collect::<Vec<_>>()
        .await;
    items.sort_by_key(|item| item["index"].as_u64().unwrap_or_default());
    Ok(items)
}

async fn execute_item(context: ExecutionContext<'_>, mut task: PreparedTask) -> Value {
    let started = Instant::now();
    let runs = match prepare_task(context, &mut task).await {
        Ok(()) => execute_tasks(context.service, std::slice::from_ref(&task)).await,
        Err(mut error) => {
            tracing::warn!(
                batch_index = task.index,
                error_code = error.code.0,
                "[iyw-image] input preparation failed; other batch items continue"
            );
            let mut details = super::super::errors::details(&error);
            details["stage"] = serde_json::json!("input");
            details["execution_status"] = serde_json::json!("not_started");
            details["retryable"] = serde_json::json!(false);
            error.data = Some(details);
            vec![RunOutcome {
                task_index: task.index,
                run_index: 0,
                duration_ms: started.elapsed().as_millis(),
                result: Err(error),
            }]
        }
    };
    aggregate_task(&task, &runs)
}
