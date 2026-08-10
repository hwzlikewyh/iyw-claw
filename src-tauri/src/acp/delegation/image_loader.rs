use std::fmt;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};

pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_BASE64_LEN: usize = MAX_IMAGE_BYTES.div_ceil(3) * 4;

pub struct ImageLoadRequest<'a> {
    pub source: &'a str,
    pub mime_type: Option<&'a str>,
    pub allow_http: bool,
}

pub struct LoadedImage {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub source_kind: Option<&'static str>,
    pub source: Option<String>,
    pub name: Option<String>,
}

pub struct ImageLoadError {
    pub code: &'static str,
    message: String,
}

impl ImageLoadError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn safe_message(&self) -> &'static str {
        match self.code {
            "image_too_large" => "The image exceeds the 10 MiB limit.",
            "image_download_failed" => "The image could not be downloaded.",
            "image_unavailable" => "The image file could not be read.",
            "image_invalid_base64" => "The image data is not valid Base64.",
            "image_url_not_https" => "Image analysis only accepts HTTPS URLs.",
            "image_mime_mismatch" => "The declared image type does not match its content.",
            "image_unsupported_format" => "The image format is not supported.",
            _ => "The image source is invalid.",
        }
    }
}

impl fmt::Display for ImageLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

struct LoadedSource {
    bytes: Vec<u8>,
    declared_mime: Option<String>,
    source_kind: Option<&'static str>,
    source: Option<String>,
    name: Option<String>,
}

pub async fn load(
    request: ImageLoadRequest<'_>,
    working_dir: &Path,
) -> Result<LoadedImage, ImageLoadError> {
    let source = request.source.trim();
    if source.is_empty() {
        return Err(ImageLoadError::new(
            "image_invalid_source",
            "source must not be empty",
        ));
    }
    let loaded = load_source(&request, source, working_dir).await?;
    ensure_size(loaded.bytes.len())?;
    let detected = super::image_format::detect_mime(&loaded.bytes)
        .ok_or_else(|| ImageLoadError::new("image_unsupported_format", "unsupported image data"))?;
    let declared = request.mime_type.or(loaded.declared_mime.as_deref());
    if let Some(mime) = declared {
        let normalized = super::image_format::normalize_mime(mime).ok_or_else(|| {
            ImageLoadError::new(
                "image_unsupported_format",
                format!("unsupported MIME type: {mime}"),
            )
        })?;
        if normalized != detected {
            return Err(ImageLoadError::new(
                "image_mime_mismatch",
                format!("declared MIME type {mime} does not match {detected} image data"),
            ));
        }
    }
    Ok(LoadedImage {
        bytes: loaded.bytes,
        mime_type: detected,
        source_kind: loaded.source_kind,
        source: loaded.source,
        name: loaded.name,
    })
}

async fn load_source(
    request: &ImageLoadRequest<'_>,
    source: &str,
    working_dir: &Path,
) -> Result<LoadedSource, ImageLoadError> {
    if source.starts_with("data:") {
        return load_data_uri(source);
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        if !request.allow_http && !source.starts_with("https://") {
            return Err(ImageLoadError::new(
                "image_url_not_https",
                "image URL must use HTTPS",
            ));
        }
        return load_url(source).await;
    }
    let path = resolve_path(source, working_dir)?;
    if path.exists() || request.mime_type.is_none() {
        return load_file(path).await;
    }
    let mime = request.mime_type.ok_or_else(|| {
        ImageLoadError::new("image_invalid_source", "raw Base64 requires mime_type")
    })?;
    Ok(LoadedSource {
        bytes: decode_base64(source)?,
        declared_mime: Some(mime.to_string()),
        source_kind: None,
        source: None,
        name: None,
    })
}

fn resolve_path(source: &str, working_dir: &Path) -> Result<PathBuf, ImageLoadError> {
    if source.starts_with("file:") {
        return reqwest::Url::parse(source)
            .map_err(|error| {
                ImageLoadError::new("image_invalid_source", format!("invalid file URI: {error}"))
            })?
            .to_file_path()
            .map_err(|_| ImageLoadError::new("image_invalid_source", "invalid file URI path"));
    }
    let path = PathBuf::from(source);
    Ok(if path.is_absolute() {
        path
    } else {
        working_dir.join(path)
    })
}

async fn load_file(path: PathBuf) -> Result<LoadedSource, ImageLoadError> {
    let path = std::fs::canonicalize(&path).map_err(|error| {
        ImageLoadError::new(
            "image_unavailable",
            format!("cannot open image {}: {error}", path.display()),
        )
    })?;
    let size = std::fs::metadata(&path)
        .map_err(|error| ImageLoadError::new("image_unavailable", error.to_string()))?
        .len() as usize;
    ensure_size(size)?;
    let read_path = path.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(read_path))
        .await
        .map_err(|error| {
            ImageLoadError::new(
                "image_unavailable",
                format!("image read task failed: {error}"),
            )
        })?
        .map_err(|error| {
            ImageLoadError::new("image_unavailable", format!("cannot read image: {error}"))
        })?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    Ok(LoadedSource {
        bytes,
        declared_mime: None,
        source_kind: Some("file"),
        source: Some(path.to_string_lossy().into_owned()),
        name,
    })
}

async fn load_url(source: &str) -> Result<LoadedSource, ImageLoadError> {
    let fetched = crate::remote_image::network::download(source, MAX_IMAGE_BYTES)
        .await
        .map_err(|error| {
            ImageLoadError::new(
                "image_download_failed",
                format!("cannot download image: {error}"),
            )
        })?;
    let parsed = reqwest::Url::parse(source).map_err(|error| {
        ImageLoadError::new(
            "image_invalid_source",
            format!("invalid image URL: {error}"),
        )
    })?;
    let name = parsed
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .filter(|name| !name.is_empty())
        .map(|name| urlencoding::decode(name).unwrap_or_default().into_owned());
    Ok(LoadedSource {
        bytes: fetched.bytes,
        declared_mime: None,
        source_kind: Some("url"),
        source: Some(source.into()),
        name,
    })
}

fn load_data_uri(source: &str) -> Result<LoadedSource, ImageLoadError> {
    let (header, payload) = source
        .split_once(',')
        .ok_or_else(|| ImageLoadError::new("image_invalid_source", "invalid Data URI"))?;
    let mime = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .ok_or_else(|| {
            ImageLoadError::new(
                "image_invalid_source",
                "image Data URI must use base64 encoding",
            )
        })?;
    Ok(LoadedSource {
        bytes: decode_base64(payload)?,
        declared_mime: Some(mime.into()),
        source_kind: None,
        source: None,
        name: None,
    })
}

fn decode_base64(value: &str) -> Result<Vec<u8>, ImageLoadError> {
    if value.len() > MAX_BASE64_LEN + 2 {
        return Err(ImageLoadError::new(
            "image_too_large",
            "image exceeds the 10 MiB limit",
        ));
    }
    STANDARD.decode(value).map_err(|error| {
        ImageLoadError::new(
            "image_invalid_base64",
            format!("invalid Base64 image: {error}"),
        )
    })
}

fn ensure_size(size: usize) -> Result<(), ImageLoadError> {
    if size > MAX_IMAGE_BYTES {
        Err(ImageLoadError::new(
            "image_too_large",
            "image exceeds the 10 MiB limit",
        ))
    } else {
        Ok(())
    }
}
