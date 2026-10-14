use std::collections::HashSet;
use std::time::Instant;

use super::{ExecutionContext, PreparedTask};
use crate::acp::builtin_mcp::iyw_image::{
    fusion, preflight_kind, prepare_images, recovery, select_kind, upload_images, validate_request,
    ImageBatchRequest, ImageRequest, ImageSource, PreparedImage,
};
use crate::acp::builtin_mcp::iyw_tool_policy::ensure_image_enabled;

use super::super::limits::{MAX_BATCH_EXECUTIONS, MAX_BATCH_ITEMS};
const MAX_ID_CHARS: usize = 64;

pub(super) fn validate_batch(request: &ImageBatchRequest) -> Result<(), rmcp::ErrorData> {
    if !(1..=MAX_BATCH_ITEMS).contains(&request.requests.len()) {
        return Err(super::super::invalid(format!(
            "requests must contain between 1 and {MAX_BATCH_ITEMS} items"
        )));
    }
    let mut ids = HashSet::new();
    let mut executions: usize = 0;
    for item in &request.requests {
        ensure_image_enabled(item.kind.as_deref())?;
        validate_request(item)?;
        validate_batch_item(item, &mut ids)?;
        executions = executions.saturating_add(
            item.count()
                .saturating_mul(item.fallbacks.len().saturating_add(1)),
        );
    }
    if executions > MAX_BATCH_EXECUTIONS {
        return Err(super::super::invalid(
            format!("planned candidate executions, including count and fallbacks but excluding automatic output retries, must not exceed {MAX_BATCH_EXECUTIONS}; split into multiple batches"),
        ));
    }
    Ok(())
}

pub(super) async fn prepare_single(
    context: ExecutionContext<'_>,
    request: ImageRequest,
) -> Result<PreparedTask, rmcp::ErrorData> {
    ensure_image_enabled(request.kind.as_deref())?;
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
    let started = Instant::now();
    let routing_prompt = seed
        .request
        .parameters
        .get("prompt")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .or_else(|| seed.request.prompt.clone());
    let kind = select_kind(
        seed.request.kind.as_deref(),
        &routing_prompt,
        seed.request.images.len(),
    );
    let (images, candidates) = prepare_inputs(context, &seed.request, &kind).await?;
    tracing::info!(
        requested_type = seed.request.kind.as_deref().unwrap_or("auto"),
        selected_type = %kind,
        image_count = images.len(),
        batch_index = seed.index,
        preparation_ms = started.elapsed().as_millis(),
        "[iyw-image] image route prepared"
    );
    Ok(PreparedTask {
        index: seed.index,
        id: seed.id,
        kind,
        request: seed.request,
        images,
        candidates,
    })
}

async fn prepare_inputs(
    context: ExecutionContext<'_>,
    request: &ImageRequest,
    kind: &str,
) -> Result<(Vec<PreparedImage>, Vec<ImageRequest>), rmcp::ErrorData> {
    let placeholders = placeholder_images(&request.images);
    preflight_kind(request, kind, &placeholders)?;
    let candidates = recovery::prepare_candidates(request, kind, &placeholders)?;
    recovery::validate_models(
        context.service,
        (request, kind),
        (&candidates, &placeholders),
    )
    .await?;
    let mut images = prepare_images(context.authority.cwd(), &request.images).await?;
    if kind == "edit"
        || candidates
            .iter()
            .any(|request| request.kind.as_deref() == Some("edit"))
    {
        fusion::prepare_edit_images(
            (context.service, context.authority.cwd()),
            (request, kind),
            &mut images,
        )
        .await?;
    }
    recovery::validate_models(context.service, (request, kind), (&candidates, &images)).await?;
    if kind != "edit"
        || candidates
            .iter()
            .any(|request| request.kind.as_deref() != Some("edit"))
    {
        upload_images(context.service, &mut images).await?;
    }
    preflight_kind(request, kind, &images)?;
    for candidate in &candidates {
        preflight_kind(
            candidate,
            candidate.kind.as_deref().expect("candidate type validated"),
            &images,
        )?;
    }
    Ok((images, candidates))
}

struct TaskSeed {
    index: usize,
    id: Option<String>,
    request: ImageRequest,
}

fn placeholder_images(sources: &[ImageSource]) -> Vec<PreparedImage> {
    sources
        .iter()
        .map(|source| PreparedImage {
            url: "https://preflight.invalid/image.png".to_string(),
            role: source.role().to_string(),
            bytes: None,
            mime_type: Some("image/png".to_string()),
        })
        .collect()
}
