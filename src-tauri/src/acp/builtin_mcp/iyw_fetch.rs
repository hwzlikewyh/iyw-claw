use reqwest::header::HeaderValue;
use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};

use super::iyw_service::IywGatewayService;
use super::tool_identity::FETCH_URL_TOOL;

mod http;
mod request;
mod response;

pub(super) fn tool() -> Value {
    json!({
        "name": FETCH_URL_TOOL,
        "description": "Call a known IYW website API using the current iyw-claw login. Supports HTTPS iyw.cn and all subdomain levels, with any API path. Use endpoints and parameters from the user, official documentation, or observed requests. Supports GET, POST (default), PUT, PATCH, DELETE, HEAD, and OPTIONS; method names are case-insensitive. GET/HEAD cannot carry bodies. body_type selects json (default), form (URL-encoded fields), or text (raw string, including XML); override content-type in headers when needed. query and form fields accept scalars or scalar arrays (repeated keys), with null omitted. The host supplies the current token and browser defaults (Origin https://tu.iyw.cn, Referer https://tu.iyw.cn/trendPop?from=portal). Additional headers may override defaults but cannot replace credentials, Host, or transport headers. Never supply tokens or cookies. Every tool execution returns the outputSchema envelope: ok, status, content_type, body_type, body, error. ok means HTTP 2xx and a complete response, not business success; inspect body business codes/messages. body_type is json, text, base64, or empty. Failures preserve HTTP status/body when available; status is null before any response. HTTP non-2xx and tool failures set the MCP error flag. The HTTP timeout is 60 seconds; encoded request and raw response bodies are limited to 2 MiB. Follow at most five redirects within the allowed HTTPS domains; rejected redirects return 3xx. No automatic retries: a timeout/cancellation or incomplete response may follow an executed operation. Only perform user-authorized operations and never blindly resubmit writes. Prefer dedicated tools when available, such as generate_iyw_image for image production. No capability search/read is required.",
        "inputSchema": {
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {"type": "string", "minLength": 1, "description": "Absolute HTTPS URL on iyw.cn or any subdomain. No URL credentials or fragments."},
                "method": {"type": "string", "default": "POST", "description": "GET, POST, PUT, PATCH, DELETE, HEAD, or OPTIONS (case-insensitive)."},
                "query": request::fields_schema(),
                "headers": {"type": "object", "additionalProperties": {"type": "string"}, "description": "Optional HTTP headers. Authentication, cookies, Host, framing, connection, and proxy headers are reserved for the host."},
                "body_type": {"type": "string", "enum": ["json", "form", "text"], "default": "json"},
                "body": {"description": "Body encoded according to body_type. json accepts any JSON value (including null); form requires an object with scalar/array fields; text requires a string. Omit for GET/HEAD. Encoded limit: 2 MiB."}
            },
            "additionalProperties": false
        },
        "outputSchema": response::schema(),
        "annotations": {
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": false,
            "openWorldHint": true
        }
    })
}

pub(super) async fn fetch(
    service: &IywGatewayService,
    arguments: Value,
) -> Result<CallToolResult, ErrorData> {
    let client = http::client()?;
    let mut request = request::prepare(&client, arguments)?;
    let token = service.token().await.map_err(|error| {
        ErrorData::invalid_request(
            error.message,
            Some(json!({
                "code": "authentication_error", "execution_status": "not_started"
            })),
        )
    })?;
    let mut token_header = HeaderValue::from_str(&token)
        .map_err(|_| invalid("IYW login token is not a valid HTTP header"))?;
    token_header.set_sensitive(true);
    request.headers_mut().insert("token", token_header);
    tracing::info!(target: "builtin_mcp", host = request.url().host_str(),
        method = request.method().as_str(), "[iyw-fetch] request started");
    response::execute(&client, request, &token).await
}

fn invalid(message: &'static str) -> ErrorData {
    tracing::warn!(target: "builtin_mcp", reason = message,
        execution_status = "not_started", "[iyw-fetch] request rejected");
    ErrorData::invalid_params(
        message,
        Some(json!({
            "code": "invalid_request", "execution_status": "not_started"
        })),
    )
}

pub(super) fn failure(error: ErrorData) -> CallToolResult {
    response::failure(error)
}

pub(super) fn cancelled() -> ErrorData {
    ErrorData::invalid_request(
        "IYW request cancelled or authority revoked; execution may have occurred. Do not retry writes blindly.",
        Some(json!({"code": "cancelled", "execution_status": "unknown"})),
    )
}
