use std::sync::Arc;

use axum::{
    body::Body,
    extract::{multipart::Field, DefaultBodyLimit, FromRequest, Multipart, Request},
    http::{HeaderMap, HeaderValue},
};
use serde_json::{json, Map, Value};
use tokio::io::AsyncWriteExt;

use super::{
    assets::AssetCache, error::GatewayError, limits, media, spool::Spool,
    spool_budget::SpoolBudget, Runtime,
};

const MAX_FIELDS: usize = 256;

struct Image {
    name: String,
    mime: String,
    body: Spool,
}

pub(super) fn is_multipart(path: &str, headers: &HeaderMap) -> bool {
    path.ends_with("/images/edits")
        && headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("multipart/form-data"))
            })
}

pub(super) async fn prepare(
    runtime: &Arc<Runtime>,
    body: Spool,
    (headers, token): (&mut HeaderMap, &str),
) -> Result<Spool, GatewayError> {
    if headers
        .get("content-encoding")
        .is_some_and(|value| value != "identity")
    {
        return Err(GatewayError::input(
            "Compressed multipart image edits are unsupported",
        ));
    }
    let _permit = runtime
        .images
        .acquire()
        .await
        .map_err(|_| GatewayError::unavailable("Image queue closed"))?;
    let mut request = Request::new(Body::new(body.into_body()?));
    *request.headers_mut() = headers.clone();
    DefaultBodyLimit::max(limits::MAX_BODY_BYTES as usize).apply(&mut request);
    let multipart = Multipart::from_request(request, &())
        .await
        .map_err(|_| GatewayError::input("Invalid image edit multipart boundary"))?;
    let (payload, images) = fields(multipart).await?;
    let body = upload_images(runtime, (payload, images), token).await?;
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    Ok(body)
}

async fn upload_images(
    runtime: &Runtime,
    (mut payload, images): (Map<String, Value>, Vec<Image>),
    token: &str,
) -> Result<Spool, GatewayError> {
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| GatewayError::input("Image edit model is required"))?
        .to_owned();
    if payload.get("stream") == Some(&Value::Bool(true)) {
        return Err(GatewayError::input("Streaming image edits are unsupported"));
    }
    let mut sources = Vec::new();
    for image in images {
        let name = image.name.clone();
        let url = upload(runtime, image, (token, &model)).await?;
        let source = json!({"image_url": url});
        if name == "mask" {
            payload.insert(name, source);
        } else {
            sources.push(source);
        }
    }
    let image_count = sources.len();
    payload.insert("images".into(), Value::Array(sources));
    let body = serde_json::to_vec(&payload)
        .map_err(|_| GatewayError::unavailable("Image edit JSON encoding failed"))?;
    tracing::info!(target: "model_gateway", image_count, has_mask=payload.contains_key("mask"),
        final_bytes=body.len(), "multipart image edit prepared as JSON image URLs");
    Spool::memory(body.into())
}

async fn fields(
    mut multipart: Multipart,
) -> Result<(Map<String, Value>, Vec<Image>), GatewayError> {
    let mut payload = Map::new();
    let mut images: Vec<Image> = Vec::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| GatewayError::input("Invalid image edit multipart body"))?
    {
        if payload.len() + images.len() >= MAX_FIELDS {
            return Err(GatewayError::input("Too many image edit fields"));
        }
        let name = field
            .name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| GatewayError::input("Image edit field name is required"))?
            .to_owned();
        if matches!(name.as_str(), "image" | "image[]" | "mask") {
            add_image(&mut images, &mut field, name).await?;
        } else {
            if field.file_name().is_some()
                || matches!(name.as_str(), "images" | "mask")
                || payload.contains_key(&name)
            {
                return Err(GatewayError::input(
                    "Unsupported or duplicate image edit field",
                ));
            }
            let value = text_field(&mut field, &name).await?;
            payload.insert(name, value);
        }
    }
    if !images.iter().any(|image| image.name != "mask") {
        return Err(GatewayError::input("Image edit requires a source image"));
    }
    Ok((payload, images))
}

async fn add_image(
    images: &mut Vec<Image>,
    field: &mut Field<'_>,
    name: String,
) -> Result<(), GatewayError> {
    if images.len() >= limits::MAX_IMAGES
        || name == "mask" && images.iter().any(|v| v.name == "mask")
    {
        return Err(GatewayError::input(
            "Too many image edit images or duplicate mask",
        ));
    }
    let mime = field
        .content_type()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    images.push(Image {
        name,
        mime,
        body: receive_image(field).await?,
    });
    Ok(())
}

async fn text_field(field: &mut Field<'_>, name: &str) -> Result<Value, GatewayError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|_| GatewayError::input("Image edit field could not be read"))?
    {
        if bytes.len() + chunk.len() > limits::METADATA_BYTES {
            return Err(GatewayError::input("Image edit text field is too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| GatewayError::input("Image edit fields must be UTF-8"))?;
    match name {
        "n" | "output_compression" | "partial_images" => text
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| GatewayError::input("Image edit numeric field is invalid")),
        "stream" => text
            .parse::<bool>()
            .map(Value::from)
            .map_err(|_| GatewayError::input("Image edit stream field is invalid")),
        _ => Ok(Value::String(text)),
    }
}

async fn receive_image(field: &mut Field<'_>) -> Result<Spool, GatewayError> {
    let mut file = tokio::fs::File::from_std(tempfile::tempfile()?);
    let budget = Arc::new(SpoolBudget::default());
    let mut size = 0;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|_| GatewayError::input("Image edit file could not be read"))?
    {
        size += chunk.len() as u64;
        if size > limits::MAX_IMAGE_BYTES {
            return Err(GatewayError::input("Image exceeds 32 MiB"));
        }
        budget.reserve(chunk.len() as u64)?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(Spool::File(file.into_std().await, size, budget))
}

async fn upload(
    runtime: &Runtime,
    image: Image,
    (token, model): (&str, &str),
) -> Result<String, GatewayError> {
    let prepared = super::work::blocking(runtime, move || {
        let prepared = media::prepare_file(&image.body, &image.mime)?;
        if image.name == "mask" && prepared.mime != "image/png" {
            return Err(GatewayError::input("Image edit masks must be PNG"));
        }
        Ok(prepared)
    })
    .await?;
    let key = AssetCache::key(token, &prepared.encoded_digest);
    let _same_asset = runtime.asset_locks[usize::from(key[0]) % runtime.asset_locks.len()]
        .lock()
        .await;
    Ok(runtime.upload((token, model), prepared).await?.image_url)
}
