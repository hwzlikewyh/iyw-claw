use reqwest::Url;
use serde_json::Value;

pub(super) fn extract_url(value: &Value) -> Result<Url, rmcp::ErrorData> {
    let raw = value
        .as_str()
        .or_else(|| value.get("value").and_then(Value::as_str))
        .or_else(|| value.get("url").and_then(Value::as_str))
        .ok_or_else(|| rmcp::ErrorData::invalid_params("upload URL is missing", None))?;
    let url = Url::parse(raw)
        .map_err(|_| rmcp::ErrorData::invalid_params("upload URL is invalid", None))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err(rmcp::ErrorData::invalid_params(
            "upload URL must be credential-free HTTPS",
            None,
        ));
    }
    Ok(url)
}

pub(super) fn public_url(url: &Url) -> String {
    let mut clean = url.clone();
    clean.set_query(None);
    clean.set_fragment(None);
    clean.to_string()
}
