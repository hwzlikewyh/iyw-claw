use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::app_error::AppCommandError;

pub const DISPLAY_ASSET_URI_PREFIX: &str = "iyw-claw://display-assets/";
const DISPLAY_ASSETS_DIR: &str = "display-assets";

const IMAGE_FORMATS: [(&str, &str); 7] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
    ("image/bmp", "bmp"),
    ("image/avif", "avif"),
    ("image/svg+xml", "svg"),
];

#[derive(Debug, Clone)]
pub struct StoredDisplayAsset {
    pub uri: String,
    pub mime_type: &'static str,
}

#[derive(Debug)]
pub struct DisplayAsset {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
}

pub async fn store(bytes: Vec<u8>, mime_type: &str) -> Result<StoredDisplayAsset, String> {
    let mime_type = normalized_format(mime_type)
        .map(|(mime, _)| mime)
        .ok_or_else(|| "unsupported display image format".to_string())?;
    if bytes.is_empty() || bytes.len() > crate::acp::delegation::image_loader::MAX_IMAGE_BYTES {
        return Err("display image size is invalid".to_string());
    }
    if crate::acp::delegation::image_format::detect_mime(&bytes) != Some(mime_type) {
        return Err("display image content does not match its declared format".to_string());
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let path = asset_path(&hash, mime_type)?;
    let write_path = path.clone();
    let created = tokio::task::spawn_blocking(move || write_once(&write_path, &bytes))
        .await
        .map_err(|error| format!("display asset write task failed: {error}"))??;
    tracing::info!(
        asset_hash = %hash_prefix(&hash),
        bytes = bytes_len(&path),
        mime_type,
        created,
        "[display-assets] image stored"
    );
    Ok(StoredDisplayAsset {
        uri: format!("{DISPLAY_ASSET_URI_PREFIX}{hash}"),
        mime_type,
    })
}

pub async fn read(hash: &str) -> Result<DisplayAsset, AppCommandError> {
    validate_hash(hash)?;
    let (path, mime_type) = find_asset(hash).ok_or_else(|| {
        AppCommandError::not_found("Display image is unavailable")
            .with_detail(format!("asset_hash={}", hash_prefix(hash)))
    })?;
    let bytes = tokio::fs::read(&path).await.map_err(|error| {
        AppCommandError::io_error("Failed to read display image").with_detail(error.to_string())
    })?;
    let detected = crate::acp::delegation::image_format::detect_mime(&bytes);
    if detected != Some(mime_type) {
        return Err(AppCommandError::invalid_input(
            "Display image content does not match its stored format",
        ));
    }
    Ok(DisplayAsset { bytes, mime_type })
}

pub fn hash_from_uri(uri: &str) -> Result<&str, AppCommandError> {
    let hash = uri
        .strip_prefix(DISPLAY_ASSET_URI_PREFIX)
        .ok_or_else(|| AppCommandError::invalid_input("Invalid display image URI"))?;
    validate_hash(hash)?;
    Ok(hash)
}

pub fn default_name(mime_type: &str) -> String {
    let extension = match mime_type {
        "image/jpeg" => "jpg",
        "image/svg+xml" => "svg",
        _ => mime_type.strip_prefix("image/").unwrap_or("img"),
    };
    format!("image.{extension}")
}

fn display_assets_root() -> PathBuf {
    if let Some(data) = std::env::var_os("IYW_CLAW_DATA_DIR").filter(|value| !value.is_empty()) {
        return PathBuf::from(data).join(DISPLAY_ASSETS_DIR);
    }
    crate::paths::iyw_claw_home_dir().join(DISPLAY_ASSETS_DIR)
}

fn asset_path(hash: &str, mime_type: &str) -> Result<PathBuf, String> {
    let extension = normalized_format(mime_type)
        .map(|(_, extension)| extension)
        .ok_or_else(|| "unsupported display image format".to_string())?;
    Ok(display_assets_root().join(format!("{hash}.{extension}")))
}

fn find_asset(hash: &str) -> Option<(PathBuf, &'static str)> {
    let root = display_assets_root();
    IMAGE_FORMATS.iter().find_map(|(mime_type, extension)| {
        let path = root.join(format!("{hash}.{extension}"));
        path.is_file().then_some((path, *mime_type))
    })
}

fn write_once(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    if path.is_file() {
        return Ok(false);
    }
    let parent = path
        .parent()
        .ok_or_else(|| "display asset path has no parent".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create display asset directory: {error}"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("cannot create display asset temp file: {error}"))?;
    temp.write_all(bytes)
        .map_err(|error| format!("cannot write display asset: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("cannot sync display asset: {error}"))?;
    match temp.persist_noclobber(path) {
        Ok(_) => Ok(true),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(format!("cannot publish display asset: {}", error.error)),
    }
}

fn normalized_format(mime_type: &str) -> Option<(&'static str, &'static str)> {
    let normalized = crate::acp::delegation::image_format::normalize_mime(mime_type)?;
    IMAGE_FORMATS
        .iter()
        .find(|(candidate, _)| *candidate == normalized)
        .copied()
}

fn validate_hash(hash: &str) -> Result<(), AppCommandError> {
    if hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(AppCommandError::invalid_input(
        "Invalid display image identifier",
    ))
}

fn hash_prefix(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

fn bytes_len(path: &Path) -> u64 {
    path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
}
