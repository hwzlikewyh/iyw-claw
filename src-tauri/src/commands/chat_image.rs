use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits,
};
use serde::Serialize;

use crate::app_error::AppCommandError;

pub const CHAT_IMAGE_SOURCE_MAX_BYTES: u64 = 100 * 1024 * 1024;
pub const CHAT_IMAGE_DERIVED_MAX_BYTES: usize = 10 * 1024 * 1024;
pub const CHAT_IMAGE_I18N_KEY_TOO_LARGE: &str = "errors.chatImage.tooLarge";
const CHAT_IMAGE_MAX_EDGE: u32 = 2048;
const CHAT_IMAGE_DECODE_MAX_EDGE: u32 = 16_384;
const CHAT_IMAGE_DECODE_MAX_ALLOC: u64 = 512 * 1024 * 1024;
const RESIZE_NUMERATOR: u32 = 3;
const RESIZE_DENOMINATOR: u32 = 4;
const MAX_RESIZE_ATTEMPTS: usize = 6;
const JPEG_QUALITIES: [u8; 4] = [85, 70, 55, 40];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedChatImage {
    pub data: String,
    pub mime_type: String,
    pub name: String,
    pub source_bytes: u64,
    pub derived_bytes: usize,
    pub width: u32,
    pub height: u32,
}

struct EncodedImage {
    bytes: Vec<u8>,
    mime_type: &'static str,
    width: u32,
    height: u32,
}

fn image_error(message: &'static str, error: impl std::fmt::Display) -> AppCommandError {
    AppCommandError::invalid_input(message).with_detail(error.to_string())
}

pub(crate) fn source_too_large_error(file_name: &str, source_bytes: u64) -> AppCommandError {
    let mib = 1024.0 * 1024.0;
    let params = BTreeMap::from([
        ("name".to_string(), file_name.to_string()),
        (
            "size".to_string(),
            format!("{:.1}", source_bytes as f64 / mib),
        ),
        (
            "limit".to_string(),
            (CHAT_IMAGE_SOURCE_MAX_BYTES / (1024 * 1024)).to_string(),
        ),
    ]);
    AppCommandError::invalid_input("Image exceeds the 100 MB source limit")
        .with_i18n(CHAT_IMAGE_I18N_KEY_TOO_LARGE, params)
}

fn supported_mime(format: ImageFormat) -> Result<&'static str, AppCommandError> {
    match format {
        ImageFormat::Png => Ok("image/png"),
        ImageFormat::Jpeg => Ok("image/jpeg"),
        ImageFormat::WebP => Ok("image/webp"),
        ImageFormat::Gif => Ok("image/gif"),
        _ => Err(AppCommandError::invalid_input(
            "Image format is not supported",
        )),
    }
}

fn decode_image(bytes: &[u8]) -> Result<(DynamicImage, ImageFormat, Orientation), AppCommandError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| image_error("Unable to inspect image", error))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(CHAT_IMAGE_DECODE_MAX_EDGE);
    limits.max_image_height = Some(CHAT_IMAGE_DECODE_MAX_EDGE);
    limits.max_alloc = Some(CHAT_IMAGE_DECODE_MAX_ALLOC);
    reader.limits(limits);
    let format = reader
        .format()
        .ok_or_else(|| AppCommandError::invalid_input("Image format is not supported"))?;
    supported_mime(format)?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| image_error("Unable to decode image", error))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| image_error("Unable to read image orientation", error))?;
    let image = DynamicImage::from_decoder(decoder)
        .map_err(|error| image_error("Unable to decode image", error))?;
    Ok((image, format, orientation))
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, AppCommandError> {
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|error| image_error("Unable to encode image", error))?;
    Ok(cursor.into_inner())
}

fn encode_jpeg(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, AppCommandError> {
    let rgb = image.to_rgb8();
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| image_error("Unable to encode image", error))?;
    Ok(bytes)
}

fn shrink(image: &DynamicImage) -> DynamicImage {
    let width = (image.width() * RESIZE_NUMERATOR / RESIZE_DENOMINATOR).max(1);
    let height = (image.height() * RESIZE_NUMERATOR / RESIZE_DENOMINATOR).max(1);
    image.resize(width, height, image::imageops::FilterType::Lanczos3)
}

fn encode_derived(mut image: DynamicImage) -> Result<EncodedImage, AppCommandError> {
    for attempt in 0..MAX_RESIZE_ATTEMPTS {
        if image.has_alpha() {
            let bytes = encode_png(&image)?;
            if bytes.len() <= CHAT_IMAGE_DERIVED_MAX_BYTES {
                return Ok(EncodedImage {
                    bytes,
                    mime_type: "image/png",
                    width: image.width(),
                    height: image.height(),
                });
            }
        } else {
            for quality in JPEG_QUALITIES {
                let bytes = encode_jpeg(&image, quality)?;
                if bytes.len() <= CHAT_IMAGE_DERIVED_MAX_BYTES {
                    return Ok(EncodedImage {
                        bytes,
                        mime_type: "image/jpeg",
                        width: image.width(),
                        height: image.height(),
                    });
                }
            }
        }
        if attempt + 1 < MAX_RESIZE_ATTEMPTS {
            image = shrink(&image);
        }
    }
    Err(AppCommandError::invalid_input(
        "Image cannot be reduced to the attachment limit",
    ))
}

fn prepare_sync(
    path: &Path,
    source_bytes: u64,
    name: String,
) -> Result<PreparedChatImage, AppCommandError> {
    let bytes = std::fs::read(path).map_err(|error| {
        AppCommandError::io_error("Unable to read image").with_detail(error.to_string())
    })?;
    if bytes.len() as u64 > CHAT_IMAGE_SOURCE_MAX_BYTES {
        return Err(source_too_large_error(&name, bytes.len() as u64));
    }

    let (mut image, format, orientation) = decode_image(&bytes)?;
    let source_mime = supported_mime(format)?;
    let can_reuse = format != ImageFormat::Gif
        && bytes.len() <= CHAT_IMAGE_DERIVED_MAX_BYTES
        && image.width() <= CHAT_IMAGE_MAX_EDGE
        && image.height() <= CHAT_IMAGE_MAX_EDGE
        && orientation == Orientation::NoTransforms;
    let encoded = if can_reuse {
        EncodedImage {
            bytes,
            mime_type: source_mime,
            width: image.width(),
            height: image.height(),
        }
    } else {
        image.apply_orientation(orientation);
        image = image.thumbnail(CHAT_IMAGE_MAX_EDGE, CHAT_IMAGE_MAX_EDGE);
        encode_derived(image)?
    };

    Ok(PreparedChatImage {
        data: BASE64.encode(&encoded.bytes),
        mime_type: encoded.mime_type.to_string(),
        name,
        source_bytes,
        derived_bytes: encoded.bytes.len(),
        width: encoded.width,
        height: encoded.height,
    })
}

pub async fn prepare_chat_image_core(path: PathBuf) -> Result<PreparedChatImage, AppCommandError> {
    if !path.is_absolute() {
        return Err(AppCommandError::invalid_input(
            "Image path must be absolute",
        ));
    }
    let metadata = tokio::fs::metadata(&path).await.map_err(|error| {
        AppCommandError::io_error("Unable to inspect image").with_detail(error.to_string())
    })?;
    if !metadata.is_file() {
        return Err(AppCommandError::invalid_input("Image path is not a file"));
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    if metadata.len() > CHAT_IMAGE_SOURCE_MAX_BYTES {
        return Err(source_too_large_error(&name, metadata.len()));
    }
    let log_name = name.clone();
    let source_bytes = metadata.len();
    let started = Instant::now();
    let prepared = tokio::task::spawn_blocking(move || prepare_sync(&path, source_bytes, name))
        .await
        .map_err(|error| {
            AppCommandError::task_execution_failed("Image processing task failed")
                .with_detail(error.to_string())
        })??;
    tracing::info!(
        target: "chat.image",
        file_name = %log_name,
        mime_type = %prepared.mime_type,
        source_bytes = prepared.source_bytes,
        derived_bytes = prepared.derived_bytes,
        width = prepared.width,
        height = prepared.height,
        elapsed_ms = started.elapsed().as_millis(),
        "prepared chat image"
    );
    Ok(prepared)
}

#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn prepare_chat_image(path: String) -> Result<PreparedChatImage, AppCommandError> {
    prepare_chat_image_core(PathBuf::from(path)).await
}
