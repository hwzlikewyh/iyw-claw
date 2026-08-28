use super::model_catalog::layer_from_models;
use super::model_catalog_types::{
    ImageInputMode, ModelCapabilities, ModelCatalogLayer, ModelLimits, PersistedModel,
};

pub(super) fn layer_from_payload(payload: &serde_json::Value) -> Option<ModelCatalogLayer> {
    let entries = payload.get("data")?.as_array()?;
    let models = entries.iter().filter_map(parse_model).collect::<Vec<_>>();
    if !entries.is_empty() && models.is_empty() {
        return None;
    }
    Some(layer_from_models(models))
}

fn parse_model(value: &serde_json::Value) -> Option<PersistedModel> {
    let id = value.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let capabilities = value
        .get("capabilities")
        .and_then(serde_json::Value::as_object)
        .map(parse_capabilities)
        .unwrap_or_default();
    let image_input_mode = value
        .pointer("/image_input/mode")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_image_input_mode)
        .unwrap_or(if capabilities.vision {
            ImageInputMode::Native
        } else {
            ImageInputMode::None
        });
    let limits = value
        .get("limits")
        .and_then(serde_json::Value::as_object)
        .map(parse_limits)
        .unwrap_or_default();
    Some(PersistedModel {
        id: id.to_string(),
        capabilities,
        image_input_mode,
        limits,
    })
}

fn parse_limits(value: &serde_json::Map<String, serde_json::Value>) -> ModelLimits {
    let limit = |key: &str| value.get(key).and_then(serde_json::Value::as_u64);
    ModelLimits {
        context_window: limit("context_window"),
        max_input_tokens: limit("max_input_tokens"),
        max_output_tokens: limit("max_output_tokens"),
        compaction_at_tokens: limit("compaction_at_tokens"),
    }
}

fn parse_capabilities(value: &serde_json::Map<String, serde_json::Value>) -> ModelCapabilities {
    let enabled = |key: &str| value.get(key).and_then(serde_json::Value::as_bool) == Some(true);
    ModelCapabilities {
        streaming: enabled("streaming"),
        tool_calling: enabled("tool_calling"),
        parallel_tool_calling: enabled("parallel_tool_calling"),
        web_search: enabled("web_search"),
        vision: enabled("vision"),
        audio_input: enabled("audio_input"),
        structured_output: enabled("structured_output"),
        prompt_cache: enabled("prompt_cache"),
        image_generation: enabled("image_generation"),
        image_editing: enabled("image_editing"),
    }
}

fn parse_image_input_mode(value: &str) -> Option<ImageInputMode> {
    match value {
        "native" => Some(ImageInputMode::Native),
        "fallback" => Some(ImageInputMode::Fallback),
        "none" => Some(ImageInputMode::None),
        _ => None,
    }
}
