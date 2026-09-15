use axum::{
    body::Body,
    extract::{Path, Request},
    http::{header, StatusCode},
    response::Response,
};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use super::{is_asset, resources};
use crate::{app_error::AppCommandError, commands::folders::open_no_follow};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16_000_000;
static READ_SLOTS: std::sync::LazyLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(tokio::sync::Semaphore::new(8)));

pub async fn serve(
    Path((id, name)): Path<(String, String)>,
    request: Request,
) -> Result<Response, AppCommandError> {
    let resource = resources()
        .get(&id)
        .cloned()
        .ok_or_else(|| AppCommandError::not_found("Preview expired"))?;
    let permit = tokio::select! {
        _ = resource.cancelled.cancelled() => return Err(AppCommandError::not_found("Preview closed")),
        slot = READ_SLOTS.clone().acquire_owned() => slot.map_err(|_| AppCommandError::invalid_input("Preview read unavailable"))?,
    };
    let candidate = if name == "file" {
        resource.target.clone()
    } else if resource.assets {
        resource
            .target
            .parent()
            .unwrap_or(&resource.root)
            .join(&name)
    } else {
        return Err(AppCommandError::not_found("Preview asset not found"));
    };
    let target = tokio::fs::canonicalize(candidate)
        .await
        .map_err(AppCommandError::io)?;
    if !target.starts_with(&resource.root) || (target != resource.target && !is_asset(&target)) {
        return Err(AppCommandError::invalid_input(
            "Preview asset outside allowed scope",
        ));
    }
    let path = target.clone();
    let opened = tokio::task::spawn_blocking(move || {
        let file = open_no_follow(&path)?;
        let metadata = file.metadata()?;
        validate_image(&file, &path)?;
        Ok::<_, std::io::Error>((file, metadata))
    })
    .await
    .map_err(|_| AppCommandError::invalid_input("Preview file open failed"))?
    .map_err(AppCommandError::io)?;
    let (file, metadata) = opened;
    let limit = super::byte_limit(&target);
    if !metadata.is_file() || metadata.len() > limit {
        return Err(AppCommandError::invalid_input(
            "Preview asset exceeds limit",
        ));
    }
    if target == resource.target && (metadata.len(), metadata.modified().ok()) != resource.version {
        return response(StatusCode::CONFLICT, Body::empty());
    }
    if resource.cancelled.is_cancelled() {
        return Err(AppCommandError::not_found("Preview closed"));
    }
    let response = stream_file(file, metadata, target, request, resource.cancelled.clone()).await?;
    let (parts, body) = response.into_parts();
    let stream = body.into_data_stream().map(move |chunk| {
        let _ = &permit;
        chunk
    });
    Ok(Response::from_parts(parts, Body::from_stream(stream)))
}

async fn stream_file(
    file: std::fs::File,
    metadata: std::fs::Metadata,
    target: std::path::PathBuf,
    request: Request,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Response, AppCommandError> {
    let size = metadata.len();
    let stamp = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let etag = format!("\"{size:x}-{stamp:x}\"");
    let range = request.headers().get(header::RANGE).filter(|_| {
        request
            .headers()
            .get(header::IF_RANGE)
            .is_none_or(|value| value == etag.as_str())
    });
    let bounds = match range.map(|value| {
        value
            .to_str()
            .ok()
            .and_then(|value| http_range_header::parse_range_header(value).ok())
            .and_then(|ranges| ranges.validate(size).ok())
    }) {
        None => None,
        Some(Some(ranges)) if ranges.len() == 1 => ranges.into_iter().next(),
        _ => {
            let mut result = response(StatusCode::RANGE_NOT_SATISFIABLE, Body::empty())?;
            result.headers_mut().insert(
                header::CONTENT_RANGE,
                format!("bytes */{size}").parse().unwrap(),
            );
            return Ok(result);
        }
    };
    let (start, length) = bounds
        .as_ref()
        .map(|range| (*range.start(), range.end() - range.start() + 1))
        .unwrap_or((0, size));
    let mut file = tokio::fs::File::from_std(file);
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(AppCommandError::io)?;
    let body = ReaderStream::with_capacity(file.take(length), STREAM_BUFFER_BYTES)
        .take_until(cancel.cancelled_owned());
    let mut result = response(
        if bounds.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        },
        Body::from_stream(body),
    )?;
    let headers = result.headers_mut();
    headers.insert(header::CONTENT_TYPE, mime(&target).parse().unwrap());
    headers.insert(header::CONTENT_LENGTH, length.into());
    headers.insert(header::ETAG, etag.parse().unwrap());
    if super::extension(&target) == "svg" {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:"
                .parse()
                .unwrap(),
        );
    }
    if let Some(range) = bounds {
        headers.insert(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{size}", range.start(), range.end())
                .parse()
                .unwrap(),
        );
    }
    Ok(result)
}

fn response(status: StatusCode, body: Body) -> Result<Response, AppCommandError> {
    Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_DISPOSITION, "inline")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(body)
        .map_err(|_| AppCommandError::invalid_input("Invalid preview response"))
}

fn mime(path: &std::path::Path) -> &'static str {
    match super::extension(path).as_str() {
        "pdf" => "application/pdf",
        "gltf" => "model/gltf+json",
        "glb" => "model/gltf-binary",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ktx2" => "image/ktx2",
        _ => "application/octet-stream",
    }
}

fn validate_image(file: &std::fs::File, path: &std::path::Path) -> std::io::Result<()> {
    if !matches!(
        super::extension(path).as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif"
    ) {
        return Ok(());
    }
    let reader = image::ImageReader::new(std::io::BufReader::new(file.try_clone()?))
        .with_guessed_format()?;
    let (width, height) = reader.into_dimensions().map_err(std::io::Error::other)?;
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(std::io::Error::other("Image exceeds preview pixel limit"));
    }
    Ok(())
}
