use serde_json::{json, Value};

use super::{
    input::PreparedImage, invalid, result, ImageExecution, ImageRequest, ImageResult,
    IywGatewayService,
};

mod output;
mod validation;

use output::native_result;

const COMMERCE: &str = "/ai-application/api/commerce";
const MICRO: &str = "/ai-application/api/microModel";

const COMMERCE_OPERATIONS: &[(&str, &str)] = &[
    ("modify", "imageModification"),
    ("seed-edit", "SeedEdit"),
    ("blend", "blend"),
    ("erase", "erase"),
    ("watermark-erase", "watermarkEraser"),
    ("extract", "extraction"),
    ("lineart", "lineart"),
    ("vectorize", "vectorizeImage"),
    ("three-views", "threeVisions"),
    ("bleed-line", "bleedLine"),
    ("upscale", "upscaleImage"),
    ("super-upscale", "SuperUpscale"),
    ("video-auto-director", "videoAutoDirector"),
    ("video-remake-director", "videoRemakeDirector"),
    ("extract-color", "extractColor"),
    ("save-color", "saveColor"),
    ("detect-grid", "detectImageGrid"),
    ("classify-intent", "classifyCanvasIntent"),
    ("build-extract-prompts", "buildExtractPrompts"),
    ("g-tools", "g_tools"),
    ("f-tools", "f_tools"),
];

pub(super) struct Operation {
    prefix: &'static str,
    path: &'static str,
    poll: Option<&'static str>,
}

pub(super) fn operation(kind: &str) -> Option<Operation> {
    if let Some(operation) = special_operation(kind) {
        return Some(operation);
    }
    let path = COMMERCE_OPERATIONS
        .iter()
        .find_map(|(name, path)| (*name == kind).then_some(*path))?;
    let poll = (!matches!(
        kind,
        "extract-color"
            | "save-color"
            | "detect-grid"
            | "classify-intent"
            | "build-extract-prompts"
    ))
    .then_some("getCommerceTaskDetail");
    Some(Operation {
        prefix: COMMERCE,
        path,
        poll,
    })
}

fn special_operation(kind: &str) -> Option<Operation> {
    let (prefix, path, poll) = match kind {
        "background-remove" => (MICRO, "GetImageSegment", None),
        "check-image" => (MICRO, "checkImage", None),
        "micro-upscale" => (MICRO, "upscale", Some("GetDetails")),
        "micro-upscale-image" => (MICRO, "upscaleImage", Some("GetDetails")),
        "micro-generate" => (MICRO, "v2/generate", Some("GetDetails")),
        "micro-variation" => (MICRO, "variation", Some("GetDetails")),
        "scheme-generate" => (
            "/ai-chat/api/designScheme",
            "generateImage",
            Some("searchGenerateResult"),
        ),
        "faddish" => ("/ai-application/faddish", "generate", None),
        _ => return None,
    };
    Some(Operation { prefix, path, poll })
}

fn image_key(kind: &str) -> &'static str {
    match kind {
        "background-remove" | "micro-upscale" | "micro-upscale-image" => "imageUrl",
        "check-image" => "image",
        "g-tools" => "images",
        _ => "imageUrls",
    }
}

pub(super) fn validate_request(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    payload(request, kind, images).map(|_| ())
}

fn payload(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<Value, rmcp::ErrorData> {
    let mut payload = request.parameters.clone();
    if let Some(prompt) = request
        .prompt
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload
            .entry("prompt")
            .or_insert_with(|| json!(prompt.trim()));
    }
    let key = image_key(kind);
    let single = matches!(key, "imageUrl" | "image");
    if !images.is_empty() {
        if payload.contains_key(key) {
            return Err(invalid(
                "Provide source images in images or parameters, not both",
            ));
        }
        let urls = images
            .iter()
            .map(|image| json!(image.url))
            .collect::<Vec<_>>();
        payload.insert(
            key.to_string(),
            if single { urls[0].clone() } else { json!(urls) },
        );
    }
    if single && images.len() > 1 {
        return Err(invalid("This operation accepts one source image"));
    }
    validation::validate(kind, &payload)?;
    Ok(Value::Object(payload))
}

pub(super) async fn generate(
    service: &IywGatewayService,
    execution: ImageExecution<'_>,
) -> Result<ImageResult, rmcp::ErrorData> {
    let op =
        operation(execution.kind).ok_or_else(|| invalid("Unsupported native image operation"))?;
    let body = payload(execution.request, execution.kind, execution.images)?;
    tracing::info!(
        image_type = execution.kind,
        operation = op.path,
        "[iyw-image] native operation submitted"
    );
    let created = service.post_gateway(op.prefix, op.path, body).await?;
    let result = native_result(&op, created, None);
    if result.task_id.is_none()
        || op.poll.is_none()
        || execution.request.wait.timeout_seconds == Some(0)
    {
        return Ok(result);
    }
    poll(service, (&op, &execution.request.wait), result).await
}

async fn poll(
    service: &IywGatewayService,
    (op, wait): (&Operation, &super::WaitOptions),
    mut current: ImageResult,
) -> Result<ImageResult, rmcp::ErrorData> {
    let deadline = std::time::Instant::now() + wait.platform_timeout();
    while !result::TERMINAL.contains(&current.status.as_str())
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_secs_f64(
            wait.poll_interval_seconds,
        ))
        .await;
        let response = service
            .post_gateway(
                op.prefix,
                op.poll.expect("polling endpoint checked"),
                json!({"taskId": current.task_id}),
            )
            .await;
        let value = match response {
            Ok(value) => value,
            Err(_) => {
                current.metadata["poll_error"] = json!("Task query failed; preserve task_id and query its status before retrying generation");
                return Ok(current);
            }
        };
        let task_id = current.task_id.clone();
        current = native_result(op, value, task_id);
    }
    Ok(current)
}
