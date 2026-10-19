use super::json_scan::{ImageFormat, ImageSlot, Value};

pub(super) fn slot(object: &Value, editing: bool, prepare: bool) -> Option<ImageSlot> {
    let format = match object.text("type") {
        Some("input_image") => ImageFormat::Responses,
        Some("image_url") => ImageFormat::Chat,
        Some("image") => ImageFormat::Messages,
        _ if editing => ImageFormat::Edit,
        _ if prepare && object.text("operation") == Some("upload") => ImageFormat::Responses,
        _ => return None,
    };
    if matches!(format, ImageFormat::Messages)
        && object.get("source")?.text("type") != Some("base64")
    {
        return None;
    }
    let value = object
        .get("source")
        .and_then(|source| source.get("data"))
        .or_else(|| {
            object
                .get("image_url")
                .map(|url| url.get("url").unwrap_or(url))
        })
        .or_else(|| prepare.then(|| object.get("data")).flatten())?;
    if value.span.end < value.span.start + 2 {
        return None;
    }
    let raw = object
        .get("source")
        .is_some_and(|source| source.text("type") == Some("base64"))
        || prepare;
    if !value.inline && !raw {
        return None;
    }
    Some(ImageSlot {
        encoded: value.span.clone(),
        replace: object
            .get("source")
            .map(|source| source.span.clone())
            .unwrap_or_else(|| value.span.clone()),
        mime: declared_mime(object),
        format,
        source_extra: object
            .get("source")
            .map(|source| source.source_extra.clone())
            .unwrap_or_default(),
    })
}

fn declared_mime(object: &Value) -> Option<String> {
    object
        .get("source")
        .and_then(|source| source.text("media_type"))
        .or_else(|| object.text("mime_type"))
        .map(str::to_owned)
}
