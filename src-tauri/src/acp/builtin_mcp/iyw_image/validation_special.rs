use serde_json::{Map, Value};

use super::invalid;
use super::validation::{
    image_field, image_urls, nonnegative_number, number_range, require_object, required_string,
    validate_https,
};

const RATIOS: [&str; 7] = ["auto", "1:1", "4:3", "3:4", "16:9", "9:16", "21:9"];

pub(super) fn free_imitation(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "reference")?;
    let stats = require_object(payload, "stats")?;
    for key in ["width", "height", "strength"] {
        nonnegative_number(stats, key)?;
    }
    if payload.get("model").and_then(Value::as_str) != Some("free") {
        return Err(invalid("free-imitation model must be free"));
    }
    Ok(())
}

pub(super) fn outpaint(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "image")?;
    for key in ["top", "right", "bottom", "left"] {
        number_range(payload, key, 0.0, 1.0)?;
    }
    Ok(())
}

pub(super) fn super_resolution(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "reference")?;
    match payload.get("upscale").and_then(Value::as_i64) {
        Some(2 | 4) => Ok(()),
        _ => Err(invalid("upscale must be 2 or 4")),
    }
}

pub(super) fn split_layers(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "reference")?;
    if payload.get("model").and_then(Value::as_str) != Some("extract_layers") {
        return Err(invalid("split-layers model must be extract_layers"));
    }
    Ok(())
}

pub(super) fn enhance(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "image")?;
    if !matches!(
        payload.get("enhanceType").and_then(Value::as_i64),
        Some(1 | 2)
    ) {
        return Err(invalid("enhanceType must be 1 or 2"));
    }
    if payload.get("model").and_then(Value::as_i64).is_none() {
        return Err(invalid("enhance model must be an integer"));
    }
    Ok(())
}

pub(super) fn convert(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "image")?;
    for key in ["inputFormat", "outputFormat"] {
        let format = required_string(payload, key)?.to_ascii_lowercase();
        if !matches!(
            format.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
        ) {
            return Err(invalid(format!("unsupported image format: {format}")));
        }
    }
    Ok(())
}

pub(super) fn line_extraction(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "reference")?;
    let stats = require_object(payload, "stats")?;
    validate_https(required_string(stats, "reference")?)?;
    if !matches!(
        payload.get("model").and_then(Value::as_str),
        Some("realistic" | "canny")
    ) {
        return Err(invalid("line-extraction model must be realistic or canny"));
    }
    if payload
        .get("batch_size")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .is_none()
    {
        return Err(invalid("line-extraction batch_size must be positive"));
    }
    Ok(())
}

pub(super) fn color_transfer(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    let urls = image_urls(payload, 2, 2)?;
    payload.insert(
        "toolName".to_string(),
        Value::String("color_transfer".to_string()),
    );
    payload.insert(
        "imageUrls".to_string(),
        Value::Array(urls.into_iter().map(Value::String).collect()),
    );
    image_field(payload, "productImg")?;
    image_field(payload, "styleImg")?;
    if !matches!(
        payload.get("resolution").and_then(Value::as_str),
        Some("2K" | "4K")
    ) {
        return Err(invalid("color-transfer resolution must be 2K or 4K"));
    }
    Ok(())
}

pub(super) fn image_to_3d(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "image")?;
    let stats = require_object(payload, "stats")?;
    if stats.get("format").and_then(Value::as_i64).is_none() {
        return Err(invalid("image-to-3d requires stats.format"));
    }
    if let Some(views) = stats.get("MultiViewImages") {
        validate_3d_views(views)?;
    }
    Ok(())
}

pub(super) fn video(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_field(payload, "reference")?;
    required_string(payload, "prompt")?;
    if !RATIOS[1..].contains(
        &payload
            .get("ratio")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    ) {
        return Err(invalid("video ratio is invalid"));
    }
    if !matches!(
        payload.get("duration").and_then(Value::as_i64),
        Some(4..=15)
    ) {
        return Err(invalid("video duration must be 4 to 15 seconds"));
    }
    if !matches!(
        payload.get("mode").and_then(Value::as_str),
        Some("normal" | "hd")
    ) {
        return Err(invalid("video mode must be normal or hd"));
    }
    Ok(())
}

pub(super) fn model_scene(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    image_urls(payload, 1, 10)?;
    required_string(payload, "prompt")?;
    if !RATIOS.contains(
        &payload
            .get("size")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    ) {
        return Err(invalid("model-scene size is invalid"));
    }
    if !matches!(
        payload.get("resolution").and_then(Value::as_str),
        Some("standard" | "4K")
    ) {
        return Err(invalid("model-scene resolution must be standard or 4K"));
    }
    Ok(())
}

fn validate_3d_views(views: &Value) -> Result<(), rmcp::ErrorData> {
    let views = views
        .as_array()
        .ok_or_else(|| invalid("MultiViewImages must be an array"))?;
    for view in views {
        let view = view
            .as_object()
            .ok_or_else(|| invalid("each 3D view must be an object"))?;
        validate_https(required_string(view, "ViewImageUrl")?)?;
    }
    Ok(())
}
