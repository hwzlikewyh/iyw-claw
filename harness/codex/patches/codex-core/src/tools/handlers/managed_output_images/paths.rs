use std::collections::HashSet;
use std::env;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use codex_exec_server::GetMetadataOptions;
use codex_file_system::EnvironmentAccess;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_image::data_url_from_bytes;
use codex_utils_path_uri::PathUri;
use futures::StreamExt;
use regex_lite::Regex;

use crate::session::turn_context::TurnEnvironment;

use super::{
    DATA_DIR_ENV, IMAGE_PATH_PATTERN, IMAGE_PATH_REGEX, MAX_IMAGE_PATH_MATCHES,
    MAX_OUTPUT_IMAGE_BYTES, MAX_OUTPUT_IMAGES, NativeOutputImage, NativeOutputImageCapture,
};

pub(super) async fn capture_paths(
    sources: &[&str],
    environment: &TurnEnvironment,
) -> NativeOutputImageCapture {
    if environment.environment.is_remote() {
        return NativeOutputImageCapture::default();
    }
    let paths = referenced_paths(sources);
    if paths.is_empty() {
        return NativeOutputImageCapture::default();
    }
    let Some(scope) = active_output_scope(environment.cwd()) else {
        return NativeOutputImageCapture::default();
    };

    let fs = environment.fs_accessor(None);
    let mut capture = NativeOutputImageCapture::default();
    let mut seen = HashSet::new();
    let mut total_bytes = 0_usize;
    let started = Instant::now();
    for candidate in paths {
        let Some(path) = safe_output_path(&scope, &candidate) else {
            continue;
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        if capture.images.len() >= MAX_OUTPUT_IMAGES {
            capture.omitted += 1;
            continue;
        }
        let remaining_bytes = MAX_OUTPUT_IMAGE_BYTES - total_bytes;
        let Some(image) = read_image(&fs, &path, remaining_bytes).await else {
            capture.omitted += 1;
            continue;
        };
        total_bytes += image.size_bytes;
        capture.images.push(image);
    }
    log_capture(&capture, total_bytes, started);
    capture
}

fn log_capture(capture: &NativeOutputImageCapture, total_bytes: usize, started: Instant) {
    if !capture.is_empty() {
        tracing::info!(
            image_count = capture.images.len(),
            image_bytes = total_bytes,
            omitted_count = capture.omitted,
            elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            "prepared managed output images as native model input"
        );
    }
}

fn active_output_scope(cwd: &PathUri) -> Option<OutputScope> {
    let data_dir = env::var_os(DATA_DIR_ENV).filter(|value| !value.is_empty())?;
    let data_dir = dunce::canonicalize(PathBuf::from(data_dir)).ok()?;
    let chat_root = dunce::canonicalize(data_dir.join("chat-sessions")).ok()?;
    let cwd = dunce::canonicalize(cwd.to_abs_path().ok()?).ok()?;
    let relative = cwd.strip_prefix(chat_root).ok()?;
    if !is_chat_directory(relative) {
        return None;
    }

    let output = cwd.join("output");
    let metadata = std::fs::symlink_metadata(&output).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    let output = dunce::canonicalize(output).ok()?;
    (output.parent() == Some(cwd.as_path())).then(|| OutputScope { cwd, output })
}

struct OutputScope {
    cwd: PathBuf,
    output: PathBuf,
}

fn is_chat_directory(relative: &Path) -> bool {
    let parts: Vec<_> = relative.components().collect();
    let [Component::Normal(date), Component::Normal(id)] = parts.as_slice() else {
        return false;
    };
    let Some(date) = date.to_str() else {
        return false;
    };
    let Some(id) = id.to_str() else {
        return false;
    };
    valid_date(date) && uuid::Uuid::parse_str(id).is_ok()
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn referenced_paths(sources: &[&str]) -> Vec<PathBuf> {
    let Some(regex) = IMAGE_PATH_REGEX
        .get_or_init(|| Regex::new(IMAGE_PATH_PATTERN).ok())
        .as_ref()
    else {
        return Vec::new();
    };
    sources
        .iter()
        .flat_map(|text| regex.find_iter(text))
        .take(MAX_IMAGE_PATH_MATCHES)
        .map(|matched| PathBuf::from(matched.as_str()))
        .collect()
}

fn safe_output_path(scope: &OutputScope, candidate: &Path) -> Option<PathBuf> {
    let candidate = if candidate.is_absolute() {
        dunce::simplified(candidate).to_path_buf()
    } else {
        scope.cwd.join(candidate)
    };
    let relative = candidate.strip_prefix(&scope.output).ok()?;
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let mut current = scope.output.clone();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return None;
        };
        current.push(component);
        if std::fs::symlink_metadata(&current)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
    }
    let canonical = dunce::canonicalize(current).ok()?;
    canonical.starts_with(&scope.output).then_some(canonical)
}

async fn read_image(
    fs: &(impl EnvironmentAccess + ?Sized),
    path: &Path,
    max_bytes: usize,
) -> Option<NativeOutputImage> {
    let path = PathUri::from_abs_path(&AbsolutePathBuf::from_absolute_path(path).ok()?);
    let metadata = fs
        .get_metadata(
            &path,
            GetMetadataOptions {
                follow_symlinks: false,
            },
        )
        .await
        .ok()?;
    if !metadata.is_file || metadata.is_symlink || metadata.size > max_bytes as u64 {
        return None;
    }
    let mut stream = fs.read_file_stream(&path).await.ok()?;
    let mut bytes = Vec::with_capacity(metadata.size as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let mime_type = match image::guess_format(&bytes).ok()? {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Gif => "image/gif",
        _ => return None,
    };
    Some(NativeOutputImage {
        data_url: data_url_from_bytes(mime_type, &bytes),
        size_bytes: bytes.len(),
    })
}
