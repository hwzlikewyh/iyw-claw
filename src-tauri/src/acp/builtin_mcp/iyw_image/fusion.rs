use reqwest::multipart::{Form, Part};
use serde_json::{json, Map, Value};

use super::super::iyw_image_models::{
    display_name, is_model_ref, load_catalog, model_ref, supports_operation,
};
use super::input::PreparedImage;
use super::result::{materialize_fusion_images, value_to_form_text};
use super::{invalid, required_prompt, ImageRequest, ImageResult, IywGatewayService};

mod capabilities;
mod input;
mod parameters;
pub(super) use input::prepare_edit_images;
pub(in crate::acp::builtin_mcp) use parameters::schema as parameters_schema;
pub(super) use parameters::validate as validate_parameters;

pub(super) async fn generate(
    service: &IywGatewayService,
    request: &ImageRequest,
) -> Result<ImageResult, rmcp::ErrorData> {
    let prompt = required_prompt(request.prompt.as_deref())?;
    let (model_id, model_name) = select_model(service, request, ("generate", &[])).await?;
    let mut payload = request
        .parameters
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<_, _>>();
    payload.insert("model".to_string(), Value::String(model_id));
    payload.insert("prompt".to_string(), Value::String(prompt));
    let response = service
        .post_fusion("images/generations", Value::Object(payload))
        .await
        .map_err(|error| super::errors::model_failure(error, &model_name))?;
    Ok(ImageResult {
        operation: "generate".to_string(),
        status: "succeeded".to_string(),
        task_id: None,
        images: materialize_fusion_images(service, &response).await?,
        metadata: json!({"model_name": model_name, "request_id": response.get("_iyw_request_id")}),
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
    let (model_id, model_name) = select_model(service, request, ("edit", images)).await?;
    let form = build_edit_form(service, (request, images), (model_id, prompt)).await?;
    let response = service
        .post_fusion_multipart("images/edits", form)
        .await
        .map_err(|error| super::errors::model_failure(error, &model_name))?;
    Ok(ImageResult {
        operation: "edit".to_string(),
        status: "succeeded".to_string(),
        task_id: None,
        images: materialize_fusion_images(service, &response).await?,
        metadata: json!({"model_name": model_name, "request_id": response.get("_iyw_request_id")}),
    })
}

async fn build_edit_form(
    service: &IywGatewayService,
    (request, images): (&ImageRequest, &[PreparedImage]),
    (model, prompt): (String, String),
) -> Result<Form, rmcp::ErrorData> {
    let mut form = Form::new().text("model", model).text("prompt", prompt);
    let image_count = images.iter().filter(|image| image.role != "mask").count();
    for (index, image) in images.iter().enumerate() {
        form = form.part(
            edit_part_name(image, image_count),
            image_part(service, image, index).await?,
        );
    }
    for (key, value) in &request.parameters {
        if !matches!(key.as_str(), "model" | "prompt" | "mask") && !value.is_null() {
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
    let mime = crate::acp::delegation::image_format::detect_mime(&bytes)
        .ok_or_else(|| invalid("edit input image format is unsupported"))?;
    let extension = match mime {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    Part::bytes(bytes)
        .file_name(format!("input-{}.{}", index + 1, extension))
        .mime_str(mime)
        .map_err(|error| invalid(error.to_string()))
}

fn edit_part_name(image: &PreparedImage, image_count: usize) -> &'static str {
    if image.role == "mask" {
        "mask"
    } else if image_count > 1 {
        "image[]"
    } else {
        "image"
    }
}

async fn select_model(
    service: &IywGatewayService,
    request: &ImageRequest,
    (kind, images): (&str, &[PreparedImage]),
) -> Result<(String, String), rmcp::ErrorData> {
    let models = load_catalog(service).await?;
    let editing = kind == "edit";
    let selected = resolve_model(&models, &request.parameters, editing)?;
    validate_capabilities(&models, request, (kind, images))?;
    tracing::info!(target: "builtin_mcp", model_id = selected.0, editing, agent_selected = true,
        "selected Fusion image model");
    Ok(selected)
}

pub(super) fn resolve_model(
    models: &[Value],
    parameters: &Map<String, Value>,
    editing: bool,
) -> Result<(String, String), rmcp::ErrorData> {
    let requested = requested_model(parameters)?;
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

pub(super) fn requested_model(parameters: &Map<String, Value>) -> Result<&str, rmcp::ErrorData> {
    parameters
        .get("model")
        .and_then(Value::as_str)
        .filter(|reference| is_model_ref(reference))
        .ok_or_else(|| {
            tracing::warn!(
                target: "builtin_mcp",
                "[iyw-image] Fusion request rejected before execution: opaque model_ref required"
            );
            invalid("generate (including auto without images) and edit require an opaque model_ref in parameters.model. Call list_iyw_image_models, select a model_ref supporting image_generation for generate or image_editing for edit, and copy it unchanged into parameters.model. Raw model IDs and display names are rejected. No prior platform failure is required and no default model is selected. edit also requires a source image")
        })
}

// 候选预检与实际调用共享同一份目录声明。
pub(super) fn validate_capabilities(
    models: &[Value],
    request: &ImageRequest,
    (kind, images): (&str, &[PreparedImage]),
) -> Result<(), rmcp::ErrorData> {
    let reference = requested_model(&request.parameters)?;
    if let Some(model) = models
        .iter()
        .find(|model| model_ref(model).as_deref() == Some(reference))
    {
        capabilities::validate(model, request, (kind, images))?;
    }
    Ok(())
}
