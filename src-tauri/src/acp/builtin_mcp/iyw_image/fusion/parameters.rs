use serde_json::{json, Map, Value};

use super::super::{invalid, ImageRequest, PreparedImage};

const MAX_IMAGES: usize = 16;
const MAX_OUTPUT_COUNT: u64 = 10;
const MAX_COMPRESSION: u64 = 100;
const STRING_FIELDS: [&str; 10] = [
    "quality",
    "size",
    "background",
    "output_format",
    "response_format",
    "moderation",
    "input_fidelity",
    "style",
    "user",
    "mask",
];

pub(in crate::acp::builtin_mcp::iyw_image) fn validate(
    request: &ImageRequest,
    kind: &str,
    images: &[PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    let parameters = &request.parameters;
    for field in STRING_FIELDS {
        if parameters.get(field).is_some_and(|value| {
            !value.is_null() && value.as_str().is_none_or(|text| text.trim().is_empty())
        }) {
            return Err(invalid(format!(
                "parameters.{field} must be a non-blank string"
            )));
        }
    }
    integer_range(parameters, "n", 1..=MAX_OUTPUT_COUNT)?;
    integer_range(parameters, "output_compression", 0..=MAX_COMPRESSION)?;
    validate_response_mode(parameters)?;
    validate_output(parameters)?;
    if ["image", "images", "image[]"]
        .iter()
        .any(|key| parameters.contains_key(*key))
    {
        return Err(invalid(
            "Provide Fusion source images in images, not parameters.image/images",
        ));
    }
    if kind == "generate"
        && ["mask", "input_fidelity"]
            .iter()
            .any(|key| parameters.get(*key).is_some_and(|value| !value.is_null()))
    {
        return Err(invalid("mask and input_fidelity require type=edit"));
    }
    if kind == "edit" {
        let mask_count = images.iter().filter(|image| image.role == "mask").count();
        if mask_count > 1 || images.len() == mask_count || images.len() - mask_count > MAX_IMAGES {
            return Err(invalid(
                "edit accepts 1-16 source images and at most one mask",
            ));
        }
    }
    Ok(())
}

fn integer_range(
    parameters: &Map<String, Value>,
    field: &str,
    range: std::ops::RangeInclusive<u64>,
) -> Result<(), rmcp::ErrorData> {
    if let Some(value) = parameters.get(field).filter(|value| !value.is_null()) {
        if !value.as_u64().is_some_and(|value| range.contains(&value)) {
            return Err(invalid(format!(
                "parameters.{field} must be an integer in {range:?}"
            )));
        }
    }
    Ok(())
}

fn validate_response_mode(parameters: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    if parameters
        .get("stream")
        .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
    {
        return Err(invalid("Fusion Images relay supports complete JSON responses only; stream must be false or omitted"));
    }
    integer_range(parameters, "partial_images", 0..=0)
}

fn validate_output(parameters: &Map<String, Value>) -> Result<(), rmcp::ErrorData> {
    let format = parameters
        .get("output_format")
        .and_then(Value::as_str)
        .unwrap_or("png");
    if parameters.get("background").and_then(Value::as_str) == Some("transparent")
        && !matches!(format, "png" | "webp")
    {
        return Err(invalid(
            "transparent background requires png or webp output",
        ));
    }
    if parameters
        .get("output_compression")
        .is_some_and(|value| !value.is_null())
        && !matches!(format, "jpeg" | "webp")
    {
        return Err(invalid(
            "output_compression requires explicit output_format=jpeg or webp",
        ));
    }
    Ok(())
}

pub(in crate::acp::builtin_mcp) fn schema() -> Value {
    json!({
        "quality": {"type": ["string", "null"], "description": "generate/edit: low, medium, high or auto for compatible image models; legacy models may use standard/hd. Model-specific support comes from its contract; no default is forced."},
        "size": {"type": ["string", "null"], "description": "generate/edit: auto or WIDTHxHEIGHT, e.g. 1024x1024, 1536x1024, 1024x1536; flexible/large dimensions require selected-model support. Platform operations retain their own ratio/size contract."},
        "n": {"type": ["integer", "null"], "minimum": 1, "maximum": MAX_OUTPUT_COUNT, "description": "Single generate/edit upstream output count, if supported. For independent tasks use requests; for repeated executions use count. n cannot be combined with count, requests or fallbacks."},
        "background": {"type": ["string", "null"], "description": "generate/edit: transparent, opaque or auto where supported. Transparency requires png/webp and a model supporting it."},
        "output_format": {"type": ["string", "null"], "description": "generate/edit: png, jpeg or webp for compatible models."},
        "output_compression": {"type": ["integer", "null"], "minimum": 0, "maximum": MAX_COMPRESSION, "description": "generate/edit: 0-100, only with explicit jpeg/webp output."},
        "response_format": {"type": ["string", "null"], "description": "url or b64_json only when the selected model supports this parameter. Distinct from output_format. Do not force it on models that return Base64 by default."},
        "moderation": {"type": ["string", "null"], "description": "Generation filtering level auto or low where supported; edit support requires its channel contract."},
        "input_fidelity": {"type": ["string", "null"], "description": "edit only: high or low for models supporting this option. Do not assume all editing models accept it."},
        "style": {"type": ["string", "null"], "description": "Generation style vivid or natural for models supporting it; not a universal GPT Image option."},
        "user": {"type": ["string", "null"], "description": "Optional opaque end-user identifier. Do not send personal information or authentication credentials."},
        "stream": {"type": ["boolean", "null"], "enum": [false, null], "description": "Only false or null accepted: the current Fusion Images relay returns complete JSON, not SSE."},
        "partial_images": {"type": ["integer", "null"], "minimum": 0, "maximum": 0, "description": "Only omitted/null/0: partial image streaming is unavailable in the current relay."}
    })
}
