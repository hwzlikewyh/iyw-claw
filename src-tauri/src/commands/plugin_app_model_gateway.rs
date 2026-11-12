use serde_json::{Map, Value};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = crate::plugin_runtime::app_host::MAX_MESSAGE_BYTES;
const MODEL_PATHS: &[&str] = &[
    "models",
    "chat/completions",
    "responses",
    "responses/compact",
    "images/generations",
    "images/edits",
];

pub(super) async fn request(
    db: &crate::db::AppDatabase,
    arguments: &Map<String, Value>,
) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let path = string(arguments, "path")?;
    let method = string(arguments, "method")?;
    let url = gateway_url(&path, &method)?;
    let token = crate::commands::iyw_account::iyw_account_access_token_core(&db.conn)
        .await
        .map_err(|_| rejected("sign in to iyw-claw to use the local model gateway"))?
        .ok_or_else(|| rejected("sign in to iyw-claw to use the local model gateway"))?;
    forward(model_client()?, arguments, &url, &method, token.expose()).await
}

fn gateway_url(
    path: &str,
    method: &str,
) -> Result<String, crate::plugin_runtime::types::PluginInvokeError> {
    if !MODEL_PATHS.contains(&path.as_str()) || !matches!(method.as_str(), "GET" | "POST") {
        return Err(rejected(
            "model endpoint is not available through the plugin gateway",
        ));
    }
    if (path == "models") != (method == "GET") {
        return Err(rejected("model endpoint method is invalid"));
    }
    crate::acp::model_gateway::endpoint(path)
        .ok_or_else(|| rejected("the local model gateway is unavailable"))
}

fn model_client() -> Result<reqwest::Client, crate::plugin_runtime::types::PluginInvokeError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|_| rejected("the local model gateway client is unavailable"))
}

async fn forward(
    client: reqwest::Client,
    arguments: &Map<String, Value>,
    url: &str,
    method: &str,
    token: &str,
) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let mut request = client.request(
        if method == "GET" {
            reqwest::Method::GET
        } else {
            reqwest::Method::POST
        },
        url,
    );
    if let Some(query) = arguments.get("query").and_then(Value::as_object) {
        let pairs = query
            .iter()
            .filter_map(|(key, value)| value.as_str().map(|value| (key, value)))
            .collect::<Vec<_>>();
        request = request.query(&pairs);
    }
    if method == "POST" {
        let body = arguments
            .get("body")
            .filter(|value| value.is_object())
            .ok_or_else(|| rejected("a JSON request body is required"))?;
        request = request.json(body);
    }
    let mut response = request
        .header("token", token)
        .header("accept-encoding", "identity")
        .send()
        .await
        .map_err(|_| rejected("the local model gateway request failed"))?;
    read_response(&mut response).await
}

async fn read_response(
    response: &mut reqwest::Response,
) -> Result<Value, crate::plugin_runtime::types::PluginInvokeError> {
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| rejected("the model response was interrupted"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(rejected(
                "the model response exceeds the plugin message limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = if content_type.starts_with("application/json") || content_type.starts_with("text/")
    {
        serde_json::json!({ "text": String::from_utf8_lossy(&bytes) })
    } else {
        use base64::Engine;
        serde_json::json!({ "base64": base64::engine::general_purpose::STANDARD.encode(bytes) })
    };
    Ok(serde_json::json!({ "status": status, "contentType": content_type, "body": body }))
}

fn string(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<String, crate::plugin_runtime::types::PluginInvokeError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| rejected("model request fields are invalid"))
}

fn rejected(message: &str) -> crate::plugin_runtime::types::PluginInvokeError {
    crate::plugin_runtime::types::PluginInvokeError::before_effect(
        "plugin_model_gateway_unavailable",
        message,
    )
}
