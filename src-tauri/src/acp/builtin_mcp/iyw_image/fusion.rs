use serde_json::{json, Map, Value};

use super::super::iyw_image_models::{
    display_name, is_model_ref, load_catalog, model_ref, refresh_catalog, supports_operation,
};
use super::input::PreparedImage;
use super::{invalid, required_prompt, ImageRequest, ImageResult, IywGatewayService};

mod capabilities;
mod input;
mod output;
mod parameters;
pub(super) use input::prepare_edit_images;
pub(in crate::acp::builtin_mcp) use parameters::schema as parameters_schema;
pub(super) use parameters::validate as validate_parameters;

pub(super) async fn generate(
    service: &IywGatewayService,
    request: &ImageRequest,
) -> Result<ImageResult, rmcp::ErrorData> {
    let prompt = required_prompt(request.prompt.as_deref())?;
    let selected = if has_model_parameter(&request.parameters) {
        let selected = select_model(service, &request.parameters, false).await?;
        let models = load_catalog(service).await?;
        validate_capabilities(&models, request, ("generate", &[]))?;
        Some(selected)
    } else {
        None
    };
    let mut payload = request.parameters.clone();
    if let Some((model_id, _)) = &selected {
        payload.insert("model".to_string(), Value::String(model_id.clone()));
    }
    payload.insert("prompt".to_string(), Value::String(prompt));
    payload.entry("n").or_insert(Value::from(1));
    let model_name = selected
        .as_ref()
        .map(|(_, name)| name.as_str())
        .unwrap_or("image processing");
    let response = service
        .post_fusion("images/generations", Value::Object(payload))
        .await
        .map_err(|error| super::errors::model_failure(error, model_name))?;
    output::finish(service, "generate", (model_name, &response)).await
}

pub(super) async fn edit(
    service: &IywGatewayService,
    request: &ImageRequest,
    images: &[PreparedImage],
) -> Result<ImageResult, rmcp::ErrorData> {
    let prompt = required_prompt(request.prompt.as_deref())?;
    if images.is_empty() {
        return Err(invalid("edit requires at least one image"));
    }
    let selected = if has_model_parameter(&request.parameters) {
        let selected = select_model(service, &request.parameters, true).await?;
        let models = load_catalog(service).await?;
        validate_capabilities(&models, request, ("edit", images))?;
        Some(selected)
    } else {
        None
    };
    let model_name = selected
        .as_ref()
        .map(|(_, name)| name.as_str())
        .unwrap_or("image processing");
    let model_id = selected.as_ref().map(|(model, _)| model.clone());
    let payload = build_edit_payload((request, images), (model_id, prompt))?;
    tracing::info!(
        image_count = images.iter().filter(|image| image.role != "mask").count(),
        has_mask = payload.get("mask").is_some(),
        transport = "json_image_url",
        "[iyw-image] submitting Fusion edit with image URLs"
    );
    let response = service
        .post_fusion("images/edits", payload)
        .await
        .map_err(|error| super::errors::model_failure(error, model_name))?;
    output::finish(service, "edit", (model_name, &response)).await
}

fn build_edit_payload(
    (request, images): (&ImageRequest, &[PreparedImage]),
    (model, prompt): (Option<String>, String),
) -> Result<Value, rmcp::ErrorData> {
    let mut payload = request.parameters.clone();
    payload.retain(|key, value| key != "mask" && !value.is_null());
    if let Some(model) = model {
        payload.insert("model".to_string(), Value::String(model));
    }
    payload.insert("prompt".to_string(), Value::String(prompt));
    let mut sources = Vec::new();
    for image in images {
        if image.url.is_empty() {
            return Err(invalid("edit input must be uploaded before generation"));
        }
        let reference = json!({"image_url": image.url});
        if image.role == "mask" {
            payload.insert("mask".to_string(), reference);
        } else {
            sources.push(reference);
        }
    }
    payload.insert("images".to_string(), Value::Array(sources));
    Ok(Value::Object(payload))
}

pub(super) async fn select_model(
    service: &IywGatewayService,
    parameters: &Map<String, Value>,
    editing: bool,
) -> Result<(String, String), rmcp::ErrorData> {
    let requested = requested_model(parameters)?.ok_or_else(|| {
        invalid("an explicit model_ref is required when selecting a Fusion image model")
    })?;
    let mut models = load_catalog(service).await?;
    if !models.iter().any(|item| {
        supports_operation(item, editing) && model_ref(item).as_deref() == Some(requested)
    }) {
        models = refresh_catalog(service, &models).await?;
    }
    let selected = resolve_model(&models, parameters, editing)?;
    tracing::info!(target: "builtin_mcp", model_id = selected.0, editing, agent_selected = true,
        "selected Fusion image model");
    Ok(selected)
}

pub(super) fn resolve_model(
    models: &[Value],
    parameters: &Map<String, Value>,
    editing: bool,
) -> Result<(String, String), rmcp::ErrorData> {
    let requested = requested_model(parameters)?.ok_or_else(|| {
        invalid("an explicit model_ref is required when selecting a Fusion image model")
    })?;
    let model = models
        .iter()
        .find(|item| {
            supports_operation(item, editing)
                && model_ref(item).as_deref() == Some(requested)
        })
        .ok_or_else(|| {
            tracing::warn!(editing, execution_status = "not_started", "[iyw-image] model reference unavailable for requested operation");
            invalid("parameters.model is unavailable for this image operation. No generation was started. Refresh list_iyw_image_models once and select its current model_ref supporting the operation; never use a raw model ID or display name")
        })?;
    let id = model
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let id = id.ok_or_else(|| {
        rmcp::ErrorData::internal_error("Image model configuration is invalid", None)
    })?;
    let name = display_name(model, models);
    Ok((id.to_string(), name.to_string()))
}

pub(super) fn requested_model(
    parameters: &Map<String, Value>,
) -> Result<Option<&str>, rmcp::ErrorData> {
    let Some(value) = parameters.get("model").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    value
        .as_str()
        .filter(|reference| is_model_ref(reference))
        .map(Some)
        .ok_or_else(|| {
            tracing::warn!(
                target: "builtin_mcp",
                "[iyw-image] Fusion request rejected before execution: opaque model_ref required"
            );
            invalid("parameters.model must be an opaque model_ref selected from list_iyw_image_models, or omitted to use the configured default image model. Raw model IDs and display names are rejected")
        })
}

fn has_model_parameter(parameters: &Map<String, Value>) -> bool {
    parameters
        .get("model")
        .is_some_and(|value| !value.is_null())
}

// 候选预检与实际调用共享同一份目录声明。
pub(super) fn validate_capabilities(
    models: &[Value],
    request: &ImageRequest,
    (kind, images): (&str, &[PreparedImage]),
) -> Result<(), rmcp::ErrorData> {
    let Some(reference) = requested_model(&request.parameters)? else {
        return Ok(());
    };
    if let Some(model) = models
        .iter()
        .find(|model| model_ref(model).as_deref() == Some(reference))
    {
        capabilities::validate(model, request, (kind, images))?;
    }
    Ok(())
}
