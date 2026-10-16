use std::time::Instant;

use serde_json::{json, Map, Value};

use super::input::PreparedImage;
use super::result::{find_task_id, result_from_value, TERMINAL};
use super::validation::validate_payload;
use super::{invalid, ImageExecution, ImageRequest, ImageResult, IywGatewayService};

pub(super) async fn generate(
    service: &IywGatewayService,
    execution: ImageExecution<'_>,
) -> Result<ImageResult, rmcp::ErrorData> {
    let operation = operation_for(execution.kind)?;
    let payload = prepare_payload(execution.request, execution.kind, execution.images)?;
    let requested_channel = payload.get("modelChannel").cloned();
    tracing::info!(
        image_type = execution.kind,
        model_channel = requested_channel.as_ref().and_then(|value| value.as_u64()),
        explicit_channel = execution.request.parameters.contains_key("modelChannel"),
        "[iyw-image] platform image request channel selected"
    );
    let created = service
        .post_image_gateway(
            "/ai-application/api/commerce",
            &operation,
            Value::Object(payload),
        )
        .await?;
    let mut result = wait_for_task(service, &operation, created, &execution.request.wait).await?;
    if let Some(channel) = requested_channel {
        result.metadata["requested_model_channel"] = channel;
    }
    Ok(result)
}

pub(super) fn validate_request(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    prepare_payload(request, kind, images).map(|_| ())
}

fn prepare_payload(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<Map<String, Value>, rmcp::ErrorData> {
    operation_for(kind)?;
    let mut payload = request.parameters.clone();
    inject_prompt(&mut payload, request.prompt.as_deref());
    inject_images(&mut payload, kind, images);
    validate_payload(kind, &mut payload)?;
    Ok(payload)
}

fn operation_for(kind: &str) -> Result<String, rmcp::ErrorData> {
    let operation = match kind {
        "variation" | "extend" | "mix" | "pattern-apply" | "material-product" | "ip-apply"
        | "extract-pattern" | "repeat-horizontal" | "separate-layers" | "color-transfer" => {
            "g_tools_generate_image"
        }
        "free-imitation" => "fission",
        "outpaint" => "outpainting",
        "super-resolution" => "SuperResolution",
        "split-layers" => "f_tools",
        "enhance" => "EnhanceImage",
        "convert" => "convert",
        "line-extraction" => "lineExtraction",
        "image-to-3d" => "ImageTo3D",
        "video" => "videoGenerator",
        "model-scene" | "background" => "modelScene",
        _ => return Err(invalid(format!("unsupported image type: {kind}"))),
    };
    Ok(operation.to_string())
}

fn inject_prompt(payload: &mut Map<String, Value>, prompt: Option<&str>) {
    if !payload.contains_key("prompt") {
        if let Some(prompt) = prompt.filter(|value| !value.trim().is_empty()) {
            payload.insert(
                "prompt".to_string(),
                Value::String(prompt.trim().to_string()),
            );
        }
    }
}

fn inject_images(payload: &mut Map<String, Value>, kind: &str, images: &[PreparedImage]) {
    if images.is_empty() {
        return;
    }
    let urls = images
        .iter()
        .map(|image| Value::String(image.url.clone()))
        .collect::<Vec<_>>();
    if matches!(kind, "variation" | "extend") {
        payload
            .entry("imageUrls")
            .or_insert_with(|| urls[0].clone());
    } else if kind == "mix" {
        payload.entry("imageUrls").or_insert(Value::Array(urls));
    } else if let Some(field) = first_image_field(kind) {
        payload
            .entry(field.to_string())
            .or_insert_with(|| urls[0].clone());
    } else if !payload.contains_key("imageUrls") {
        payload.insert("imageUrls".to_string(), Value::Array(urls));
    }
}

fn first_image_field(kind: &str) -> Option<&'static str> {
    match kind {
        "free-imitation" | "super-resolution" | "split-layers" | "line-extraction" | "video" => {
            Some("reference")
        }
        "outpaint" | "enhance" | "convert" | "image-to-3d" => Some("image"),
        _ => None,
    }
}

pub(super) async fn wait_for_task(
    service: &IywGatewayService,
    operation: &str,
    created: Value,
    wait: &super::WaitOptions,
) -> Result<ImageResult, rmcp::ErrorData> {
    let task_id = find_task_id(&created);
    if task_id.is_none() || wait.timeout_seconds == Some(0) {
        return Ok(result_from_value(operation, created, task_id));
    }
    let task_id = task_id.expect("checked above");
    let deadline = Instant::now() + wait.platform_timeout();
    let mut result = result_from_value(operation, created, Some(task_id.clone()));
    while !TERMINAL.contains(&result.status.as_str()) && Instant::now() < deadline {
        let task = super::poll::query(
            service,
            ("/ai-application/api/commerce", "getCommerceTaskDetail"),
            (json!({"taskId": task_id}), deadline),
        )
        .await;
        let task = match task {
            Ok(task) => task,
            Err(error) => {
                result.metadata["poll_error"] =
                    super::errors::details(&super::errors::query_failure(
                        error,
                        std::slice::from_ref(&task_id),
                        "/ai-application/api/commerce/getCommerceTaskDetail",
                    ));
                break;
            }
        };
        let previous = std::mem::take(&mut result.images);
        result = result_from_value(operation, task, Some(task_id.clone()));
        super::result::preserve_images(&previous, &mut result.images);
        if TERMINAL.contains(&result.status.as_str())
            || !super::poll::pause(service, wait, deadline).await
        {
            break;
        }
    }
    Ok(result)
}
