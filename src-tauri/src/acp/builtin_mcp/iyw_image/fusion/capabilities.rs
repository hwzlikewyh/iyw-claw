use serde_json::{json, Value};

use super::super::{ImageRequest, PreparedImage};
use crate::acp::builtin_mcp::iyw_image_models::{image_capabilities, image_dimensions};

pub(in crate::acp::builtin_mcp::iyw_image) fn validate(
    model: &Value,
    request: &ImageRequest,
    (kind, images): (&str, &[PreparedImage]),
) -> Result<(), rmcp::ErrorData> {
    let capabilities = image_capabilities(model);
    let operation = &capabilities[kind];
    for (key, value) in &request.parameters {
        if !value.is_null() && !matches!(key.as_str(), "model" | "prompt" | "mask") {
            validate_parameter(&operation["parameters"][key], key, value)?;
        }
    }
    validate_limits(&operation["limits"], request, images)?;
    if images.iter().any(|image| image.role == "mask")
        || request
            .parameters
            .get("mask")
            .is_some_and(|value| !value.is_null())
    {
        validate_parameter(&operation["parameters"]["mask"], "mask", &json!(true))?;
    }
    Ok(())
}

fn reject(field: &str) -> rmcp::ErrorData {
    tracing::warn!(
        field,
        execution_status = "not_started",
        "[iyw-image] model capability rejected request"
    );
    rmcp::ErrorData::invalid_params(
        format!("Selected image model does not support the requested {field}; choose a compatible model_ref from list_iyw_image_models"),
        Some(json!({"code": "image_model_capability_unsupported", "field": field, "execution_status": "not_started", "retryable": false})),
    )
}

fn validate_parameter(rule: &Value, field: &str, value: &Value) -> Result<(), rmcp::ErrorData> {
    if rule.get("supported").and_then(Value::as_bool) == Some(false) {
        return Err(reject(field));
    }
    if let Some(values) = rule.get("values").and_then(Value::as_array) {
        if !values.contains(value) {
            return Err(reject(field));
        }
    }
    if let Some(number) = value.as_u64() {
        if !within(rule, "min", "max", number) {
            return Err(reject(field));
        }
    }
    if field == "size" && !valid_size(rule, value) {
        return Err(reject(field));
    }
    Ok(())
}

fn valid_size(rule: &Value, value: &Value) -> bool {
    let sizes = rule.get("sizes").and_then(Value::as_array);
    if sizes.is_some_and(|sizes| sizes.contains(value)) {
        return true;
    }
    let Some(custom) = rule.get("custom").filter(|value| value.is_object()) else {
        return sizes.is_none();
    };
    let Some((width, height)) = value.as_str().and_then(image_dimensions) else {
        return false;
    };
    let pixels = width.saturating_mul(height);
    let step = custom
        .get("step")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1);
    let ratio = width.max(height) as f64 / width.min(height) as f64;
    within(custom, "min_edge", "max_edge", width)
        && within(custom, "min_edge", "max_edge", height)
        && within(custom, "min_pixels", "max_pixels", pixels)
        && width % step == 0
        && height % step == 0
        && custom
            .get("max_aspect_ratio")
            .and_then(Value::as_f64)
            .is_none_or(|max| ratio <= max)
}

fn within(rule: &Value, min: &str, max: &str, number: u64) -> bool {
    rule.get(min)
        .and_then(Value::as_u64)
        .is_none_or(|min| number >= min)
        && rule
            .get(max)
            .and_then(Value::as_u64)
            .is_none_or(|max| number <= max)
}

fn limit(limits: &Value, key: &str, number: usize) -> Result<(), rmcp::ErrorData> {
    if limits
        .get(key)
        .and_then(Value::as_u64)
        .is_some_and(|max| number as u64 > max)
    {
        return Err(reject(key));
    }
    Ok(())
}

fn validate_limits(
    limits: &Value,
    request: &ImageRequest,
    images: &[PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    let n = request
        .parameters
        .get("n")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    limit(limits, "max_outputs", n as usize)?;
    limit(
        limits,
        "max_prompt_chars",
        request
            .prompt
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count(),
    )?;
    let masks = images.iter().filter(|image| image.role == "mask").count();
    let parameter_mask =
        usize::from(masks == 0 && request.parameters.get("mask").is_some_and(|v| !v.is_null()));
    limit(limits, "max_masks", masks + parameter_mask)?;
    limit(limits, "max_images", images.len() - masks)?;
    for image in images {
        if let Some(byte_size) = image.byte_size {
            limit(limits, "max_input_bytes", byte_size)?;
            if image.role != "mask" {
                validate_format(limits, image)?;
            }
        }
    }
    Ok(())
}

fn validate_format(limits: &Value, image: &PreparedImage) -> Result<(), rmcp::ErrorData> {
    let Some(formats) = limits.get("input_formats").and_then(Value::as_array) else {
        return Ok(());
    };
    let format = image
        .mime_type
        .as_deref()
        .unwrap_or_default()
        .strip_prefix("image/")
        .unwrap_or_default();
    if !formats.iter().any(|value| value.as_str() == Some(format)) {
        return Err(reject("input_formats"));
    }
    Ok(())
}
