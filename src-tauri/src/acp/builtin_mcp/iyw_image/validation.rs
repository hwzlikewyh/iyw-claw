use serde_json::{Map, Value};

use super::invalid;
use super::validation_special;

pub(super) fn validate_payload(
    kind: &str,
    payload: &mut Map<String, Value>,
) -> Result<(), rmcp::ErrorData> {
    match kind {
        "variation" | "extend" => validate_g_tool(payload, kind, 1, true),
        "mix" => validate_mix(payload),
        "pattern-apply" => validate_pattern_apply(payload),
        "material-product" => validate_material_product(payload),
        "ip-apply" => validate_ip_apply(payload),
        "free-imitation" => validation_special::free_imitation(payload),
        "outpaint" => validation_special::outpaint(payload),
        "super-resolution" => validation_special::super_resolution(payload),
        "split-layers" => validation_special::split_layers(payload),
        "separate-layers" => validate_g_tool(payload, "seperate_layers", 1, false),
        "enhance" => validation_special::enhance(payload),
        "extract-pattern" => validate_g_tool(payload, "extract_pattern", 1, true),
        "repeat-horizontal" => validate_repeat_horizontal(payload),
        "convert" => validation_special::convert(payload),
        "line-extraction" => validation_special::line_extraction(payload),
        "color-transfer" => validation_special::color_transfer(payload),
        "image-to-3d" => validation_special::image_to_3d(payload),
        "video" => validation_special::video(payload),
        "model-scene" | "background" => validation_special::model_scene(payload),
        _ => Err(invalid(format!("unsupported image type: {kind}"))),
    }
}

fn validate_g_tool(
    payload: &mut Map<String, Value>,
    tool_name: &str,
    image_count: usize,
    prompt_required: bool,
) -> Result<(), rmcp::ErrorData> {
    let urls = image_urls(payload, image_count, image_count)?;
    payload.insert("toolName".to_string(), Value::String(tool_name.to_string()));
    if matches!(tool_name, "variation" | "extend" | "mix") {
        payload.insert("modelChannel".to_string(), Value::from(2));
    }
    payload.insert("imageUrls".to_string(), Value::String(urls[0].clone()));
    if prompt_required {
        required_string(payload, "prompt")?;
    }
    if matches!(tool_name, "variation" | "extend") {
        payload.insert("batchSize".to_string(), Value::from(1));
    }
    Ok(())
}

fn validate_mix(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    let urls = image_urls(payload, 2, 10)?;
    payload.insert("toolName".to_string(), Value::String("mix".to_string()));
    payload.insert("modelChannel".to_string(), Value::from(2));
    payload.insert(
        "imageUrls".to_string(),
        Value::Array(urls.into_iter().map(Value::String).collect()),
    );
    required_string(payload, "prompt")?;
    Ok(())
}

fn validate_pattern_apply(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    validate_g_tool(payload, "iyw_tu", 2, true)?;
    require_array(payload, "product")?;
    require_array(payload, "material")
}

fn validate_material_product(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    validate_g_tool(payload, "user_product", 2, true)?;
    require_object(payload, "product")?;
    require_array(payload, "material")
}

fn validate_ip_apply(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    validate_g_tool(payload, "iyw_ip", 1, true)?;
    require_object(payload, "product")?;
    require_object(payload, "jsonData").map(|_| ())
}

fn validate_repeat_horizontal(payload: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    let urls = image_urls(payload, 1, 1)?;
    payload.insert(
        "toolName".to_string(),
        Value::String("return_leftright".to_string()),
    );
    payload.insert(
        "imageUrls".to_string(),
        Value::Array(vec![Value::String(urls[0].clone())]),
    );
    Ok(())
}

pub(super) fn image_urls(
    payload: &Map<String, Value>,
    min: usize,
    max: usize,
) -> Result<Vec<String>, rmcp::ErrorData> {
    let value = payload
        .get("imageUrls")
        .ok_or_else(|| invalid("imageUrls is required"))?;
    let urls = match value {
        Value::String(url) => vec![url.clone()],
        Value::Array(values) => values
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid("imageUrls must contain URLs"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(invalid("imageUrls must contain URLs")),
    };
    if !(min..=max).contains(&urls.len()) {
        return Err(invalid(format!(
            "imageUrls must contain {min} to {max} image URLs"
        )));
    }
    for url in &urls {
        validate_https(url)?;
    }
    Ok(urls)
}

pub(super) fn image_field(payload: &Map<String, Value>, key: &str) -> Result<(), rmcp::ErrorData> {
    validate_https(required_string(payload, key)?)
}

pub(super) fn validate_https(value: &str) -> Result<(), rmcp::ErrorData> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid("image URL is invalid"))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err(invalid("image URL must use credential-free HTTPS"));
    }
    Ok(())
}

pub(super) fn required_string<'a>(
    payload: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, rmcp::ErrorData> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid(format!("{key} must be a non-empty string")))
}

pub(super) fn require_object<'a>(
    payload: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, rmcp::ErrorData> {
    payload
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| invalid(format!("{key} must be an object")))
}

pub(super) fn require_array(
    payload: &Map<String, Value>,
    key: &str,
) -> Result<(), rmcp::ErrorData> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(format!("{key} must be an array")))?;
    Ok(())
}

pub(super) fn nonnegative_number(
    payload: &Map<String, Value>,
    key: &str,
) -> Result<(), rmcp::ErrorData> {
    number_range(payload, key, 0.0, f64::MAX)
}

pub(super) fn number_range(
    payload: &Map<String, Value>,
    key: &str,
    min: f64,
    max: f64,
) -> Result<(), rmcp::ErrorData> {
    let value = payload
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| *value >= min && *value <= max);
    value.ok_or_else(|| invalid(format!("{key} is outside the supported range")))?;
    Ok(())
}
