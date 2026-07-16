use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};

pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BASE64_LEN: usize = MAX_IMAGE_BYTES.div_ceil(3) * 4;
const SIZE_ERROR: &str = "image exceeds the 10 MiB limit";
#[derive(Debug, Deserialize)]
struct ImageArguments {
    source: String,
    mime_type: Option<String>,
    caption: Option<String>,
    name: Option<String>,
}
struct LoadedSource {
    bytes: Vec<u8>,
    declared_mime: Option<String>,
    source_kind: Option<&'static str>,
    source: Option<String>,
    name: Option<String>,
}
struct LoadedImage {
    bytes: Vec<u8>,
    mime_type: &'static str,
    metadata: Value,
}
pub async fn execute(arguments: Value, working_dir: PathBuf) -> Value {
    match load_image(arguments, &working_dir).await {
        Ok(image) => image.success_result(),
        Err(error) => json!({
            "content": [{ "type": "text", "text": error }],
            "isError": true,
        }),
    }
}

async fn load_image(arguments: Value, working_dir: &Path) -> Result<LoadedImage, String> {
    let args: ImageArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("invalid show_image arguments: {error}"))?;
    validate_text_fields(&args)?;
    let loaded = load_source(&args, working_dir).await?;
    ensure_size(loaded.bytes.len())?;
    let detected = detect_mime(&loaded.bytes).ok_or("unsupported image data")?;
    let declared = args
        .mime_type
        .as_deref()
        .or(loaded.declared_mime.as_deref());
    if let Some(mime) = declared {
        let normalized =
            normalize_mime(mime).ok_or_else(|| format!("unsupported MIME type: {mime}"))?;
        if normalized != detected {
            return Err(format!(
                "declared MIME type {mime} does not match {detected} image data"
            ));
        }
    }
    let extension = match detected {
        "image/jpeg" => "jpg",
        "image/svg+xml" => "svg",
        _ => detected.strip_prefix("image/").unwrap_or("img"),
    };
    let name = args
        .name
        .or(loaded.name)
        .unwrap_or_else(|| format!("image.{extension}"));
    Ok(LoadedImage {
        bytes: loaded.bytes,
        mime_type: detected,
        metadata: json!({
            "type": "iyw_claw_display_image", "caption": args.caption, "name": name,
            "source_kind": loaded.source_kind, "source": loaded.source,
        }),
    })
}

fn validate_text_fields(args: &ImageArguments) -> Result<(), String> {
    if args.source.trim().is_empty() {
        return Err("source must not be empty".into());
    }
    if args
        .name
        .as_ref()
        .is_some_and(|name| name.chars().count() > 255)
    {
        return Err("name must not exceed 255 characters".into());
    }
    if args
        .caption
        .as_ref()
        .is_some_and(|caption| caption.chars().count() > 2000)
    {
        return Err("caption must not exceed 2000 characters".into());
    }
    Ok(())
}

async fn load_source(args: &ImageArguments, working_dir: &Path) -> Result<LoadedSource, String> {
    let source = args.source.trim();
    if source.starts_with("data:") {
        return load_data_uri(source);
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        return load_url(source).await;
    }
    let path = resolve_path(source, working_dir)?;
    if path.exists() || args.mime_type.is_none() {
        return load_file(path).await;
    }
    let mime = args
        .mime_type
        .clone()
        .ok_or("raw Base64 requires mime_type")?;
    Ok(LoadedSource {
        bytes: decode_base64(source)?,
        declared_mime: Some(mime),
        source_kind: None,
        source: None,
        name: None,
    })
}

fn resolve_path(source: &str, working_dir: &Path) -> Result<PathBuf, String> {
    if source.starts_with("file:") {
        return reqwest::Url::parse(source)
            .map_err(|error| format!("invalid file URI: {error}"))?
            .to_file_path()
            .map_err(|_| "invalid file URI path".to_string());
    }
    let path = PathBuf::from(source);
    Ok(if path.is_absolute() {
        path
    } else {
        working_dir.join(path)
    })
}

async fn load_file(path: PathBuf) -> Result<LoadedSource, String> {
    let path = std::fs::canonicalize(&path)
        .map_err(|error| format!("cannot open image {}: {error}", path.display()))?;
    let size = std::fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len() as usize;
    ensure_size(size)?;
    let read_path = path.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(read_path))
        .await
        .map_err(|error| format!("image read task failed: {error}"))?
        .map_err(|error| format!("cannot read image: {error}"))?;
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

async fn load_url(source: &str) -> Result<LoadedSource, String> {
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|error| format!("cannot create HTTP client: {error}"))?;
    let response = client
        .get(source)
        .send()
        .await
        .map_err(|error| format!("cannot download image: {error}"))?
        .error_for_status()
        .map_err(|error| format!("cannot download image: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_IMAGE_BYTES as u64)
    {
        return Err(SIZE_ERROR.into());
    }
    let declared_mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_mime)
        .map(str::to_string);
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.map_err(|error| format!("cannot download image: {error}"))?);
        ensure_size(bytes.len())?;
    }
    let parsed =
        reqwest::Url::parse(source).map_err(|error| format!("invalid image URL: {error}"))?;
    let name = parsed
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .filter(|name| !name.is_empty())
        .map(|name| urlencoding::decode(name).unwrap_or_default().into_owned());
    Ok(LoadedSource {
        bytes,
        declared_mime,
        source_kind: Some("url"),
        source: Some(source.into()),
        name,
    })
}

fn load_data_uri(source: &str) -> Result<LoadedSource, String> {
    let (header, payload) = source.split_once(',').ok_or("invalid Data URI")?;
    let mime = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .ok_or("image Data URI must use base64 encoding")?;
    Ok(LoadedSource {
        bytes: decode_base64(payload)?,
        declared_mime: Some(mime.into()),
        source_kind: None,
        source: None,
        name: None,
    })
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    if value.len() > MAX_BASE64_LEN + 2 {
        return Err(SIZE_ERROR.into());
    }
    STANDARD
        .decode(value)
        .map_err(|error| format!("invalid Base64 image: {error}"))
}

fn ensure_size(size: usize) -> Result<(), String> {
    if size > MAX_IMAGE_BYTES {
        Err(SIZE_ERROR.into())
    } else {
        Ok(())
    }
}
fn normalize_mime(value: &str) -> Option<&'static str> {
    let mime = value.split(';').next()?.trim().to_ascii_lowercase();
    match mime.as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        "image/bmp" => Some("image/bmp"),
        "image/avif" => Some("image/avif"),
        "image/svg+xml" => Some("image/svg+xml"),
        _ => None,
    }
}
fn detect_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && bytes[8..]
            .chunks_exact(4)
            .any(|brand| brand == b"avif" || brand == b"avis")
    {
        return Some("image/avif");
    }
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    if text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg")) {
        return Some("image/svg+xml");
    }
    None
}

impl LoadedImage {
    fn success_result(self) -> Value {
        json!({
            "content": [
                { "type": "text", "text": self.metadata.to_string() },
                { "type": "image", "data": STANDARD.encode(self.bytes), "mimeType": self.mime_type },
            ],
            "isError": false,
        })
    }
}
