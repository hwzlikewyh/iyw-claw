use serde_json::{Map, Value};

use super::super::{
    invalid,
    validation::{image_field, image_urls, required_string},
};

pub(super) fn validate(kind: &str, payload: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    if ["token", "authorization", "cookie", "refreshToken"]
        .iter()
        .any(|key| payload.contains_key(*key))
    {
        return Err(invalid(
            "Image parameters must not contain authentication credentials",
        ));
    }
    validate_input(kind, payload)?;
    if matches!(
        kind,
        "modify"
            | "seed-edit"
            | "video-auto-director"
            | "video-remake-director"
            | "micro-variation"
    ) {
        required_string(payload, "prompt")?;
    }
    if matches!(kind, "erase" | "watermark-erase") {
        image_field(payload, "mask")?;
    }
    if kind == "bleed-line" && !payload.contains_key("size") {
        return Err(invalid(
            "bleed-line requires the size from the documented page request",
        ));
    }
    optional_types(payload)
}

fn validate_input(kind: &str, payload: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    match kind {
        "classify-intent" | "save-color" | "f-tools" => validate_data(kind, payload)?,
        "scheme-generate" => {
            required_id(payload, "schemeId")?;
            required_string(payload, "prompt")?;
        }
        "micro-generate" | "faddish" => {
            required_string(payload, "prompt")?;
            optional_images(payload)?;
        }
        "background-remove" | "micro-upscale" | "micro-upscale-image" => {
            image_field(payload, "imageUrl")?
        }
        "check-image" => image_field(payload, "image")?,
        "g-tools" => validate_g_tools(payload)?,
        "blend" => {
            image_urls(payload, 2, 10)?;
        }
        _ => {
            image_urls(payload, 1, 10)?;
        }
    }
    Ok(())
}

fn validate_data(kind: &str, payload: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    match kind {
        "classify-intent" => required_array(payload, "keys"),
        "save-color" => required_array(payload, "colors"),
        _ if payload.get("content").is_none_or(Value::is_null) => Err(invalid(
            "f-tools requires content from the documented page request",
        )),
        _ => Ok(()),
    }
}

fn validate_g_tools(payload: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    required_string(payload, "prompt")?;
    if let Some(images) = payload.get("images") {
        let mut urls = Map::new();
        urls.insert("imageUrls".to_string(), images.clone());
        image_urls(&urls, 1, 10)?;
    }
    Ok(())
}

fn optional_images(payload: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    if payload.contains_key("imageUrls") {
        image_urls(payload, 1, 10)?;
    }
    Ok(())
}

fn required_array(payload: &Map<String, Value>, key: &str) -> Result<(), rmcp::ErrorData> {
    if !payload
        .get(key)
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
    {
        return Err(invalid(format!(
            "{key} must be a non-empty array; copy the documented item structure"
        )));
    }
    Ok(())
}

fn required_id(payload: &Map<String, Value>, key: &str) -> Result<(), rmcp::ErrorData> {
    match payload.get(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(()),
        Some(Value::Number(value)) if value.as_u64().is_some_and(|id| id > 0) => Ok(()),
        _ => Err(invalid(format!("{key} must be an existing identifier"))),
    }
}

fn optional_types(payload: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    for key in ["upscale", "strength", "duration"] {
        if payload
            .get(key)
            .is_some_and(|value| !value.as_f64().is_some_and(|number| number >= 0.0))
        {
            return Err(invalid(format!("{key} must be a non-negative number")));
        }
    }
    for key in ["ratio", "mode", "style"] {
        if payload.contains_key(key) {
            required_string(payload, key)?;
        }
    }
    if payload
        .get("batchSize")
        .is_some_and(|value| !value.as_u64().is_some_and(|count| count > 0))
    {
        return Err(invalid("batchSize must be a positive integer"));
    }
    Ok(())
}
