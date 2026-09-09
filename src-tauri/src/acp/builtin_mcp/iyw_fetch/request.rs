use std::collections::BTreeMap;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Method, Request, RequestBuilder, Url};
use rmcp::ErrorData;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value};

use super::{http, invalid};

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const RESERVED_HEADERS: [&str; 12] = [
    "token",
    "authorization",
    "cookie",
    "cookie2",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "trailer",
    "te",
    "upgrade",
    "expect",
];

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BodyType {
    #[default]
    Json,
    Form,
    Text,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchRequest {
    url: String,
    description: Option<String>,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    query: Map<String, Value>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body_type: BodyType,
    #[serde(default, deserialize_with = "present_body")]
    body: Option<Value>,
}

fn default_method() -> String {
    "POST".to_string()
}

fn present_body<'de, D: Deserializer<'de>>(value: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(value).map(Some)
}

pub(super) fn prepare(client: &Client, arguments: Value) -> Result<Request, ErrorData> {
    let params: FetchRequest = serde_json::from_value(arguments)
        .map_err(|_| invalid("Invalid request fields; follow the fetch_iyw_url input schema"))?;
    super::super::iyw_progress::validate(params.description.as_deref())?;
    let url = Url::parse(&params.url).map_err(|_| invalid("url must be an absolute HTTPS URL"))?;
    if !http::allowed_url(&url) {
        return Err(invalid(
            "url must use HTTPS on iyw.cn or its subdomains, without credentials or fragments",
        ));
    }
    let method = parse_method(&params.method)?;
    if (method == Method::GET || method == Method::HEAD) && params.body.is_some() {
        return Err(invalid(
            "GET and HEAD cannot include body; use query parameters",
        ));
    }
    let request = client
        .request(method, url)
        .query(&field_pairs(&params.query)?);
    let request = with_body(request, &params)?
        .headers(request_headers(&params.headers)?)
        .build()
        .map_err(|_| invalid("Failed to encode HTTP request"))?;
    if request
        .body()
        .and_then(|body| body.as_bytes())
        .is_some_and(|bytes| bytes.len() > MAX_REQUEST_BYTES)
    {
        return Err(invalid("request body exceeds the 2 MiB limit"));
    }
    Ok(request)
}

fn parse_method(value: &str) -> Result<Method, ErrorData> {
    let value = value.trim().to_ascii_uppercase();
    if !METHODS.contains(&value.as_str()) {
        return Err(invalid(
            "method must be GET, POST, PUT, PATCH, DELETE, HEAD, or OPTIONS",
        ));
    }
    Method::from_bytes(value.as_bytes()).map_err(|_| invalid("Invalid HTTP method"))
}

fn with_body(request: RequestBuilder, params: &FetchRequest) -> Result<RequestBuilder, ErrorData> {
    let Some(body) = &params.body else {
        return Ok(request);
    };
    match params.body_type {
        BodyType::Json => Ok(request.json(body)),
        BodyType::Form => {
            let fields = body
                .as_object()
                .ok_or_else(|| invalid("form body must be an object"))?;
            Ok(request.form(&field_pairs(fields)?))
        }
        BodyType::Text => {
            let text = body
                .as_str()
                .ok_or_else(|| invalid("text body must be a string"))?;
            Ok(request
                .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(text.to_string()))
        }
    }
}

fn field_pairs(fields: &Map<String, Value>) -> Result<Vec<(String, String)>, ErrorData> {
    let mut pairs = Vec::new();
    for (key, field) in fields {
        let values = field
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(std::slice::from_ref(field));
        for value in values {
            if let Some(value) = scalar(value)? {
                pairs.push((key.clone(), value));
            }
        }
    }
    Ok(pairs)
}

fn scalar(value: &Value) -> Result<Option<String>, ErrorData> {
    match value {
        Value::String(value) => Ok(Some(value.clone())),
        Value::Number(_) | Value::Bool(_) => Ok(Some(value.to_string())),
        Value::Null => Ok(None),
        _ => Err(invalid(
            "query/form fields must be scalars or arrays of scalars",
        )),
    }
}

fn request_headers(values: &BTreeMap<String, String>) -> Result<HeaderMap, ErrorData> {
    let mut headers = HeaderMap::new();
    for (name, value) in values {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid("Invalid header name"))?;
        if RESERVED_HEADERS.contains(&name.as_str()) || name.as_str().starts_with("proxy-") {
            return Err(invalid(
                "Credential, Host, framing, connection, and proxy headers cannot be overridden",
            ));
        }
        let value = HeaderValue::from_str(value).map_err(|_| invalid("Invalid header value"))?;
        if headers.insert(name, value).is_some() {
            return Err(invalid(
                "Duplicate header names are not allowed, including case variants",
            ));
        }
    }
    Ok(headers)
}

pub(super) fn fields_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": {
            "anyOf": [
                {"type": ["string", "number", "boolean", "null"]},
                {"type": "array", "items": {"type": ["string", "number", "boolean", "null"]}}
            ]
        },
        "description": "Scalar values or scalar arrays. Arrays produce repeated keys; null values are omitted."
    })
}
