use reqwest::Url;
use rmcp::ErrorData;
use serde_json::json;

pub(super) fn ensure_image_enabled(kind: Option<&str>) -> Result<(), ErrorData> {
    if kind.is_some_and(|kind| kind.trim().eq_ignore_ascii_case("image-to-3d")) {
        return Err(disabled_image_to_3d());
    }
    Ok(())
}

pub(super) fn ensure_url_enabled(url: &Url) -> Result<(), ErrorData> {
    let path = urlencoding::decode(url.path()).unwrap_or_else(|_| url.path().into());
    if path
        .split('/')
        .any(|segment| segment.eq_ignore_ascii_case("ImageTo3D"))
    {
        return Err(disabled_image_to_3d());
    }
    Ok(())
}

fn disabled_image_to_3d() -> ErrorData {
    tracing::warn!(target: "builtin_mcp", operation = "image-to-3d",
        execution_status = "not_started", "[iyw-tools] disabled operation rejected");
    ErrorData::invalid_request(
        "Built-in image-to-3d is disabled. Do not retry it through fetch, browser, or another wrapper",
        Some(json!({
            "code": "operation_disabled",
            "operation": "image-to-3d",
            "execution_status": "not_started"
        })),
    )
}
