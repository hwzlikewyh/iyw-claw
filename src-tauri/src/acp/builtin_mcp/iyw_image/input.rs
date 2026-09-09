use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::acp::delegation::image_loader::{self, ImageLoadRequest};

use super::{invalid, IywGatewayService};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ImageSource {
    String(String),
    Object(ImageSourceObject),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ImageSourceObject {
    url: Option<String>,
    path: Option<String>,
    base64: Option<String>,
    data: Option<String>,
    mime_type: Option<String>,
    role: Option<String>,
    name: Option<String>,
}

pub(super) struct PreparedImage {
    pub(super) url: String,
    pub(super) role: String,
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) mime_type: Option<String>,
}

pub(super) async fn prepare_images(
    cwd: &Path,
    sources: &[ImageSource],
) -> Result<Vec<PreparedImage>, rmcp::ErrorData> {
    let mut prepared = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        prepared.push(prepare_image(cwd, source, index).await?);
    }
    Ok(prepared)
}

pub(super) async fn upload_images(
    service: &IywGatewayService,
    images: &mut [PreparedImage],
) -> Result<(), rmcp::ErrorData> {
    for image in images.iter_mut().filter(|image| image.url.is_empty()) {
        let bytes = image
            .bytes
            .as_ref()
            .ok_or_else(|| invalid("image data is missing"))?;
        let mime = image
            .mime_type
            .as_deref()
            .ok_or_else(|| invalid("image MIME type is missing"))?;
        image.url = service
            .upload_bytes(bytes.clone(), mime, extension_for_mime(mime)?)
            .await?;
    }
    Ok(())
}

async fn prepare_image(
    cwd: &Path,
    source: &ImageSource,
    index: usize,
) -> Result<PreparedImage, rmcp::ErrorData> {
    let (value, mut role, name, mime, encoded) = source_parts(source, index)?;
    if value.starts_with("https://") {
        return Ok(PreparedImage {
            url: credential_free_https(&value)?,
            role,
            bytes: None,
            mime_type: mime,
        });
    }
    if value.starts_with("http://") {
        return Err(invalid("image URLs must use HTTPS"));
    }
    let load_source = local_source(cwd, &value, mime.as_deref(), encoded)?;
    let loaded = image_loader::load(
        ImageLoadRequest {
            source: &load_source,
            mime_type: mime.as_deref(),
            allow_http: false,
        },
        cwd,
    )
    .await
    .map_err(|error| invalid(error.safe_message()))?;
    if role.is_empty() {
        role = name.unwrap_or_else(|| "source".to_string());
    }
    Ok(PreparedImage {
        url: String::new(),
        role,
        bytes: Some(loaded.bytes),
        mime_type: Some(loaded.mime_type.to_string()),
    })
}

fn source_parts(
    source: &ImageSource,
    index: usize,
) -> Result<(String, String, Option<String>, Option<String>, bool), rmcp::ErrorData> {
    match source {
        ImageSource::String(value) => Ok((
            value.trim().to_string(),
            "source".to_string(),
            Some(format!("input-{}.png", index + 1)),
            None,
            value.trim().starts_with("data:"),
        )),
        ImageSource::Object(value) => source_object_parts(value),
    }
}

fn source_object_parts(
    source: &ImageSourceObject,
) -> Result<(String, String, Option<String>, Option<String>, bool), rmcp::ErrorData> {
    let values = [&source.url, &source.path, &source.base64, &source.data];
    if values.iter().filter(|item| item.is_some()).count() != 1 {
        return Err(invalid(
            "each image object must provide exactly one of url, path, base64, or data",
        ));
    }
    let value = source
        .url
        .as_ref()
        .or(source.path.as_ref())
        .or(source.base64.as_ref())
        .or(source.data.as_ref())
        .expect("validated image source");
    Ok((
        value.trim().to_string(),
        source.role.clone().unwrap_or_else(|| "source".to_string()),
        source.name.clone(),
        source.mime_type.clone(),
        source.base64.is_some() || source.data.is_some(),
    ))
}

fn local_source(
    cwd: &Path,
    value: &str,
    mime: Option<&str>,
    encoded: bool,
) -> Result<String, rmcp::ErrorData> {
    if value.starts_with("data:") {
        return Ok(value.to_string());
    }
    if encoded {
        let mime = mime.ok_or_else(|| invalid("raw base64 requires mimeType"))?;
        return Ok(format!("data:{mime};base64,{value}"));
    }
    Ok(workspace_path(cwd, value)?.to_string_lossy().into_owned())
}

fn credential_free_https(value: &str) -> Result<String, rmcp::ErrorData> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid("image URL is invalid"))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err(invalid("image URL must be credential-free HTTPS"));
    }
    Ok(value.to_string())
}

fn workspace_path(cwd: &Path, value: &str) -> Result<PathBuf, rmcp::ErrorData> {
    let candidate = if Path::new(value).is_absolute() {
        PathBuf::from(value)
    } else {
        cwd.join(value)
    };
    let root =
        std::fs::canonicalize(cwd).map_err(|_| invalid("working directory is unavailable"))?;
    let path =
        std::fs::canonicalize(candidate).map_err(|_| invalid("image file is unavailable"))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(invalid("image path must stay inside the current workspace"));
    }
    Ok(path)
}

fn extension_for_mime(mime: &str) -> Result<&'static str, rmcp::ErrorData> {
    match mime {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        "image/gif" => Ok("gif"),
        "image/bmp" => Ok("bmp"),
        "image/avif" => Ok("avif"),
        "image/svg+xml" => Ok("svg"),
        _ => Err(invalid("unsupported image MIME type")),
    }
}
