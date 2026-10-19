use axum::http::HeaderMap;

use super::error::GatewayError;

const MAX_AUTH_TOKEN_BYTES: usize = 8192;

const HOP_HEADERS: &[&str] = &[
    "host",
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
];

pub(super) fn filtered(headers: &HeaderMap) -> HeaderMap {
    let mut result = headers.clone();
    for connection in headers
        .get_all("connection")
        .iter()
        .filter_map(|value| value.to_str().ok())
    {
        for name in connection.split(',').map(str::trim) {
            result.remove(name);
        }
    }
    for name in HOP_HEADERS {
        result.remove(*name);
    }
    result.remove("origin");
    result.remove("referer");
    result
}

pub(super) fn token(headers: &HeaderMap) -> Result<String, GatewayError> {
    let value = headers
        .get("token")
        .or_else(|| headers.get("authorization"))
        .or_else(|| headers.get("x-api-key"))
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let value = value.trim();
    let value = match value.split_once(' ') {
        Some((scheme, token)) if scheme.eq_ignore_ascii_case("Bearer") => token.trim(),
        _ => value,
    };
    if value.is_empty() || value.len() > MAX_AUTH_TOKEN_BYTES {
        return Err(GatewayError::unauthorized());
    }
    Ok(value.to_owned())
}
