use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::iyw_service::IywGatewayService;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KnowledgeRequest {
    query: String,
    #[serde(default)]
    category: i64,
    folder_id: Option<i64>,
    file_id: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default = "default_dense_weight")]
    dense_weight: f64,
}

const fn default_limit() -> i64 {
    10
}

const fn default_dense_weight() -> f64 {
    0.5
}

pub(super) async fn search(
    service: &IywGatewayService,
    arguments: Value,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let request: KnowledgeRequest = serde_json::from_value(arguments)
        .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
    validate(&request)?;
    let body = json!({
        "category": request.category,
        "query": request.query.trim(),
        "folderId": request.folder_id,
        "fileId": request.file_id,
        "limit": request.limit,
        "denseWeight": request.dense_weight,
    });
    let value = service
        .post_gateway("/ai-agent-new/api/knowledge", "search", body)
        .await?;
    let result = normalize(value)?;
    Ok(rmcp::model::CallToolResult::structured(result))
}

fn validate(request: &KnowledgeRequest) -> Result<(), rmcp::ErrorData> {
    if request.query.trim().is_empty() || request.query.chars().count() > 4096 {
        return Err(rmcp::ErrorData::invalid_params(
            "query must contain between 1 and 4096 characters",
            None,
        ));
    }
    if !(1..=100).contains(&request.limit) {
        return Err(rmcp::ErrorData::invalid_params(
            "limit must be between 1 and 100",
            None,
        ));
    }
    if !(0.0..=1.0).contains(&request.dense_weight) {
        return Err(rmcp::ErrorData::invalid_params(
            "denseWeight must be between 0 and 1",
            None,
        ));
    }
    Ok(())
}

fn normalize(value: Value) -> Result<Value, rmcp::ErrorData> {
    let result = value
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_response("knowledge search omitted result"))?;
    if result.get("code").and_then(Value::as_i64) != Some(0) {
        return Err(invalid_response("knowledge search failed"));
    }
    let data = result
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_response("knowledge search omitted result data"))?;
    let count = data
        .get("count")
        .and_then(Value::as_i64)
        .filter(|count| *count >= 0)
        .ok_or_else(|| invalid_response("knowledge search returned invalid count"))?;
    let items = data
        .get("result_list")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("knowledge search returned invalid results"))?;
    let results = items
        .iter()
        .map(normalize_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"count": count, "results": results}))
}

fn normalize_item(value: &Value) -> Result<Value, rmcp::ErrorData> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_response("knowledge result item is invalid"))?;
    let document = object
        .get("doc_info")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    Ok(json!({
        "id": object.get("id"),
        "score": object.get("score"),
        "content": object.get("content"),
        "md_content": object.get("md_content"),
        "chunk_type": object.get("chunk_type"),
        "doc_id": document.get("doc_id"),
        "doc_name": document.get("doc_name"),
        "doc_type": document.get("doc_type"),
    }))
}

fn invalid_response(message: &'static str) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(message, None)
}
