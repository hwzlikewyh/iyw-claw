use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::{json, Value};

use super::iyw_service::IywGatewayService;
use super::tool_identity::IMAGE_MODELS_TOOL;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListModelsRequest {}

pub(super) fn tool() -> Value {
    json!({
        "name": IMAGE_MODELS_TOOL,
        "description": concat!(
            "List available Fusion image models with {} before generate_iyw_image type=generate (images/generations), auto without images, or explicit edit (images/edits). ",
            "Also evaluate this catalog when no specialized platform operation matches the user's visual transformation or a relevant platform route has confirmed failure. A missing operation name or sparse description is not proof of unsupported editing. ",
            "Choose from returned descriptions, capabilities, prices and user requirements: image_generation=true for generate, image_editing=true for edit. For source images with a selected model use edit and preserve the references. Neither route requires a prior platform attempt or failure. ",
            "Copy the exact id only into parameters.model; use display_name or a generic image-processing name in user-facing replies, progress and delivery descriptions. Never append internal model/provider IDs or backend names. ",
            "Do not guess IDs or infer technical guarantees from names/flags. Verify constraints such as masks, transparency, bit depth, reference counts and resolution against documentation; do not invent stricter requirements or discard real ones. A supported visual edit may be expressed in the prompt without a same-named API. ",
            "Platform variation, mix and extend need no Fusion catalog; passing a catalog model to those operations does not select that Fusion model. Claim a model only when the actual result confirms it. ",
            "Reuse the catalog for the same task or batch; refresh when availability changes. An empty/failed lookup supplies no model. Before manual processing require concrete limitations of both applicable platform and model routes; do not enumerate unrelated models. ",
            "This read-only tool does not generate images, upload inputs, or need capability search/read."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        },
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": true
        }
    })
}

pub(super) async fn list(
    service: &IywGatewayService,
    arguments: Value,
) -> Result<CallToolResult, ErrorData> {
    let _: ListModelsRequest = serde_json::from_value(arguments)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    let models = load_catalog(service)
        .await?
        .iter()
        .filter(|model| supports_operation(model, false) || supports_operation(model, true))
        .map(model_summary)
        .collect::<Vec<_>>();
    tracing::info!(target: "builtin_mcp", model_count = models.len(), "listed Fusion image models");
    Ok(CallToolResult::structured(json!({
        "object": "list",
        "model_type": "image",
        "data": models
    })))
}

pub(super) async fn load_catalog(service: &IywGatewayService) -> Result<Vec<Value>, ErrorData> {
    let catalog = service
        .get_fusion("models", &[("model_type", "image")])
        .await?;
    catalog
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("Fusion image model catalog is invalid", None))
}

pub(super) fn supports_operation(item: &Value, editing: bool) -> bool {
    let capability = if editing {
        "image_editing"
    } else {
        "image_generation"
    };
    item.get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty())
        && item
            .get("capabilities")
            .and_then(|caps| caps.get(capability))
            .and_then(Value::as_bool)
            == Some(true)
}

fn model_summary(model: &Value) -> Value {
    json!({
        "id": model["id"],
        "display_name": display_name(model),
        "description": model["description"],
        "owned_by": model["owned_by"],
        "capabilities": {
            "image_generation": supports_operation(model, false),
            "image_editing": supports_operation(model, true)
        },
        "prices": model["prices"]
    })
}

pub(super) fn display_name(model: &Value) -> &str {
    let id = model.get("id").and_then(Value::as_str).unwrap_or("").trim();
    let name = model
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty()
        || (!id.is_empty() && name.to_ascii_lowercase().contains(&id.to_ascii_lowercase()))
    {
        "image processing"
    } else {
        name
    }
}
