use std::sync::OnceLock;

use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::iyw_service::IywGatewayService;
use super::tool_identity::IMAGE_MODELS_TOOL;

const MODEL_REF_PREFIX: &str = "image_model_";
const MODEL_REF_DIGEST_LENGTH: usize = 64;
static MODEL_REF_SALT: OnceLock<uuid::Uuid> = OnceLock::new();

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListModelsRequest {}

pub(super) fn tool() -> Value {
    json!({
        "name": IMAGE_MODELS_TOOL,
        "description": concat!(
            "List available Fusion image models with {} before generate_iyw_image type=generate (images/generations), auto without images, or explicit edit (images/edits). ",
            "Also evaluate this catalog when no specialized platform operation matches the user's visual transformation or a relevant platform route has confirmed failure. A missing operation name or sparse description is not proof of unsupported editing. ",
            "Choose from returned descriptions, capabilities and user requirements: image_generation=true for generate, image_editing=true for edit. For source images with a selected model use edit and preserve the references. Neither route requires a prior platform attempt or failure. ",
            "Copy the opaque model_ref only into parameters.model; use display_name or a generic image-processing name in user-facing replies, progress, questions and delivery descriptions. Never expose model_ref, prices, currency amounts, provider names, backend names or internal model IDs, even when asked about routing, cost or model choice. Only report points explicitly confirmed by the platform; never calculate points from prices. ",
            "Do not guess references or infer technical guarantees from names/flags. Verify constraints such as masks, transparency, bit depth, reference counts and resolution against documentation; do not invent stricter requirements or discard real ones. A supported visual edit may be expressed in the prompt without a same-named API. ",
            "Platform variation, mix and extend need no Fusion catalog; passing a catalog model to those operations does not select that Fusion model. Claim a model only when the actual result confirms it. ",
            "Reuse the catalog for the same task or batch; refresh when availability changes or a reference is rejected after an application restart. References are stable only within the current host process; raw model IDs and display names are not accepted as references. An empty/failed lookup supplies no model. Before manual processing require concrete limitations of both applicable platform and model routes; do not enumerate unrelated models. ",
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
    let catalog = load_catalog(service).await?;
    let models = catalog
        .iter()
        .filter(|model| supports_operation(model, false) || supports_operation(model, true))
        .map(|model| model_summary(model, &catalog))
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

fn model_summary(model: &Value, catalog: &[Value]) -> Value {
    let mut summary = json!({
        "model_ref": model_ref(model),
        "display_name": display_name(model, catalog),
        "capabilities": {
            "image_generation": supports_operation(model, false),
            "image_editing": supports_operation(model, true)
        }
    });
    if let Some(description) = public_text(model, "description", catalog) {
        summary["description"] = Value::String(description.to_string());
    }
    summary
}

pub(super) fn model_ref(model: &Value) -> Option<String> {
    let id = model.get("id").and_then(Value::as_str)?;
    if id.trim().is_empty() {
        return None;
    }
    // 进程随机盐避免从公开摘要反查型号；目录重排与并发读取不改变引用。
    let salt = MODEL_REF_SALT.get_or_init(uuid::Uuid::new_v4);
    let mut digest = Sha256::new();
    digest.update(salt.as_bytes());
    digest.update(id.as_bytes());
    Some(format!("{MODEL_REF_PREFIX}{:x}", digest.finalize()))
}

pub(super) fn is_model_ref(value: &str) -> bool {
    value.strip_prefix(MODEL_REF_PREFIX).is_some_and(|digest| {
        digest.len() == MODEL_REF_DIGEST_LENGTH
            && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn public_text<'a>(model: &'a Value, field: &str, catalog: &[Value]) -> Option<&'a str> {
    let text = model.get(field).and_then(Value::as_str)?.trim();
    if text.is_empty() || contains_internal_identity(catalog, text) || contains_amount(text) {
        return None;
    }
    let sensitive = [
        "price",
        "cost",
        "provider",
        "owned_by",
        "model id",
        "model_id",
        "model_ref",
        "cny",
        "rmb",
        "usd",
        "eur",
        "价格",
        "单价",
        "计费",
        "费用",
        "收费",
        "供应商",
        "模型id",
        "模型 id",
        "¥",
        "￥",
        "$",
        "€",
    ];
    let lower = text.to_lowercase();
    if sensitive.iter().any(|term| lower.contains(term)) {
        return None;
    }
    Some(text)
}

fn contains_amount(text: &str) -> bool {
    static AMOUNT: OnceLock<regex::Regex> = OnceLock::new();
    AMOUNT
        .get_or_init(|| {
            regex::Regex::new(r"(?i)[\d一二三四五六七八九十百千万零〇两.,]+\s*(元|美元|人民币|点|points?|dollars?|cents?|euros?)")
                .expect("valid image catalog amount pattern")
        })
        .is_match(text)
}

fn contains_internal_identity(catalog: &[Value], text: &str) -> bool {
    let normalized = normalize_identity(text);
    catalog.iter().any(|model| {
        ["id", "owned_by"]
            .iter()
            .filter_map(|key| model.get(*key).and_then(Value::as_str))
            .chain(
                model["prices"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|price| price.get("provider").and_then(Value::as_str)),
            )
            .map(normalize_identity)
            .any(|identity| !identity.is_empty() && normalized.contains(&identity))
    })
}

fn normalize_identity(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn display_name<'a>(model: &'a Value, catalog: &[Value]) -> &'a str {
    public_text(model, "display_name", catalog).unwrap_or("image processing")
}
