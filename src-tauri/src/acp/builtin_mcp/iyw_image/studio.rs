use super::{input::PreparedImage, ImageExecution, ImageRequest, ImageResult, IywGatewayService};

mod batch;
mod output;
mod payload;

const PRODUCT_KIT_PREFIX: &str = "/ai-agent-new/api/product-kit";
const COMMERCE_PREFIX: &str = "/ai-application/api/commerce";
const PRODUCT_TIMEOUT_SECONDS: u64 = 650;
const BATCH_TIMEOUT_SECONDS: u64 = 120;

const BATCH_OPERATIONS: &[(&str, &str)] = &[
    ("batch-shape-fill", "batchShapeFill"),
    ("batch-generate", "batchGenerate"),
    ("batch-watermark", "batchWatermark"),
    ("batch-series-extend", "batchSeriesExtend"),
    ("batch-enhance", "batchEnhance"),
    ("batch-upscale", "batchUpscale"),
    ("batch-background-remove", "batchBackgroundRemove"),
    ("batch-extract-pattern", "batchExtractPattern"),
    ("batch-replace-scene", "batchReplaceScene"),
    ("batch-mockup", "batchMockup"),
];

pub(super) fn supports(kind: &str) -> bool {
    matches!(kind, "product-kit" | "a-plus" | "a-plus-edit") || batch_operation(kind).is_some()
}

pub(super) fn http_timeout(kind: &str) -> Option<u64> {
    if matches!(kind, "product-kit" | "a-plus") {
        return Some(PRODUCT_TIMEOUT_SECONDS);
    }
    batch_operation(kind).map(|_| BATCH_TIMEOUT_SECONDS)
}

fn batch_operation(kind: &str) -> Option<&'static str> {
    BATCH_OPERATIONS
        .iter()
        .find_map(|(name, operation)| (*name == kind).then_some(*operation))
}

pub(super) fn validate_request(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    payload::prepare(request, kind, images).map(|_| ())
}

pub(super) async fn generate(
    service: &IywGatewayService,
    execution: ImageExecution<'_>,
) -> Result<ImageResult, rmcp::ErrorData> {
    let body = payload::prepare(execution.request, execution.kind, execution.images)?;
    if let Some(operation) = batch_operation(execution.kind) {
        return batch::execute(service, operation, (body, &execution.request.wait)).await;
    }
    let (prefix, path) = match execution.kind {
        "product-kit" => (PRODUCT_KIT_PREFIX, "generate-kit"),
        "a-plus" => (PRODUCT_KIT_PREFIX, "generate-a-plus"),
        _ => (COMMERCE_PREFIX, "g_tools_generate_image"),
    };
    tracing::info!(
        image_type = execution.kind,
        operation = path,
        "[iyw-image] studio request started"
    );
    let result = service.post_gateway(prefix, path, body).await?;
    if execution.kind == "a-plus-edit" {
        return super::commerce::wait_for_task(service, path, result, &execution.request.wait)
            .await;
    }
    Ok(output::product_result(execution.kind, path, result))
}
