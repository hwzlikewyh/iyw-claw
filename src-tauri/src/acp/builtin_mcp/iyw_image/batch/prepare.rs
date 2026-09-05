use std::collections::HashSet;

use super::{ExecutionContext, PreparedTask};
use crate::acp::builtin_mcp::iyw_image::{
    preflight_kind, prepare_images, select_kind, validate_request, ImageBatchRequest, ImageRequest,
    PreparedImage,
};

const MAX_BATCH_ITEMS: usize = 8;
const MAX_EXECUTIONS: usize = 16;
const MAX_ID_CHARS: usize = 64;

pub(super) fn validate_batch(request: &ImageBatchRequest) -> Result<(), rmcp::ErrorData> {
    if !(1..=MAX_BATCH_ITEMS).contains(&request.requests.len()) {
        return Err(super::super::invalid(
            "requests must contain between 1 and 8 items",
        ));
    }
    let mut ids = HashSet::new();
    let mut executions = 0;
    for item in &request.requests {
        validate_request(item)?;
        validate_batch_item(item, &mut ids)?;
        executions += item.count();
    }
    if executions > MAX_EXECUTIONS {
        return Err(super::super::invalid(
            "the sum of request counts must not exceed 16",
        ));
    }
    Ok(())
}

pub(super) async fn prepare_single(
    context: ExecutionContext<'_>,
    request: ImageRequest,
) -> Result<PreparedTask, rmcp::ErrorData> {
    validate_request(&request)?;
    prepare_task(
        context,
        TaskSeed {
            index: 0,
            id: None,
            request,
        },
    )
    .await
}

pub(super) async fn prepare_tasks(
    context: ExecutionContext<'_>,
    requests: Vec<ImageRequest>,
) -> Result<Vec<PreparedTask>, rmcp::ErrorData> {
    let mut tasks = Vec::with_capacity(requests.len());
    for (index, request) in requests.into_iter().enumerate() {
        let id = request.id.as_deref().map(str::trim).map(str::to_string);
        tasks.push(prepare_task(context, TaskSeed { index, id, request }).await?);
    }
    Ok(tasks)
}

fn validate_batch_item(
    item: &ImageRequest,
    ids: &mut HashSet<String>,
) -> Result<(), rmcp::ErrorData> {
    if item.delivery.is_some() {
        return Err(super::super::invalid(
            "delivery must be provided once at the batch root",
        ));
    }
    if item.parameters.contains_key("n") || item.parameters.contains_key("batchSize") {
        return Err(super::super::invalid(
            "batch requests must use count instead of parameters.n or parameters.batchSize",
        ));
    }
    let Some(id) = item.id.as_deref() else {
        return Ok(());
    };
    let id = id.trim();
    if id.is_empty() || id.chars().count() > MAX_ID_CHARS {
        return Err(super::super::invalid(
            "request id must contain 1 to 64 characters",
        ));
    }
    if !ids.insert(id.to_string()) {
        return Err(super::super::invalid("request ids must be unique"));
    }
    Ok(())
}

async fn prepare_task(
    context: ExecutionContext<'_>,
    seed: TaskSeed,
) -> Result<PreparedTask, rmcp::ErrorData> {
    let kind = select_kind(
        seed.request.kind.as_deref(),
        &seed.request.prompt,
        seed.request.images.len(),
    );
    preflight_kind(
        &seed.request,
        &kind,
        &placeholder_images(seed.request.images.len()),
    )?;
    let images = prepare_images(
        context.service,
        context.authority.cwd(),
        &seed.request.images,
    )
    .await?;
    preflight_kind(&seed.request, &kind, &images)?;
    Ok(PreparedTask {
        index: seed.index,
        id: seed.id,
        kind,
        request: seed.request,
        images,
    })
}

struct TaskSeed {
    index: usize,
    id: Option<String>,
    request: ImageRequest,
}

fn placeholder_images(count: usize) -> Vec<PreparedImage> {
    (0..count)
        .map(|_| PreparedImage {
            url: "https://preflight.invalid/image.png".to_string(),
            role: "source".to_string(),
            bytes: None,
            mime_type: Some("image/png".to_string()),
        })
        .collect()
}
