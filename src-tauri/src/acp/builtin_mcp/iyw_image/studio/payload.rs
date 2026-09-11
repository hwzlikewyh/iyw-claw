use serde_json::{json, Map, Value};

use super::super::{
    input::PreparedImage,
    invalid,
    validation::{image_urls, number_range, require_array, require_object, required_string},
    ImageRequest,
};

pub(super) fn prepare(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<Value, rmcp::ErrorData> {
    let mut body = request.parameters.clone();
    if ["token", "tokenInfo", "authorization", "cookie"]
        .iter()
        .any(|key| body.contains_key(*key))
    {
        return Err(invalid(
            "Image parameters must not contain authentication credentials",
        ));
    }
    if let Some(prompt) = request
        .prompt
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        body.entry("prompt").or_insert_with(|| json!(prompt.trim()));
    }
    if !images.is_empty() {
        if body.contains_key("imageUrls") {
            return Err(invalid("Provide images or parameters.imageUrls, not both"));
        }
        body.insert(
            "imageUrls".into(),
            json!(images.iter().map(|image| &image.url).collect::<Vec<_>>()),
        );
    }
    image_urls(&body, 1, if kind == "a-plus-edit" { 1 } else { 10 })?;
    match kind {
        "product-kit" | "a-plus" => product(&body, kind)?,
        "a-plus-edit" => a_plus_edit(&mut body)?,
        _ => batch(&mut body, kind)?,
    }
    Ok(Value::Object(body))
}

fn product(body: &Map<String, Value>, kind: &str) -> Result<(), rmcp::ErrorData> {
    for key in [
        "platform",
        "market",
        "language",
        "contentType",
        "resolution",
        "productInfo",
    ] {
        required_string(body, key)?;
    }
    require_array(body, "modules")?;
    if body["modules"].as_array().is_none_or(Vec::is_empty) {
        return Err(invalid(
            "modules must contain selected product-kit module keys",
        ));
    }
    if kind == "a-plus" && require_object(body, "selectedPlan")?.is_empty() {
        return Err(invalid(
            "selectedPlan must be the plan returned by product-kit/plan-a-plus",
        ));
    }
    Ok(())
}

fn a_plus_edit(body: &mut Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    required_string(body, "prompt")?;
    for (key, value) in [
        ("toolName", json!("variation")),
        ("toolType", json!(12)),
        ("modelChannel", json!(2)),
        ("batchSize", json!(1)),
        ("isChange", json!(1)),
    ] {
        if body.get(key).is_some_and(|given| given != &value) {
            return Err(invalid(format!("a-plus-edit fixes {key} to {value}")));
        }
        body.insert(key.into(), value);
    }
    body.entry("size").or_insert(json!("16:9"));
    body.entry("channelName").or_insert(json!("A+详情图编辑"));
    body.entry("remark").or_insert(json!("A+详情图单图编辑"));
    for key in ["size", "channelName"] {
        required_string(body, key)?;
    }
    for key in ["stats", "jsonData"] {
        if body.contains_key(key) {
            require_object(body, key)?;
        }
    }
    Ok(())
}

fn batch(body: &mut Map<String, Value>, kind: &str) -> Result<(), rmcp::ErrorData> {
    body.entry("batchSize").or_insert(json!(1));
    if !body["batchSize"].as_u64().is_some_and(|value| value > 0) {
        return Err(invalid(
            "batchSize must be a positive integer per source image",
        ));
    }
    match kind {
        "batch-generate" | "batch-series-extend" => {
            required_string(body, "prompt")?;
        }
        "batch-watermark" => {
            if !matches!(
                required_string(body, "target")?,
                "text" | "watermark" | "text_watermark"
            ) {
                return Err(invalid("target must be text, watermark, or text_watermark"));
            }
        }
        "batch-upscale" => {
            body.entry("scale").or_insert(json!(2));
            number_range(body, "scale", 2.0, 8.0)?;
        }
        "batch-enhance" => {
            body.entry("enhanceType").or_insert(json!(2));
            body.entry("model").or_insert(json!(0));
        }
        "batch-mockup" if body.get("mockupId").is_none_or(Value::is_null) => {
            return Err(invalid("batch-mockup requires an existing mockupId"))
        }
        _ => {}
    }
    Ok(())
}
