use serde_json::Value;

use super::super::{
    fusion, invalid, preflight_kind, validate_request, ImageRequest, IywGatewayService,
    PreparedImage,
};
use crate::acp::builtin_mcp::{
    iyw_image_models::load_catalog, iyw_tool_policy::ensure_image_enabled,
};

use super::super::limits::MAX_FALLBACKS;
const SOURCE_FIELDS: [&str; 9] = [
    "images",
    "image",
    "imageUrls",
    "imageUrl",
    "reference",
    "productImg",
    "styleImg",
    "reference_image",
    "mask",
];

pub(in crate::acp::builtin_mcp::iyw_image) fn prepare_candidates(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<Vec<ImageRequest>, rmcp::ErrorData> {
    if request.fallbacks.len() > MAX_FALLBACKS {
        return Err(invalid("fallbacks accepts at most 3 candidates"));
    }
    if request.fallbacks.is_empty() {
        return Ok(Vec::new());
    }
    validate_counts(request)?;
    let mut candidates: Vec<ImageRequest> = Vec::new();
    for fallback in &request.fallbacks {
        if fallback.kind.trim().is_empty() || fallback.kind == "auto" {
            return Err(invalid("each fallback requires an explicit type"));
        }
        ensure_image_enabled(Some(&fallback.kind))?;
        let candidate = request.candidate(fallback, kind);
        validate_request(&candidate)?;
        validate_counts(&candidate)?;
        validate_preserved_inputs(request, &candidate)?;
        preflight_kind(&candidate, &fallback.kind, images)?;
        if same_route(request, kind, &candidate)
            || candidates.iter().any(|previous| {
                same_route(
                    previous,
                    previous.kind.as_deref().unwrap_or_default(),
                    &candidate,
                )
            })
        {
            return Err(invalid(
                "fallback candidates must not repeat the same type and parameters",
            ));
        }
        candidates.push(candidate);
    }
    Ok(candidates)
}

fn validate_preserved_inputs(
    request: &ImageRequest,
    candidate: &ImageRequest,
) -> Result<(), rmcp::ErrorData> {
    if request.parameters.get("background").and_then(Value::as_str) == Some("transparent")
        && (!matches!(candidate.kind.as_deref(), Some("generate" | "edit"))
            || candidate
                .parameters
                .get("background")
                .and_then(Value::as_str)
                != Some("transparent"))
    {
        return Err(invalid("fallbacks must preserve required background=transparent and use a compatible generate/edit model"));
    }
    if candidate.parameters.contains_key("prompt")
        || SOURCE_FIELDS
            .iter()
            .any(|key| candidate.parameters.contains_key(*key))
    {
        return Err(invalid("fallbacks inherit the original prompt, images and mask; do not replace them in parameters"));
    }
    Ok(())
}

fn validate_counts(request: &ImageRequest) -> Result<(), rmcp::ErrorData> {
    if request.parameters.contains_key("n") || request.parameters.contains_key("batchSize") {
        return Err(invalid("requests with fallbacks must use count instead of parameters.n or parameters.batchSize"));
    }
    Ok(())
}

fn same_route(initial: &ImageRequest, kind: &str, candidate: &ImageRequest) -> bool {
    let mut parameters = initial.parameters.clone();
    parameters.remove("prompt");
    candidate.kind.as_deref() == Some(kind) && parameters == candidate.parameters
}

pub(in crate::acp::builtin_mcp::iyw_image) async fn validate_models(
    service: &IywGatewayService,
    initial: (&ImageRequest, &str),
    (candidates, images): (&[ImageRequest], &[PreparedImage]),
) -> Result<(), rmcp::ErrorData> {
    let requests = std::iter::once(initial)
        .chain(candidates.iter().map(|request| {
            (
                request,
                request.kind.as_deref().expect("candidate type validated"),
            )
        }))
        .filter(|(_, kind)| matches!(*kind, "generate" | "edit"))
        .collect::<Vec<_>>();
    if requests.is_empty() {
        return Ok(());
    }
    let models: Vec<Value> = load_catalog(service).await?;
    for (request, kind) in requests {
        fusion::resolve_model(&models, &request.parameters, kind == "edit")?;
        fusion::validate_capabilities(&models, request, (kind, images))?;
    }
    Ok(())
}
