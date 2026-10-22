use serde::Deserialize;
use serde_json::{json, Value};

use super::authority::SessionContext;
use super::iyw_service::IywGatewayService;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KnowledgeRequest {
    query: String,
    #[serde(default)]
    category: i64,
    folder_id: Option<Value>,
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
    authority: &SessionContext,
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
    let lookup = &service.listener().parent_lookup;
    let conversation_id = lookup
        .current_conversation_id(authority.connection_id())
        .await
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(
                "Conversation unavailable; knowledge scope cannot be resolved",
                None,
            )
        })?;
    let result = crate::commands::knowledge::search::search(&service.conn, conversation_id, body)
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
    if lookup
        .current_conversation_id(authority.connection_id())
        .await
        != Some(conversation_id)
    {
        return Err(rmcp::ErrorData::invalid_request(
            "Conversation changed; retry knowledge search",
            None,
        ));
    }
    Ok(rmcp::model::CallToolResult::structured(result))
}

fn validate(request: &KnowledgeRequest) -> Result<(), rmcp::ErrorData> {
    let valid_id = crate::commands::knowledge::models::valid_id;
    let folder_valid = request.folder_id.as_ref().is_none_or(|value| match value {
        Value::String(id) => valid_id(id),
        Value::Number(id) => id.as_u64().is_some(),
        _ => false,
    });
    if !(0..=1).contains(&request.category)
        || !folder_valid
        || request.file_id.as_deref().is_some_and(|id| !valid_id(id))
    {
        return Err(rmcp::ErrorData::invalid_params(
            "Invalid knowledge category or ID",
            None,
        ));
    }
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
