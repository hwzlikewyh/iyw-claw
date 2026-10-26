//! 聊天附件的源格式；模型输入仍限制为 PNG/JPEG/GIF/WebP。
use std::path::Path;

use crate::app_error::AppCommandError;

const SOURCE_FORMATS: &[(&str, &[&str], &[&str])] = &[
    (
        "image/png",
        &["png", "apng"],
        &["image/apng", "image/x-png"],
    ),
    (
        "image/jpeg",
        &["jpg", "jpeg", "jfif", "jpe", "jif", "jfi", "pjpeg", "pjp"],
        &["image/jpg", "image/pjpeg", "image/jfif"],
    ),
    ("image/gif", &["gif"], &[]),
    ("image/webp", &["webp"], &[]),
    ("image/bmp", &["bmp"], &["image/x-bmp", "image/x-ms-bmp"]),
    (
        "image/tiff",
        &["tif", "tiff", "btf", "tf8"],
        &["image/x-tiff"],
    ),
    (
        "image/vnd.microsoft.icon",
        &["ico", "cur"],
        &["image/x-icon", "image/ico"],
    ),
    ("image/svg+xml", &["svg"], &[]),
    (
        "image/heic",
        &["heic", "heif", "hif", "heics", "heifs"],
        &["image/heif", "image/heic-sequence", "image/heif-sequence"],
    ),
    ("image/avif", &["avif", "avifs"], &[]),
    ("image/x-tga", &["tga"], &["image/tga", "image/x-targa"]),
    (
        "image/x-portable-anymap",
        &["pbm", "pgm", "ppm", "pnm", "pam"],
        &[
            "image/x-portable-bitmap",
            "image/x-portable-graymap",
            "image/x-portable-pixmap",
            "image/x-portable-arbitrarymap",
        ],
    ),
    ("image/qoi", &["qoi"], &["image/x-qoi"]),
    ("image/vnd-ms.dds", &["dds"], &["image/x-dds"]),
];

pub(crate) fn source_mime(raw: &str) -> Option<&'static str> {
    let mime = raw.split(';').next()?.trim().to_ascii_lowercase();
    SOURCE_FORMATS
        .iter()
        .find(|(canonical, _, aliases)| *canonical == mime || aliases.contains(&mime.as_str()))
        .map(|(canonical, _, _)| *canonical)
}

pub(crate) fn source_mime_for_name(name: &str) -> Option<&'static str> {
    let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    SOURCE_FORMATS
        .iter()
        .find(|(_, extensions, _)| extensions.contains(&extension.as_str()))
        .map(|(canonical, _, _)| *canonical)
}

pub(crate) fn upload_mime(name: &str, raw_mime: &str) -> Result<&'static str, AppCommandError> {
    let named = source_mime_for_name(name);
    let raw = raw_mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let supplied = if raw.is_empty() || raw == "application/octet-stream" {
        None
    } else {
        Some(source_mime(&raw).ok_or_else(unsupported_source)?)
    };
    if named
        .zip(supplied)
        .is_some_and(|(named, supplied)| named != supplied)
    {
        return Err(AppCommandError::invalid_input(
            "Image MIME type does not match its extension",
        ));
    }
    supplied.or(named).ok_or_else(unsupported_source)
}

pub(crate) fn unsupported_source() -> AppCommandError {
    AppCommandError::invalid_input("Image format is not supported; convert it to PNG or JPEG")
        .with_i18n("errors.chatImage.unsupported", Default::default())
}

pub(super) fn decode_error(error: impl std::fmt::Display) -> AppCommandError {
    tracing::warn!(target: "chat.image", reason = %error, "source image decoding failed");
    AppCommandError::invalid_input(
        "Image could not be decoded; it may be damaged or use an unsupported encoding",
    )
    .with_i18n("errors.chatImage.decodeFailed", Default::default())
}

pub(super) fn raster_error(error: image::ImageError) -> AppCommandError {
    if matches!(error, image::ImageError::Limits(_)) {
        return dimension_error();
    }
    decode_error(error)
}

pub(super) fn dimension_error() -> AppCommandError {
    AppCommandError::invalid_input("Image dimensions or decoding memory exceed the limits")
        .with_i18n("errors.chatImage.dimensions", Default::default())
}
