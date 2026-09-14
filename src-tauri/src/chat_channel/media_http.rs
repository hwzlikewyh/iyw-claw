use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::Value;

use super::attachments::{ChannelAttachment, IncomingAttachment, MAX_INBOUND_BYTES};
use super::error::ChatChannelError;

pub fn failure(message: impl Into<String>) -> ChatChannelError {
    ChatChannelError::SendFailed(message.into())
}

pub fn transport(error: reqwest::Error) -> ChatChannelError {
    failure(error.without_url().to_string())
}

pub async fn json(response: reqwest::Response) -> Result<Value, ChatChannelError> {
    let bytes = read_limited(response, 1024 * 1024).await?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| failure("Provider returned invalid JSON"))?;
    for field in ["errcode", "ret"] {
        if let Some(code) = value.get(field).and_then(Value::as_i64) {
            if code != 0 {
                return Err(failure(format!(
                    "Provider rejected media request: {field}={code}"
                )));
            }
        }
    }
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(failure("Provider rejected media request"));
    }
    if let Some(code) = value.get("code") {
        if code != 0 && code != "0" && code != "ok" && code != "OK" {
            return Err(failure(format!(
                "Provider rejected media request: code={code}"
            )));
        }
    }
    Ok(value)
}

pub fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, ChatChannelError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| failure(format!("Provider response omitted {key}")))
}

pub async fn read_limited(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, ChatChannelError> {
    if !response.status().is_success() {
        return Err(failure(format!("Media HTTP {}", response.status())));
    }
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return Err(failure("Media exceeds the size limit"));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(transport)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(failure("Media exceeds the size limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(failure("Media is empty"));
    }
    Ok(bytes)
}

pub async fn download(url: &str) -> Result<Vec<u8>, ChatChannelError> {
    download_named(url).await.map(|(bytes, _)| bytes)
}

pub async fn download_named(url: &str) -> Result<(Vec<u8>, Option<String>), ChatChannelError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| failure("Invalid media download URL"))?;
    if parsed.scheme() != "https" {
        return Err(failure("Media download requires HTTPS"));
    }
    let file =
        crate::remote_image::network::download_resource(url, MAX_INBOUND_BYTES.saturating_add(32))
            .await
            .map_err(|error| failure(format!("Media download failed: {error}")))?;
    let name = file
        .content_disposition
        .as_deref()
        .and_then(super::media_filename::from_disposition);
    Ok((file.bytes, name))
}

pub fn attachment(source: &IncomingAttachment, bytes: Vec<u8>) -> ChannelAttachment {
    let mime_type = image::guess_format(&bytes)
        .ok()
        .map(|format| format.to_mime_type().to_string())
        .unwrap_or_else(|| source.mime_type.clone());
    ChannelAttachment {
        name: source.name.clone(),
        mime_type,
        bytes: Arc::from(bytes),
    }
}

pub fn response_filename(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(super::media_filename::from_disposition)
}

pub async fn binary(response: reqwest::Response) -> Result<Vec<u8>, ChatChannelError> {
    let json_error = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .is_none()
        && response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
    let bytes = read_limited(response, MAX_INBOUND_BYTES).await?;
    if json_error {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            for key in ["errcode", "code"] {
                if value[key].as_i64().is_some_and(|code| code != 0) {
                    return Err(failure(format!(
                        "Provider resource download rejected: {key}={}",
                        value[key]
                    )));
                }
            }
        }
    }
    Ok(bytes)
}
