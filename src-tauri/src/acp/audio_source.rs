use std::path::{Component, Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tempfile::TempPath;

use crate::acp::audio_transcription::AudioToolFailure;
use crate::acp::delegation::transport::AudioTranscriptionSource;

#[path = "audio_source_convert.rs"]
mod convert;
#[path = "audio_source_network.rs"]
mod network;

pub(crate) struct PreparedAudioFile {
    pub(crate) file: tokio::fs::File,
    pub(crate) file_name: String,
    pub(crate) content_type: &'static str,
    pub(crate) size_bytes: u64,
    _temp_path: Option<TempPath>,
}

pub(super) struct LoadedAudio {
    pub(super) path: PathBuf,
    pub(super) file_name: String,
    pub(super) content_type: Option<&'static str>,
    pub(super) temp_path: Option<TempPath>,
}

pub(crate) async fn prepare(
    working_dir: &Path,
    source: &AudioTranscriptionSource,
    max_bytes: u64,
    flash: bool,
) -> Result<PreparedAudioFile, AudioToolFailure> {
    validate_source(source)?;
    let loaded = load(working_dir, source, max_bytes).await?;
    convert::enforce_duration(&loaded.path, flash).await?;
    let loaded = convert::normalize(loaded, flash).await?;
    let metadata = tokio::fs::metadata(&loaded.path)
        .await
        .map_err(|_| AudioToolFailure::invalid_source())?;
    validate_size(metadata.len(), max_bytes)?;
    let file = tokio::fs::File::open(&loaded.path)
        .await
        .map_err(|_| AudioToolFailure::invalid_source())?;
    Ok(PreparedAudioFile {
        file,
        file_name: loaded.file_name,
        content_type: loaded
            .content_type
            .ok_or_else(AudioToolFailure::unsupported_format)?,
        size_bytes: metadata.len(),
        _temp_path: loaded.temp_path,
    })
}

fn validate_source(source: &AudioTranscriptionSource) -> Result<(), AudioToolFailure> {
    let count = [&source.path, &source.url, &source.data]
        .into_iter()
        .filter(|value| value.is_some())
        .count();
    if count != 1 {
        return Err(AudioToolFailure::invalid_source());
    }
    if [&source.path, &source.url, &source.data]
        .into_iter()
        .any(|value| value.as_ref().is_some_and(|value| value.trim().is_empty()))
    {
        return Err(AudioToolFailure::invalid_source());
    }
    if let Some(name) = source.file_name.as_deref() {
        let invalid = name.trim().is_empty()
            || name.len() > 255
            || matches!(name, "." | "..")
            || name.contains(['/', '\\', '\0', '\r', '\n']);
        if invalid {
            return Err(AudioToolFailure::invalid_arguments());
        }
    }
    Ok(())
}

async fn load(
    working_dir: &Path,
    source: &AudioTranscriptionSource,
    max_bytes: u64,
) -> Result<LoadedAudio, AudioToolFailure> {
    if let Some(path) = source.path.as_deref() {
        return load_path(working_dir, path, max_bytes).await;
    }
    if let Some(url) = source.url.as_deref() {
        return network::download(url, source.file_name.as_deref(), max_bytes).await;
    }
    if let Some(data) = source.data.as_deref() {
        return load_data(
            data,
            source.file_name.as_deref(),
            source.mime_type.as_deref(),
            max_bytes,
        )
        .await;
    }
    Err(AudioToolFailure::invalid_source())
}

async fn load_path(
    working_dir: &Path,
    source: &str,
    max_bytes: u64,
) -> Result<LoadedAudio, AudioToolFailure> {
    let relative = parse_relative_path(source)?;
    let root = std::fs::canonicalize(working_dir).map_err(|_| AudioToolFailure::invalid_path())?;
    let path =
        std::fs::canonicalize(root.join(relative)).map_err(|_| AudioToolFailure::invalid_path())?;
    if !path.starts_with(&root) {
        return Err(AudioToolFailure::invalid_path());
    }
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AudioToolFailure::invalid_path())?;
    if !metadata.is_file() {
        return Err(AudioToolFailure::invalid_path());
    }
    validate_size(metadata.len(), max_bytes)?;
    let file_name = safe_file_name(&path)?;
    Ok(LoadedAudio {
        content_type: audio_content_type(&file_name, None),
        path,
        file_name,
        temp_path: None,
    })
}

async fn load_data(
    source: &str,
    file_name: Option<&str>,
    mime_type: Option<&str>,
    max_bytes: u64,
) -> Result<LoadedAudio, AudioToolFailure> {
    let file_name = file_name
        .ok_or_else(AudioToolFailure::invalid_arguments)?
        .trim();
    let (payload, declared_mime) = parse_data(source, mime_type)?;
    if payload.len() as u64 > max_base64_len(max_bytes) {
        return Err(AudioToolFailure::too_large());
    }
    let bytes = STANDARD
        .decode(payload.trim())
        .map_err(|_| AudioToolFailure::invalid_data())?;
    validate_size(bytes.len() as u64, max_bytes)?;
    let suffix = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str());
    let path = new_temp_path(suffix)?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|_| AudioToolFailure::invalid_data())?;
    Ok(LoadedAudio {
        path: path.to_path_buf(),
        file_name: file_name.to_string(),
        content_type: audio_content_type(file_name, declared_mime),
        temp_path: Some(path),
    })
}

fn parse_data<'a>(
    source: &'a str,
    mime_type: Option<&'a str>,
) -> Result<(&'a str, Option<&'a str>), AudioToolFailure> {
    if !source.starts_with("data:") {
        return mime_type
            .filter(|value| !value.trim().is_empty())
            .map(|value| (source, Some(value)))
            .ok_or_else(AudioToolFailure::invalid_arguments);
    }
    let (header, payload) = source
        .split_once(',')
        .ok_or_else(AudioToolFailure::invalid_data)?;
    let mime = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(AudioToolFailure::invalid_data)?;
    Ok((payload, Some(mime)))
}

fn parse_relative_path(source: &str) -> Result<PathBuf, AudioToolFailure> {
    let path = Path::new(source.trim());
    let valid = !source.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    valid
        .then(|| path.to_path_buf())
        .ok_or_else(AudioToolFailure::invalid_path)
}

pub(super) fn audio_content_type(file_name: &str, mime_type: Option<&str>) -> Option<&'static str> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(extension.as_deref(), Some("m4a")) {
        return Some("audio/m4a");
    }
    match normalized_mime(mime_type).as_deref() {
        Some("audio/wav" | "audio/x-wav" | "audio/wave") => return Some("audio/wav"),
        Some("audio/mpeg" | "audio/mp3") => return Some("audio/mpeg"),
        Some("audio/ogg" | "audio/opus" | "application/ogg") => return Some("audio/ogg"),
        Some("audio/m4a" | "audio/x-m4a") => return Some("audio/m4a"),
        Some("audio/mp4" | "video/mp4") => return Some("audio/mp4"),
        Some("audio/pcm" | "audio/l16" | "application/octet-stream") => return Some("audio/pcm"),
        _ => {}
    }
    match extension?.as_str() {
        "wav" | "wave" => Some("audio/wav"),
        "mp3" => Some("audio/mpeg"),
        "ogg" | "oga" | "opus" => Some("audio/ogg"),
        "mp4" => Some("audio/mp4"),
        "m4a" => Some("audio/m4a"),
        "m4b" => Some("audio/m4a"),
        "pcm" => Some("audio/pcm"),
        _ => None,
    }
}

pub(super) fn new_temp_path(extension: Option<&str>) -> Result<TempPath, AudioToolFailure> {
    let suffix = extension
        .filter(|value| !value.is_empty())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    tempfile::Builder::new()
        .prefix("iyw-audio-")
        .suffix(&suffix)
        .tempfile()
        .map(tempfile::NamedTempFile::into_temp_path)
        .map_err(|_| AudioToolFailure::invalid_source())
}

fn normalized_mime(value: Option<&str>) -> Option<String> {
    value
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn safe_file_name(path: &Path) -> Result<String, AudioToolFailure> {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(AudioToolFailure::invalid_path)
}

fn validate_size(size: u64, max_bytes: u64) -> Result<(), AudioToolFailure> {
    if size == 0 {
        Err(AudioToolFailure::invalid_source())
    } else if size > max_bytes {
        Err(AudioToolFailure::too_large())
    } else {
        Ok(())
    }
}

fn max_base64_len(max_bytes: u64) -> u64 {
    max_bytes.saturating_mul(4).div_ceil(3).saturating_add(4)
}
