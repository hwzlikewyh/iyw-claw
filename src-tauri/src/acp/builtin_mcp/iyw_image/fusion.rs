use reqwest::multipart::{Form, Part};
use serde_json::{json, Map, Value};

use super::super::iyw_image_models::{load_catalog, supports_operation};
use super::input::PreparedImage;
use super::result::{materialize_fusion_images, value_to_form_text};
use super::{invalid, required_prompt, ImageRequest, ImageResult, IywGatewayService};

pub(super) async fn generate(
    service: &IywGatewayService,
    request: &ImageRequest,
) -> Result<ImageResult, rmcp::ErrorData> {
    let prompt = required_prompt(request.prompt.as_deref())?;
    let (model_id, model_name) = select_model(service, &request.parameters, false).await?;
    let mut payload = request.parameters.clone();
    payload.insert("model".to_string(), Value::String(model_id));
    payload.insert("prompt".to_string(), Value::String(prompt));
    payload.entry("n").or_insert(Value::from(1));
    let response = service
        .post_fusion("images/generations", Value::Object(payload))
        .await?;
    Ok(ImageResult {
        operation: "generate".to_string(),
        status: "succeeded".to_string(),
        task_id: None,
        images: materialize_fusion_images(service, &response).await?,
        metadata: json!({"model_name": model_name}),
    })
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
    let (model_id, model_name) = select_model(service, &request.parameters, true).await?;
    let form = build_edit_form(service, request, images, model_id, prompt).await?;
    let response = service.post_fusion_multipart("images/edits", form).await?;
    Ok(ImageResult {
        operation: "edit".to_string(),
        status: "succeeded".to_string(),
        task_id: None,
        images: materialize_fusion_images(service, &response).await?,
        metadata: json!({"model_name": model_name}),
    })
}

async fn build_edit_form(
    service: &IywGatewayService,
    request: &ImageRequest,
    images: &[PreparedImage],
    model: String,
    prompt: String,
) -> Result<Form, rmcp::ErrorData> {
    let mut form = Form::new().text("model", model).text("prompt", prompt);
    for (index, image) in images.iter().enumerate() {
        form = form.part(
            edit_part_name(image),
            image_part(service, image, index).await?,
        );
    }
    for (key, value) in &request.parameters {
        if key != "model" {
            form = form.text(key.clone(), value_to_form_text(value));
        }
    }
    Ok(form)
}

async fn image_part(
    service: &IywGatewayService,
    image: &PreparedImage,
    index: usize,
) -> Result<Part, rmcp::ErrorData> {
    let bytes = match &image.bytes {
        Some(bytes) => bytes.clone(),
        None => service.download_image(&image.url).await?,
    };
    Part::bytes(bytes)
        .file_name(format!("input-{}.png", index + 1))
        .mime_str(image.mime_type.as_deref().unwrap_or("image/png"))
        .map_err(|error| invalid(error.to_string()))
}

fn edit_part_name(image: &PreparedImage) -> &'static str {
    if image.role == "mask" {
        "mask"
    } else {
        "image"
    }
}

async fn select_model(
    service: &IywGatewayService,
    parameters: &Map<String, Value>,
    editing: bool,
) -> Result<(String, String), rmcp::ErrorData> {
    let requested = requested_model(parameters)?;
    let models = load_catalog(service).await?;
    let model = models
        .iter()
        .find(|item| {
            supports_operation(item, editing)
                && item.get("id").and_then(Value::as_str) == Some(requested)
        })
        .ok_or_else(|| {
            invalid("parameters.model must be an exact ID from list_iyw_image_models supporting this image operation")
        })?;
    let id = model
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let id = id
        .ok_or_else(|| rmcp::ErrorData::internal_error("Fusion image model ID is missing", None))?;
    let name = model
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("selected image model");
    tracing::info!(
        target: "builtin_mcp",
        model_id = id,
        editing,
        agent_selected = true,
        "selected Fusion image model"
    );
    Ok((id.to_string(), name.to_string()))
}

pub(super) fn requested_model(parameters: &Map<String, Value>) -> Result<&str, rmcp::ErrorData> {
    parameters
        .get("model")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            tracing::warn!(
                target: "builtin_mcp",
                "[iyw-image] Fusion request rejected before execution: explicit model ID required"
            );
            invalid("generate (including auto without images) and edit require parameters.model. Call list_iyw_image_models, select an exact ID supporting image_generation for generate or image_editing for edit, and pass it in parameters.model. No prior platform failure is required and no default model is selected. edit also requires a source image")
        })
}
