use std::path::{Path, PathBuf};

use futures_util::{stream, StreamExt};
use serde::Deserialize;

use super::{invalid, IywGatewayService};

mod asset;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ImageValidation {
    Platform,
    Edit,
    MaskedEdit,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ImageSource {
    String(String),
    Object(ImageSourceObject),
}

impl ImageSource {
    pub(super) fn role(&self) -> &str {
        match self {
            Self::String(_) => "source",
            Self::Object(source) => source.role.as_deref().unwrap_or("source"),
        }
    }
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

#[derive(Clone)]
pub(super) struct PreparedImage {
    pub(super) url: String,
    pub(super) role: String,
    pub(super) mime_type: Option<String>,
    pub(super) dimensions: Option<(u32, u32)>,
    pub(super) byte_size: Option<usize>,
}

pub(super) async fn prepare_images(
    context: (&IywGatewayService, &Path),
    sources: &[ImageSource],
    validation: ImageValidation,
) -> Result<Vec<PreparedImage>, rmcp::ErrorData> {
    let first_source = sources.iter().position(|image| image.role() != "mask");
    let pending = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let inspect = validation == ImageValidation::MaskedEdit
                && (source.role() == "mask" || Some(index) == first_source);
            prepare_image(
                context,
                (index, source),
                (validation != ImageValidation::Platform, inspect),
            )
        })
        .collect::<Vec<_>>();
    let results = stream::iter(pending)
        .buffered(super::limits::MAX_CONCURRENT_EXECUTIONS)
        .collect::<Vec<_>>()
        .await;
    results.into_iter().collect()
}

async fn prepare_image(
    (service, cwd): (&IywGatewayService, &Path),
    (index, source): (usize, &ImageSource),
    (editing, inspect): (bool, bool),
) -> Result<PreparedImage, rmcp::ErrorData> {
    let (value, mut role, name, mime, encoded) = source_parts(source, index)?;
    if value.starts_with("http://") {
        return Err(invalid("image URLs must use HTTPS"));
    }
    let load_source = if value.starts_with("https://") {
        credential_free_https(&value)?
    } else {
        local_source(cwd, &value, mime.as_deref(), encoded)?
    };
    let mut prepared = asset::prepare(
        (service, cwd),
        (&load_source, mime.as_deref()),
        (editing, inspect),
    )
    .await?;
    if role.is_empty() {
        role = name.unwrap_or_else(|| "source".to_string());
    }
    if role == "mask" && editing && prepared.mime_type.as_deref() != Some("image/png") {
        return Err(invalid("Fusion edit masks must be PNG"));
    }
    prepared.role = role;
    Ok(prepared)
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
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
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
