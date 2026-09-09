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
        "description": "List available Fusion image models before falling back from a confirmed IYW platform failure to generate_iyw_image type=generate (images/generations) or type=edit (images/edits). IYW platform operations have priority; fission, variation, extend, mix, and auto do not need this catalog. Call with {}. Returns model IDs, display names, descriptions, generation/editing capabilities, and prices. Choose a model according to the user's task and preference; require image_generation=true for generate or image_editing=true for edit, then copy its exact id to parameters.model. Do not guess model IDs or infer capabilities from names. Reuse this catalog for the same task or batch; refresh when availability changes. An empty catalog or failed lookup supplies no model. This read-only tool does not generate images, upload inputs, or need capability search/read.",
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
        "display_name": model["display_name"],
        "description": model["description"],
        "owned_by": model["owned_by"],
        "capabilities": {
            "image_generation": supports_operation(model, false),
            "image_editing": supports_operation(model, true)
        },
        "prices": model["prices"]
    })
}
